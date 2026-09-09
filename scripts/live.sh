#!/usr/bin/env bash
#
# Launch the hApp live: hc-spin opens AGENTS Electron windows (default 2), each
# loading the Vite dev UI, wired to a fresh multi-agent conductor.
#
# This runs on the HOST, not inside `nix develop`, on purpose:
#   - the bundled Electron (v42) then links against your system GUI libraries
#     (the same ones VS Code / Chromium already use) instead of a headless
#     holonix shell that has none;
#   - the Holochain binaries (holochain, lair-keystore) are pulled from the
#     holonix store and put on PATH — nix binaries carry their own RPATH, so
#     they run fine from a plain shell.
#
# Usage:  ./scripts/live.sh              # 2 agents, UI on :1420
#         AGENTS=3 UI_PORT=5000 ./scripts/live.sh
#
set -euo pipefail

UI_PORT="${UI_PORT:-1420}"
AGENTS="${AGENTS:-2}"
cd "$(dirname "${BASH_SOURCE[0]}")/.."

HAPP="workdir/p2p_art_marketplace.happ"
if [ ! -f "$HAPP" ]; then
  echo "==> Building the hApp (needs the nix toolchain)…"
  nix develop --accept-flake-config --command npm run build:happ
fi

echo "==> Resolving holonix binaries…"
HC_BIN_DIRS="$(
  nix develop --accept-flake-config --command bash -c \
    'echo "$(dirname "$(command -v holochain)"):$(dirname "$(command -v lair-keystore)"):$(dirname "$(command -v hc)")"'
)"
export PATH="$HC_BIN_DIRS:$PATH"
HOLOCHAIN_BIN="$(command -v holochain)"
echo "    holochain     : $HOLOCHAIN_BIN"
echo "    lair-keystore : $(command -v lair-keystore)"

echo "==> Starting Vite dev server on :$UI_PORT …"
npm --prefix ui run dev -- --port "$UI_PORT" --strictPort &
VITE_PID=$!
trap 'kill "$VITE_PID" 2>/dev/null || true' EXIT

npx --yes wait-on "tcp:127.0.0.1:$UI_PORT"

echo "==> Launching hc-spin with $AGENTS agents…"
exec npx --yes @holochain/hc-spin \
  -n "$AGENTS" \
  --ui-port "$UI_PORT" \
  --holochain-path "$HOLOCHAIN_BIN" \
  "$HAPP"
