# Blind fixed-inventory batches (G3a)

A minter commits a **fixed inventory** of sealed NFTs. Buyers reserve purchases. After the minter closes the batch, randomness decides **which committed NFTs are sold** and **which purchase receives which NFT**. Traits and artwork stay concealed (sealed commit-reveal) until the assigned NFT is revealed.

This is the only blind protocol. It replaces the earlier `mint_seed` / `buyer_nonce` / `BlindAssignmentReveal` design, which was order-dependent and is gone (no migration: the DNA hash changed).

It is separate from *open generative collections* (randomness determines traits; see [generative-collections.md](generative-collections.md)). A collection cannot be both: a batch NFT in a collection with `generation` set is rejected.

> **The only randomness verifier is `dev-insecure`, which is insecure by design.** It accepts a public, forgeable "proof" and exists only to exercise the protocol. No production randomness provider has been chosen or implemented; see [randomness-verification.md](randomness-verification.md) for the verification boundary.

Code: `dnas/nft/zomes/integrity/nft/src/blind_collection.rs` (protocol, validators, pure functions), `randomness.rs` (`RandomnessSpec`, `RandomnessResult`, its validator, generation seed), `randomness_verifier.rs` (verification boundary), `canonical.rs`; coordinator `dnas/nft/zomes/coordinator/nft/src/blind.rs`.

## N and K

- **N** = committed inventory size (`batch_size`, `1 ≤ N ≤ 1024`).
- **K** = purchases included in the `BatchClose`, with **`1 ≤ K ≤ N`**.

Example: `N = 10`, `K = 7`. Seven of the ten committed NFTs are selected at random and assigned at random to the seven purchases. The other three stay locked.

## Lifecycle and objects

```text
BlindBatchCommitment            batch_id = its Create ActionHash
  └─ N sealed NftManifest        (batch = batch_id, batch_slot = i)
       └─ InventoryFinalized     nfts[i] == the NFT for slot i
            └─ BlindPurchaseCommitment × any   (cite the finalization)
                 └─ BatchClose       (minter; sorted purchase hashes; 1 ≤ K ≤ N)
                      └─ RandomnessResult    (bound to the close; anyone may author)
                           └─ deterministic partial Fisher–Yates   (derived; no entry)
                                └─ BatchRelease in the first TransferCommitment
                                     └─ TransferReceipt ── Reveal
```

The batch commitment cannot contain NFT hashes (the NFTs do not exist yet), so the binding is two-stage: batch → sealed NFTs → `InventoryFinalized`. Purchases cite the finalization, so no purchase can exist before the inventory is fixed.

| State | Meaning | Entry or derived |
|---|---|---|
| DRAFT | batch exists, inventory not finalized | derived |
| SALES_OPEN | `InventoryFinalized` exists | entry, then derived |
| CLOSED | `BatchClose` exists | entry |
| RANDOMNESS_FIXED | a valid `RandomnessResult` for the close exists | entry |
| ASSIGNED | assignment is computable | **derived only** (no assignment entry) |
| TRANSFERRED | first transfer used a valid `BatchRelease` | existing transfer entries |
| REVEALED | sealed → revealed update | existing reveal |

Discovery links (`PathToBlindBatch`, `BatchToPurchase`, `BatchToInventory`, `BatchToClose`, `CloseToResult`) exist for the coordinator and client only. **Validation never enumerates links**; it works from directly referenced records and deterministic computation.

## What is validated

