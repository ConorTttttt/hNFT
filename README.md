# P2P Art Marketplace — Holochain NFT hApp

Agent-centric NFT hApp. Design A (no countersigning) from
[nft-happ-architecture.md](nft-happ-architecture.md). Runs on stock holonix /
Kangaroo conductors — no `unstable-countersigning`, no custom conductor.

- **Identity** = the mint Create `ActionHash`. No `token_id`.
- **Ownership** = the `to_agent` of the highest-`sequence` accepted `TransferReceipt`
  in the `NftToReceipt` provenance chain. `AgentToNft` is a derived lookup index only.
- **Transfer** = two independent entries: `TransferCommitment` (seller) +
  `TransferReceipt` (buyer), with an action-timestamp window making accept-vs-cancel
  deterministic in `validate()`.
- **Blind mint** = commit-reveal. `NftManifest` mints `Sealed`; `reveal_nft` does one
  `update_entry` to `Revealed`, checked against the commitment in `validate()`.
- **Collection config** (`name` / `symbol` / `max_supply` / `minter_pubkey` /
  `transfer_window_secs`) lives in DNA `modifiers.properties`; one clone cell per
  collection.
- **`max_supply`** is enforced only with a single `minter_pubkey` (progenitor
  pattern) — a lone minter's chain is linear, so `validate_create_nft_manifest`
  counts prior mints with `must_get_agent_activity`. `minter_pubkey: null` = open
  minting, cap not enforced (dev only).

## Layout

```
dnas/nft/zomes/
  integrity/nft/src/
    lib.rs             EntryTypes, LinkTypes, validate() dispatcher
    dna_properties.rs  CollectionProperties (read via dna_info())
    nft_manifest.rs    NftManifest + commit-reveal + max_supply validation
    transfer.rs        TransferCommitment / Receipt / Cancellation validation
    links.rs           link-type validation (incl. AgentToNft evidence-tag check)
  coordinator/nft/src/
    lib.rs             init() cap grant, post_commit signals, RemoteSignal
    mint.rs  reveal.rs  transfer.rs  query.rs
  coordinator/nft/tests/
    common.rs  flows.rs   (mint+reveal, full transfer, max_supply cap spike)
```

## Build & test

```bash
nix develop

# zomes -> wasm  (RUSTFLAGS is REQUIRED or the build fails inside getrandom)
RUSTFLAGS='--cfg getrandom_backend="custom"' \
  cargo build --release --target wasm32-unknown-unknown

hc app pack workdir --recursive        # -> workdir/p2p_art_marketplace.happ
cargo test                             # Sweettest suite (needs the .dna packed)
```

`npm run build:happ` / `npm test` wrap the above.

> Outside `nix develop` (host Rust toolchain) the wasm link step may report
> `undefined symbol: __hc__*` — those host fns are provided by the conductor.
> Add `-C link-arg=--allow-undefined` to `RUSTFLAGS` for a local smoke build, or
> just build inside `nix develop` where the stock command works.

## Verification status

- ✅ `cargo check` — both `nft_integrity` and `nft` compile clean against
  `hdi = 0.8.0` / `hdk = 0.7.0` (no warnings).
- ✅ `cargo build --target wasm32-unknown-unknown` — both zomes link to wasm
  (stock command inside `nix develop`).
- ✅ **Sweettest — 13/13 passing** (`nix develop`, holochain 0.7.0).

  `tests/flows.rs` (7):
  - `mint_and_reveal` — commit-reveal accepts the true preimage, rejects a wrong salt.
  - `full_transfer_updates_ownership_and_provenance` — commitment→receipt flips
    `current_owner` to the buyer, provenance = one hop (seq 1), seller can't re-offer.
  - `two_hop_transfer_chains_provenance` — A→B→C; `sequence` 1→2, agents chain, a
    past owner can't re-offer.
  - `max_supply_cap_is_enforced_for_a_single_minter` — the `must_get_agent_activity`
    walk + entry-type filter reject the (cap+1)-th mint.
  - `reveal_is_minter_only_and_single_shot` — non-minter reveal rejected; second
    reveal (branched off the first reveal) rejected.
  - `transfer_window_bounds_accept_and_cancel` — cancel-before-window rejected,
    cancel-after-window accepted, freed state re-offered and accepted.
  - `accept_after_window_is_rejected` — buyer's accept past the window rejected.

  `tests/edge.rs` (6):
  - `wrong_agent_cannot_accept` — non-recipient's accept fails; named buyer's succeeds.
  - `double_accept_is_rejected` — a commitment can't be accepted twice.
  - `drop_stale_ownership_link_removes_only_the_sellers_index_entry` — dropping the
    stale index link leaves the buyer's view and the provenance chain untouched.
  - `provenance_visible_to_an_uninvolved_third_party` — a non-party reads
    provenance / owner / wallet from the DHT.
  - `cancel_by_non_offerer_is_rejected` — only the offerer may cancel, even after the window.
  - `reveal_cannot_be_repeated_by_branching_off_the_mint` — a second Update branched
    off the sealed mint is rejected.

Every architecture-doc §7 item is confirmed at runtime: the `must_get_agent_activity`
+ `is_create_of` cap walk, `ChainFilter::new` prefix extent, `Timestamp::as_micros`
window comparison (accept vs cancel deterministically disjoint), and the
`LinkTag` ⇄ `ActionHash` (`try_from_raw_39`) round-trip.

The commit-reveal preimage is a hand-rolled length-prefixed encoding (not
`holochain_serialized_bytes`), so the TypeScript client reproduces it exactly —
see `client/`.

Run the suite with `--test-threads=1` — the window tests use real `sleep`, so the
full run takes ~4 min.

## TypeScript client

[`client/`](client/) — typed `@holochain/client` wrappers plus the commit-reveal
hashing the UI needs. `cd client && npm install && npm run typecheck`. See
[client/README.md](client/README.md).

## Design B (countersigning) — not implemented

Kept as the upgrade path in the architecture doc §4, for if a future requirement
needs host-level atomicity (e.g. atomic NFT-for-payment). It requires a conductor
built with `unstable-countersigning`.
