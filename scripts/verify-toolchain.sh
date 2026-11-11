#!/usr/bin/env bash
# Gate 0.1: fail before building if the active environment is not the approved
# Holochain 0.7.1 family. Run from inside `nix develop`.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

check_prefix() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$actual" != "$expected"* ]]; then
    echo "FAIL: $name expected $expected*, found $actual" >&2
    exit 1
  fi
  echo "OK: $name -> $actual"
}

check_prefix "rustc" "rustc 1.95.0 " "$(rustc --version)"
check_prefix "holochain" "holochain 0.7.1" "$(holochain --version)"
check_prefix "hc" "hc 0.7.1" "$(hc --version)"

node - <<'NODE'
const fs = require('node:fs');
const nodeVersion = process.versions.node;
if (!nodeVersion.startsWith('24.')) {
  throw new Error(`Expected Node.js 24.x from pinned Nix flake; found ${nodeVersion}`);
}
const lock = JSON.parse(fs.readFileSync('flake.lock', 'utf8'));
if (lock.nodes?.holochain?.original?.ref !== 'holochain-0.7.1') {
  throw new Error('flake.lock does not resolve holochain-0.7.1; refresh the lock using Nix');
}
if (!lock.nodes?.holonix?.locked?.rev || !lock.nodes?.nixpkgs?.locked?.rev) {
  throw new Error('Holonix / nixpkgs are not locked to exact revisions');
}
console.log(`OK: Node.js -> ${nodeVersion}; Holochain source -> ${lock.nodes.holochain.locked.rev.slice(0, 12)}`);
NODE

npm_version="$(npm --version)"
if [[ "$npm_version" != 11.* ]]; then
  echo "FAIL: expected npm 11.x bundled in pinned Node 24; found $npm_version" >&2
  exit 1
fi
echo "OK: npm -> $npm_version"

echo "Toolchain matches Gate 0.1 requirements. The exact Node/npm revisions are fixed by flake.lock."