- **`BlindBatchCommitment`**: minter authority; `1 ≤ batch_size ≤ 1024`; `assignment_algorithm_version == 1`; valid `collection_id`; no `generation` config. `sale_window_secs` is **advisory only** and never used by validation.
- **`NftManifest` with a batch**: `batch` and `batch_slot` both set or both unset; the batch exists and is a `BlindBatchCommitment`; same author; `slot < batch_size`; sealed; non-zero metadata hash; collection not generative. Mint-time uniqueness is *not* attempted: a twin NFT for a slot can be minted but is never usable.
- **`InventoryFinalized`**: author is the batch author; `nfts.len() == batch_size`; all NFT hashes distinct; each is the **original Create** (not an Update) of a sealed manifest by the same author, in this batch, at that index, with non-zero metadata hash; the digest recomputes; and no earlier finalization for this batch exists **on the author's own chain**.
- **`BlindPurchaseCommitment`** `{batch, inventory}`: the inventory is a valid finalization of that batch. Any agent may purchase, several times.
- **`BatchClose`**: author is the batch author; `1 ≤ K ≤ N`; purchase hashes strictly ascending by raw 39-byte `ActionHash` (so no duplicates); each purchase exists and cites this batch and inventory; no earlier close on the author's chain. **No timestamps.**
- **`RandomnessResult`**: bounds (`provider_id ≤ 64`, `event_id ≤ 64`, `proof ≤ 4096`); validation then runs the verification boundary (provider identity, binding to the close, provider proof; see [randomness-verification.md](randomness-verification.md)). Anyone may author one.
- **First transfer of a batch NFT**: `batch_release` is **mandatory** (and forbidden for non-batch NFTs). Validation reconstructs NFT → batch/slot → `InventoryFinalized` (must list this NFT at that slot) → `BatchClose` → `RandomnessResult` (bound to the close) → assignment → purchase, and requires `to_agent == purchase author`. Ordinary `initiate_transfer` of a batch NFT is rejected.

## What this does and does not guarantee

- The close **commits one immutable purchase set**. It does **not** prove every economically eligible purchase was included: purchases live on independent buyer chains and validators cannot enumerate the DHT. **The minter controls the authoritative close and can omit purchases.** Payment and refund consequences are deferred.
- Once the randomness result is fixed, **the assignment cannot be changed**: it is a pure function of the committed inventory, the sorted purchases and the randomness. Resolution order has no effect.
- Uniqueness of the finalization and of the close holds on an **honest linear minter chain**. A forked author chain can present two of them: Holochain 0.7 defines a `ChainFork` warrant to detect this, but validating one branch cannot see the other.
- **No timestamps** are used to close a batch or release an NFT. Action timestamps are author-asserted (Holochain only requires them to be non-decreasing on a chain), so they are not a consensus input here.
- **Unselected inventory** (`N − K` NFTs) has no release path in G3a and stays locked. Recovery of unsold inventory, and of batches that never close, is deferred.
- The minter can also pad a batch with purchases from other keys; purchases are free at the protocol level.
- Provider non-equivocation (one result per close) is a provider property. Several valid `RandomnessResult` entries for one close can exist when authored by different agents; the validator does not enforce uniqueness across authors.

## Canonical hashing (frozen)

All G3 hashes are blake2b-256 over `put(tag) ‖ u32_le(version) ‖ fields`, where `put(x) = u32_le(len(x)) ‖ x`, integers are little-endian, and an `ActionHash` is its raw 39 bytes. (The G1 category roll in `generation.rs` predates this form and is frozen separately.)

```text
inventory leaf   put("hnft.inventory.leaf") ‖ u32 1 ‖ collection_id[32] ‖ batch[39]
                 ‖ u32 slot ‖ nft[39] ‖ metadata_hash[32]
inventory digest put("hnft.inventory.digest") ‖ u32 1 ‖ collection_id[32] ‖ batch[39]
                 ‖ u32 N ‖ leaf_0 ‖ … ‖ leaf_{N-1}
assignment seed  put("hnft.assignment.seed") ‖ u32 1 ‖ collection_id[32] ‖ batch[39]
                 ‖ close[39] ‖ put(provider_id) ‖ put(event_id) ‖ randomness[32]
generation seed  put("hnft.generation.seed") ‖ u32 1 ‖ collection_id[32] ‖ nft[39]
                 ‖ u32 generation_version ‖ put(provider_id) ‖ put(event_id) ‖ randomness[32]
```

`collection_id` is an explicit stable namespace: **64 lowercase hex characters** in DNA properties, decoding to exactly 32 bytes. It is not the DNA hash (which changes on migration).

### Assignment (algorithm version 1)

