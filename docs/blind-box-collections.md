# Blind-box collection sales

Randomly-assigned NFT collection batches, settled via Unyt the same way
single-item sales are (see `HANDOFF.md`'s original Unyt bridge). Implemented
in `dnas/nft/zomes/integrity/nft/src/blind_collection.rs` (validation) and
`dnas/nft/zomes/coordinator/nft/src/blind.rs` (externs).

## Mechanism, in one pass

1. **Mint.** The minter calls `mint_blind_batch` with `metadata_hashes[]` (one
   commitment per slot, in slot order) and a secret `mint_seed`. This mints a
   `BlindBatchCommitment { batch_size, seed_hash }` plus `batch_size` Sealed
   `NftManifest`s, each tagged `batch` + a sequential `batch_slot`. Which real
   attributes go with which slot is implicit in this ordering — never written
   down anywhere `validate()` can see.
2. **Reserve.** A buyer calls `reserve_blind_purchase(batch)` before knowing
   anything about assignment. This generates a random `buyer_nonce`,
   encrypts it to the minter's X25519 key (see "Nonce delivery" below), and
   commits to `nonce_hash = commit_hash(buyer_nonce)` on-chain. The buyer's
   own client is responsible for holding onto the returned `buyer_nonce` —
   same pattern as a reveal salt.
3. **Payment** settles via Unyt's `nft_sale_escrow` template, same as an
   ordinary sale. `nft_id` for a blind purchase is the
   `BlindPurchaseCommitment`'s own action hash, not an NFT hash — nobody
   knows which NFT this resolves to yet.
4. **Resolve.** The minter calls `resolve_blind_assignment_from_rave` with
   the claimed settlement fields (same demo trust model as `payment.rs` —
   trusted, not re-verified against Unyt) plus `mint_seed`. This decrypts
   and checks `buyer_nonce`, computes the assigned slot
   (`blind_collection::resolve_slot` — never reimplemented coordinator-side),
   writes a `BlindAssignmentReveal`, and offers the resolved NFT via the
   existing `TransferCommitment` flow with `blind_reveal` set.
5. **Accept.** The buyer calls the existing `accept_transfer` — unchanged.

## What this does and does not claim

Pre-purchase prediction/steering (a minter tipping off a friend "buy now" to
land a chosen slot) is closed: the slot depends on the buyer's own secret
nonce, not just purchase order, and the buyer commits to (and pays for) that
nonce before learning anything about assignment.

**Two things this demo does NOT close** — both accepted as stated trust
assumptions for this pass, not fixed:

- **Resolution order.** The minter can compute `resolve_slot` themselves
  before deciding whether to write a given `BlindAssignmentReveal`, and
  controls the order purchases resolve in — which, in a linear-probe
  collision, can steer the final assignment. A closed-batch redesign
  (collect all purchases, close the batch, resolve everything in one
  deterministic pass) would remove this, but was explicitly deferred for
  this demo (see decision log below).
- **Resolution liveness.** A minter who dislikes a resolved slot can simply
  never write the reveal. `BlindPurchaseAbandoned` lets anyone flag a
  purchase as unresolved past its deadline (`blind_resolution_window_secs`),
  but **this is a UI-only signal in this implementation** — nothing on-chain
  consumes it. The UI should watch for a `BlindPurchaseAbandoned` referencing
  one of the buyer's own purchases and surface "unresolved, contact seller."
  **There is no refund/dispute mechanism**: this DNA has no way to claw back
  a Unyt payment, and a compromised/unresponsive minter leaves the buyer
  with a paid-for purchase and no assigned NFT. A real recovery path (likely
  spanning this DNA and the Unyt side) is unimplemented follow-up work, not
  a bug in what's here.

## Decision log for this pass

- **Resolution order:** accepted as a trust assumption (not the closed-batch
  redesign) — matches this demo's existing minter-trust model elsewhere.
- **Resolution liveness:** UI-only signal, no dispute/refund path.
- **Minter X25519 key discovery:** baked into `CollectionProperties.minter_x25519_pubkey`
  (hex-encoded), generated once via the `generate_minter_x25519_pubkey` extern
  and pasted into `dna.yaml` before the collection's DNA is created — same
  pre-cell-creation step as choosing `minter_pubkey`.

## Nonce delivery

`reserve_blind_purchase` generates a one-time X25519 keypair for the buyer
(`create_x25519_keypair`), box-encrypts `buyer_nonce` to the minter's
published `minter_x25519_pubkey`, and stores both the ciphertext and the
buyer's ephemeral sender pubkey in `BlindPurchaseCommitment.encrypted_nonce`
(opaque to `validate()` — only length-bounded there). `resolve_blind_assignment_from_rave`
decrypts it with the matching secret held in the minter's own lair keystore.

## Batch/purchase discovery

Neither a batch nor a purchase against it was discoverable by anything
other than an out-of-band-shared `ActionHash` in the first pass of this
design — no `PathToNft`-equivalent existed for `BlindBatchCommitment`, and
the minter had no way to see purchases reserved against their own batch.
Two new permanent link types close this, mirroring existing patterns
exactly:

- `PathToBlindBatch` (`Path("collection.all")` -> batch) — global batch
  discovery, same role as `PathToNft`. Created by `mint_blind_batch`,
  read by `get_all_blind_batches`.
- `BatchToPurchase` (batch -> purchase) — the minter's resolution queue,
  same role as `NftToListing`. Created by `reserve_blind_purchase` (which
  also sends a best-effort `BlindPurchaseReserved` remote signal to the
  minter, mirroring `ListingInterest`), read by `purchases_for_batch`.
- `resolved_purchases_for_batch` — an own-chain scan (same technique as
  `taken_slots_for_batch`) so a UI can filter already-resolved purchases
  out of the minter's queue instead of leaving them there forever.

## Grouping a batch into a Collection

`MintBlindBatchDialog` can optionally add every minted NFT to a social
Collection (the "Collections" tab — `SocialClient`'s `createCollection`/
`addToCollection`) — a separate, curated grouping from the single DNA-cell
collection every NFT here already belongs to by construction (every item
in one `mint_blind_batch` call already shares one cell and one `batch`
hash; that grouping needs no extra code). `AppState.mintBlindBatch`'s
`group` argument either creates a new Collection for the batch or adds to
an existing one; a failure here is reported as its own toast and doesn't
undo the mint, since the batch is already real either way.

## UI (`ui/src/components/MintBlindBatchDialog.svelte`, `ReserveBlindPurchaseDialog.svelte`, `ResolveBlindAssignmentDialog.svelte`, `BlindBoxesView.svelte`)

Follows the existing patterns exactly: `NftClient` methods in
`client/src/client.ts`, action-wrapper methods on `AppState`
(`ui/src/lib/appState.svelte.ts`), `<dialog>` components. `BlindBoxesView.svelte`
is a new top-level nav tab ("Blind boxes") listing every discoverable
batch, the calling agent's own batches, and — for a minter — their
resolution queue, wired to the three dialogs above. The
`generate_minter_x25519_pubkey` bootstrap step lives in
`ui/src/setup/GenerateMinterKey.svelte`, deliberately outside the main
app's routing — it's a one-time, pre-DNA-creation step, not a runtime
action any collection's UI should expose.

**Storage gap, same category as "Resolution liveness" above, at the UI
layer instead of the protocol layer:** `mint_seed` (minter) and
`buyer_nonce` (buyer) have no durable storage in this implementation
beyond `sessionStorage`, because there is no on-chain way to recover
either — only their hashes are ever committed. This is fine for a demo
running start-to-finish in one browser tab, and a real gap otherwise: a
minter who reloads their tab can no longer resolve any purchase against
that batch; a buyer who reserves on one device can't complete resolution
verification from another, or after closing the tab. A production version
needs durable, backed-up client-side storage for both secrets — same
category as `saltStore.ts`'s reveal commitments, which already solves this
problem for ordinary mints and should likely be extended to cover these
too rather than inventing a second storage mechanism.

## Known test gaps

`coordinator/nft/tests/blind.rs` covers the happy path, duplicate-purchase
rejection, batch-exhaustion rejection, and batch/purchase discovery
(`get_all_blind_batches`, `purchases_for_batch`, `resolved_purchases_for_batch`).
It does **not** cover the two
adversarial cases `blind_collection.rs`'s own test-priority notes call for
(colluding minter+buyer pre-purchase prediction; minter post-nonce
withholding) — both need a way to author a raw, off-protocol entry bypassing
`resolve_blind_assignment_from_rave`'s own bookkeeping, which this demo's
coordinator API doesn't expose. Flagged here rather than skipped silently.

## Scale caveat

`validate_create_blind_assignment_reveal` walks the minter's own chain
(`must_get_agent_activity`) to reconstruct which slots are taken — O(k) work
per reveal, paid by every validating peer, k = purchases already resolved in
the batch. Fine at a ~100-item demo batch; a redesign (a minter-signed
running commitment `validate()` checks against instead of replaying the
whole chain) is needed before this goes to a ~10,000-item target.
