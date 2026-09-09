#!/usr/bin/env bash
#
# Launch the hApp live: hc-spin opens AGENTS Electron windows (default 2), each
# loading the Vite dev UI, wired to a fresh multi-agent conductor.
#
# Runs on the HOST, not inside `nix develop`, on purpose:
#   - the bundled Electron (v42) then links against your system GUI libraries
#     (the ones VS Code / Chromium already use) instead of a headless holonix
#     shell that has none;
#   - the Holochain binaries (holochain, lair-keystore) are taken from the
#     holonix store and put on PATH — nix binaries carry their own RPATH, so
#     they run fine from a plain shell.
#
# Usage:  ./scripts/live.sh                 # 2 agents, UI on :1420
#         AGENTS=3 UI_PORT=5000 ./scripts/live.sh
#
set -euo pipefail

UI_PORT="${UI_PORT:-1420}"
AGENTS="${AGENTS:-2}"
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Ubuntu (and other distros with unprivileged-userns restrictions) refuse to run
# Electron's SUID sandbox unless node_modules/electron/dist/chrome-sandbox is
# owned by root with mode 4755. For a local dev tool the sane fix is to disable
# the sandbox rather than chase setuid perms that reset on every npm install.
export ELECTRON_DISABLE_SANDBOX=1

HAPP="workdir/p2p_art_marketplace.happ"
if [ ! -f "$HAPP" ]; then
  echo "==> Building the hApp (needs the nix toolchain)…"
  nix develop --accept-flake-config --command npm run build:happ
fi

echo "==> Resolving the holonix toolchain PATH…"
# Prepend the entire holonix PATH so hc-spin finds every binary it may spawn:
# holochain, lair-keystore, hc, and kitsune2-bootstrap-srv (the local
# bootstrap/signal server it starts for a multi-agent network).
HOLONIX_PATH="$(nix develop --accept-flake-config --command bash -c 'printf %s "$PATH"')"
export PATH="$HOLONIX_PATH:$PATH"
HOLOCHAIN_BIN="$(command -v holochain)"
for b in holochain lair-keystore hc kitsune2-bootstrap-srv; do
  printf '    %-22s %s\n' "$b" "$(command -v "$b" || echo 'NOT FOUND')"
done

# Free the UI port if a previous run left something on it.
if command -v fuser >/dev/null 2>&1; then
  fuser -k "${UI_PORT}/tcp" 2>/dev/null && sleep 1 || true
fi

echo "==> Starting Vite dev server on :$UI_PORT …"
npm --prefix ui run dev -- --port "$UI_PORT" --strictPort &
VITE_PID=$!
cleanup() { kill "$VITE_PID" 2>/dev/null || true; }
trap cleanup EXIT INT TERM

npx --yes wait-on -t 60000 "tcp:127.0.0.1:$UI_PORT"

echo "==> Launching hc-spin with $AGENTS agents…"
npx --yes @holochain/hc-spin \
  -n "$AGENTS" \
  --ui-port "$UI_PORT" \
  --holochain-path "$HOLOCHAIN_BIN" \
  "$HAPP"