1. Word stream: `word_j = u64_le(blake2b_256(assignment_seed ‖ u32_le(j))[0..8])`. `j` advances on **every** draw, including rejected ones.
2. `uniform(m)`: `m == 1` returns 0 without drawing; otherwise draw until `word < 2^64 − (2^64 mod m)`, then return `word mod m` (no modulo bias).
3. Partial Fisher–Yates over `[0, N)`: `perm = [0..N)`; for `j` in `0..K`, `r = j + uniform(N − j)`, swap `perm[j]`, `perm[r]`; the purchase at sorted position `j` gets slot `perm[j]`.

Positions are the sorted purchase order, so the result is injective, exactly `K` NFTs are selected, and the slot at position `j` does not depend on `K`.

### Golden vectors

Computed independently with Python `hashlib.blake2b(digest_size=32)` and pinned in the Rust tests. Synthetic inputs: `collection_id = 0xC0 × 32`; batch = `84 29 24 ‖ 0xB0×32 ‖ 00 00 00 01`; NFT *i* = `84 29 24 ‖ (0xA0+i)×32 ‖ 00 00 00 i`; metadata *i* = `(0x30+i)×32`.

| Vector | Value |
|---|---|
| leaf, slot 0 | `473ac37f6831f7bad8bc0f2dac4f409422a1c7744b81a4047e1f6da82db01b98` |
| leaf, slot 1 | `7aae4e39becebe53bac972d923789b2da2c386b8ff6ba792a01869b5062f6e94` |
| digest, N = 2 | `5761aa62285638f6a9fe55c610ddc648bd84276635fa92a7b9136d1a7050a78c` |
| digest, N = 1 | `d4d4f4c6a51caa85b4017a9c1b2b7cb6df081fedde62cdfe930aab413e984a29` |
| assignment seed | `fb7e64d9aa2fe672ae3fc9316b32f41d8e43e25263753cf030f8ce2a8c30b568` |
| slots, N = 5, K = 3 | `[4, 0, 3]` |
| slots, N = 5, K = 5 | `[4, 0, 3, 1, 2]` |
| generation seed | `a74698536ffcad8661ecd13bc4fae8371370c2fb9a56e22a7b8885d7fcd7937c` |

The assignment and generation vectors use `close = 84 29 24 ‖ 0xD0×32 ‖ 00 00 00 02`, `provider_id = "dev-test"`, `event_id = "event-1"`, `randomness = 0x5A × 32`; the generation seed uses NFT *0* and `generation_version = 1`.

## Randomness verification boundary (G4)

Acceptance of a `RandomnessResult` goes through a provider-independent boundary (`randomness_verifier.rs`): `verifier_for(provider_id)` selects a verifier, which deterministically checks the proof and the binding to the close and returns a typed result (`InsecureDev` or `ProviderVerified`) or a typed error. Validation performs no network access. Only `dev-insecure` is registered (`InsecureDev`, not production-grade). G4 changed no entry or property schema and did not choose a production provider. Full description: [randomness-verification.md](randomness-verification.md).

## Configuration

```yaml
properties:
  collection_id: "<64 lowercase hex chars>"     # required for blind batches
  randomness:
    provider_id: "dev-insecure"                  # dev/test only
    verifier_params: ""
```

`genesis_self_check` rejects an invalid `collection_id` or `RandomnessSpec`.

## Batch size

The protocol maximum is `MAX_BATCH_SIZE = 1024` (a full inventory list of about 43 KB, against Holochain's 4 MB entry limit). The UI and coordinator default to **≤ 256**, and CI tests use small batches. Validation of a finalization and of a close each fetch up to N records; that cost was **not measured at N = 1024**, so measure it before raising operational defaults. Merkle commitments are not used in v1.

## UI

`BlindBoxesView` lists batches with their derived state. A minter can close a batch, record **dev** randomness (labelled insecure), and release assigned NFTs; buyers can reserve a purchase and see the assignment. `mint_seed`, `buyer_nonce`, the minter X25519 key setup page and the resolve dialog were removed.

## Deferred

A production randomness provider and its proof verifier (the verification boundary exists; no real provider is registered); payment escrow, atomicity and refunds; buyer-pull transfer; recovery of unsold or never-closed batches; blind-generative secrecy; Merkle scaling; media binding.
