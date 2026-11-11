# Reproducible hNFT development environment (Gate 0.1)

> Target: Holochain **0.7.1**, HDK **0.7.1**, HDI **0.8.1**,
> `@holochain/client` **0.21.1**. All commands below use the checked-in
> lockfiles. Do not upgrade dependency graphs as part of routine builds.

## Prerequisites

- Linux or macOS with Git and Nix with `nix-command` and `flakes` enabled.
- Internet access on the **first** build to fetch pinned sources and package
  registries. Subsequent builds use the Nix and package caches.
- Enough disk/RAM for compiling the Holochain conductor for Sweettests.
- An X11/Wayland desktop if you plan to run `npm start` with `hc-spin`.
  Headless compilation and Sweettests do not need Electron.

Nix is the single source of truth for the native toolchain: the committed
`flake.lock` fixes the Holonix, Holochain and nixpkgs source revisions.
Holonix's **0.7.1** shell uses Rust **1.95.0** (also in
`rust-toolchain.toml`), and hNFT selects `nodejs_24` from its pinned nixpkgs.
The exact Node.js patch release and the bundled npm release come from that
immutable nixpkgs revision; `package.json` additionally rejects other major
versions (Node 24 / npm 11). Npm dependency versions are fixed by each
committed `package-lock.json`.

## Clean checkout and build

```bash
git clone https://github.com/ConorTttttt/hNFT.git
cd hNFT
git checkout gate/s0-01-holochain-0.7.1  # until this gate merges

# If necessary, enable Nix experimental features in nix.conf, or:
nix --extra-experimental-features "nix-command flakes" develop --accept-flake-config

# All remaining commands run INSIDE the Nix shell:
npm run verify:toolchain
cargo check --locked
npm ci
npm --prefix client ci
npm --prefix client run build
npm --prefix ui ci
npm --prefix ui run build
npm run build:happ
cargo test --locked -- --test-threads=1
```

`npm run build:happ` compiles the integrity/coordinator WASM zomes,
packs `dnas/nft/workdir/nft.dna`, and packs the `.happ` file.
The existing Sweettests require this DNA bundle, so don't skip this step.
`npm test` is the equivalent combined hApp-build + Sweettest command.
For the complete UI build, use `npm run build:ui` after installing both
`client` and `ui` dependencies. The separate client build is intentional:
the UI imports the locally built TypeScript client.

`npm start` invokes `scripts/live.sh` to start Vite and `hc-spin`.
This is an optional GUI smoke test, not a replacement for Sweettests.

## Capture local gate evidence

```bash
mkdir -p gate-evidence
set -o pipefail
npm run verify:toolchain 2>&1 | tee gate-evidence/toolchain.log
cargo check --locked 2>&1 | tee gate-evidence/cargo-check.log
npm --prefix client run build 2>&1 | tee gate-evidence/client-build.log
npm --prefix ui run build 2>&1 | tee gate-evidence/ui-build.log
npm run build:happ 2>&1 | tee gate-evidence/happ-build.log
cargo test --locked -- --test-threads=1 2>&1 | tee gate-evidence/sweettests.log
```

The gate GitHub Actions workflow uploads the same categories of log as an
artifact and records the final commit SHA in the run. Keep a link to a passing
run in [Gate 0.1 compatibility report](gates/0.1-dependency-compatibility.md)
before a GO decision. Log files must not be committed to the repository.

## Updating versions deliberately

Do not run `cargo update`, `npm update` or `nix flake update` during normal
development. To propose a dependency upgrade, update the relevant manifests
on a dedicated branch and regenerate all affected locks inside `nix develop`:

```bash
nix flake update holonix
nix develop --accept-flake-config
cargo update
npm install --package-lock-only --ignore-scripts
npm --prefix client install --package-lock-only --ignore-scripts
npm --prefix ui install --package-lock-only --ignore-scripts
```

Then repeat the clean-room `ci` / `--locked` verification and commit the
changed manifests and lockfiles together. The Gate 0.1 workflow performs
this refresh once on its dedicated bootstrap branch.

## Important compatibility notes

- Upgrading an **integrity zome** changes the compiled DNA hash; peers on
  different DNA hashes are on different networks. Do not reuse a production
  collection's identity across this upgrade without a migration decision.
- Holochain **0.7.1** changed capability-grant encoding. Existing grants
  from **0.7.0** cannot be read and must be re-issued (for disposable local
  conductor environments, recreate the app from scratch).
- hNFT's `recv_remote_signal` is a **remote zome call**, not Holochain's new
  **direct-signal** API. It uses a zome-call grant; no direct-signal grant is
  needed unless direct signals are explicitly implemented later.
- The Sweettests include real-time window tests and may take several minutes.
  If a test fails, preserve the exact log and classify as dependency/API,
  behavioral regression, stale test assumption, environment-only, or flaky
  timing before changing production logic.
- Use [the gate report](gates/0.1-dependency-compatibility.md) for the
  outstanding items and the GO/NO-GO decision.
