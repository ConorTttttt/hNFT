#!/usr/bin/env bash
# Reproducible verification for the pinned hNFT source commit.
# "typescript" requires Node/npm and preinstalled client+ui dependencies.
# "holochain" requires the repository's Nix dev shell (Holochain 0.7 + Rust).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

SOURCE_SHA="8fd2391e2f9f584c19dcb329bb21bc034f1bf19e"
MODE="${1:-all}"

echo "hNFT baseline candidate"
echo "Source commit: $SOURCE_SHA"
echo "Checked-out commit: $(git rev-parse HEAD)"

git cat-file -e "${SOURCE_SHA}^{commit}"
git merge-base --is-ancestor "$SOURCE_SHA" HEAD || {
  echo "ERROR: checkout does not descend from pinned source commit" >&2
  exit 1
}

# Only baseline evidence and CI files may differ from the pinned application.
changed="$(git diff --name-only "$SOURCE_SHA" HEAD -- . |
  grep -Ev '^(\.github/workflows/verify-baseline\.yml|docs/verification/baseline-2026-10-28\.md|scripts/verify-baseline\.sh)$' || true)"
if [[ -n "$changed" ]]; then
  echo "ERROR: application differs from pinned source:" >&2
  echo "$changed" >&2
  exit 1
fi
echo "Source-tree integrity: PASS (application bytes unchanged from pinned commit)"

check_typescript() {
  echo "==> Node $(node --version), npm $(npm --version)"
  npm --prefix client run typecheck
  npm --prefix client run test
  npm --prefix ui run check
  npm --prefix ui run test
  npm --prefix ui run build
}

check_holochain() {
  echo "==> Rust $(rustc --version), Cargo $(cargo --version), Node $(node --version)"
  hc --version
  npm run build:happ
  cargo test -- --test-threads=1
  echo "==> SHA-256 of built artefacts"
  sha256sum dnas/nft/workdir/nft.dna dnas/social/workdir/social.dna workdir/p2p_art_marketplace.happ
}

case "$MODE" in
  typescript) check_typescript ;;
  holochain)  check_holochain ;;
  all)        check_typescript; check_holochain ;;
  *) echo "Usage: $0 [typescript|holochain|all]" >&2; exit 2 ;;
esac
echo "Verification stage $MODE: PASS"
