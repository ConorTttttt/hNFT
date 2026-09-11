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

# Electron's SUID sandbox needs node_modules/electron/dist/chrome-sandbox owned
# by root with mode 4755, or Ubuntu's unprivileged-userns restriction aborts the
# launch. Disabling the sandbox instead (ELECTRON_DISABLE_SANDBOX=1) *also* stops
# hc-spin's preload from injecting window.__HC_LAUNCHER_ENV__, so the UI can't
# find the conductor. So: use the real sandbox when it's set up, and if it isn't,
# tell the user how to fix it (one-time sudo) rather than silently disabling it.
CHROME_SANDBOX="$PWD/node_modules/electron/dist/chrome-sandbox"
if [ -e "$CHROME_SANDBOX" ] && [ "$(stat -c '%u %a' "$CHROME_SANDBOX")" != "0 4755" ]; then
  cat >&2 <<EOF

  Electron's sandbox helper isn't set up. Run this once (needs sudo), then retry:

    sudo chown root:root "$CHROME_SANDBOX"
    sudo chmod 4755 "$CHROME_SANDBOX"

  (Re-run after any 'npm install' under node_modules/electron.)
  To skip the sandbox instead — the UI then can't connect — set
  HC_SPIN_NO_SANDBOX=1 and expect to pass --admin-port / VITE_HC_ADMIN_PORT.

EOF
  [ "${HC_SPIN_NO_SANDBOX:-}" = "1" ] || exit 1
fi
[ "${HC_SPIN_NO_SANDBOX:-}" = "1" ] && export ELECTRON_DISABLE_SANDBOX=1

# If this shell is confined by a snap's AppArmor profile (e.g. it's a
# terminal launched from inside the VS Code *snap*), Electron's sandboxed
# child processes inherit that confinement, and unprivileged user-namespace
# creation can end up blocked in a way this bundled Electron doesn't handle
# gracefully — it hits an internal fatal check and dies (SIGTRAP, sometimes
# SIGSEGV) before ever opening a window, independent of the chrome-sandbox
# setuid fix above. Confirmed on this exact symptom; the fix is to run
# `npm start` from a terminal that isn't under that confinement (a plain
# terminal app, not the snap's integrated one) — there's no in-script
# workaround for another program's AppArmor profile.
if [ -r /proc/self/attr/current ] && grep -q '^snap\.' /proc/self/attr/current 2>/dev/null; then
  cat >&2 <<EOF

  Heads up: this shell is confined by $(cat /proc/self/attr/current 2>/dev/null)'s
  AppArmor profile. That's been seen to crash the bundled Electron on launch
  (SIGTRAP/SIGSEGV, no window ever opens) independent of the chrome-sandbox
  setup above. If hc-spin crashes below, try running 'npm start' again from a
  terminal that isn't inside that snap's confinement (e.g. a plain terminal
  app instead of an editor's integrated one).

EOF
fi

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
