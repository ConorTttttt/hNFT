# Generative collections (G1 foundation)

> **The generator is deterministic. The eventual security and unpredictability of generative NFT minting depends on the mechanism used to produce the generation seed.**
> G1 provides deterministic generation from a seed. **From G5a the seed is never supplied**: it is derived from verified per-NFT randomness (see the G5a section below). The only verifier that exists is the insecure `dev-insecure` scheme, so no secure randomness is provided yet, and no production randomness provider is chosen or integrated. A frontend-generated random number must never become authoritative.

Code: `dnas/nft/zomes/integrity/nft/src/generation.rs` (pure Rust, no HDK calls, unit-tested natively with `cargo test -p nft_integrity`).

## Structure

A collection's trait catalogue lives in the existing per-collection config, `CollectionProperties.generation: Option<GenerationConfig>` (DNA `modifiers.properties`; `#[serde(default)]`, so existing collections deserialize unchanged and `None` = not generative). `CollectionProperties::generation_config()` returns it validated.

```text
GenerationConfig { generation_version: u32, categories: Vec<TraitCategory> }
TraitCategory    { id, display_name, traits: Vec<TraitOption> }
TraitOption      { id, display_name, weight: u32 }
```

- `id`s are the stable logical identity (non-empty, ≤64 bytes, unique within scope); `display_name` may change freely.
- Weights are relative integers (need not sum to 100). A `0` weight is never selected; a category's total must be > 0.
- Compatibility rules (e.g. "laser_eyes excludes sunglasses") are **not** implemented; they can later be added as new `#[serde(default)]` fields.

## Generation v1

`generate(config, seed) -> GeneratedTraits { generation_version, traits: [{category_id, trait_id}] }`, one trait per category in config order. Per category:

1. `roll = u64_le(blake2b_256("hnft.generation.v1" ‖ seed[32] ‖ len(category_id) u32 le ‖ category_id)[0..8])`
2. `point = roll % total_weight`
3. pick the first trait whose cumulative weight exceeds `point` (integer arithmetic only).

The roll depends on the category id, not its index, so reordering categories doesn't change picks. No clock, OS/browser randomness, or network is used. `generation_version` (only `1` exists) is stored in every result and an unsupported version is rejected; v1 rules are frozen once shipped so old NFTs remain reproducible.

`GeneratedTraits::to_attributes()` maps a result onto the existing `NftAttribute { trait_type, value }` (category id, trait id), so it fits the current reveal payload with no manifest change.

## Separation of concerns

```text
randomness layer (G4/G5a boundary; only the insecure dev verifier exists) -> seed -> generator -> traits -> {metadata, artwork}
```

- **Randomness:** the generator takes an opaque `GenerationSeed([u8; 32])` and knows nothing about its origin (QRNG, oracle, Holochain-derived, ...).
- **Artwork/media:** results contain no image URI, no IPFS reference. External storage (traits -> artwork -> IPFS) and a future fully on-chain renderer (seed + rules -> deterministic renderer -> artwork) are both possible, because nothing here depends on either.
- **NFT identity:** unchanged — the mint `ActionHash`. Ownership, transfers, Unyt bridge and the commit-reveal preimage are untouched.

## Intended lifecycle (not wired up in G1)

mint/commit (sealed) -> randomness fixed -> derive seed -> `generate` -> build metadata/artwork -> reveal.

The existing blind mint/reveal (`NftPayload::Sealed -> Revealed`, `reveal_preimage`) is not modified.

## G2: generation provenance and reveal verification

Golden vectors in `generation.rs` pin the exact v1 byte-level algorithm (expected rolls were computed independently with Python `hashlib.blake2b`).

**`GenerationRecord`** (`integrity/nft/src/generation_record.rs`) is a separate, immutable entry, linked from the mint action by `NftToGeneration`:

```text
GenerationRecord { nft, generation_version, seed: GenerationSeed,   // seed is DERIVED (G5a), never supplied
                   generated_traits: GeneratedTraits, randomness: RandomnessProvenance }
RandomnessProvenance = Verified { result: ActionHash }   // G5a: the only form; Unattested/External were removed
```

`NftPayload`, `Revealed`, the commit-reveal preimage and `client/src/commitment.ts` are unchanged.

**Lifecycle (G5a):** mint (sealed) -> minter records verified randomness (`create_generation_randomness`) -> `create_generation_record(nft, result)` -> `reveal_generated` (or `reveal_nft` with exactly the generated attributes).

**Validation (`validate()`):**
- Record: collection is generative with a valid config; `generation_version` equals the config's; `generated_traits == generate(config, seed)`; provenance well-formed; `nft` is an `NftManifest` mint; authored by that NFT's minter; the minter's chain has no earlier record for this NFT and no reveal yet (single-shot, precedes reveal).
- `NftToGeneration` link: base matches the record's `nft`; permanent.
- Reveal of a generative collection: minter only (delegated reveals are not supported for generative collections in G2); the minter's chain must hold a record for the NFT; the record is re-checked against the config; attributes must equal `generate(config, record.seed).to_attributes()` exactly (order and multiplicity included). Caller-supplied traits are never trusted.
- Collections with `generation == None` reveal exactly as before.
- `genesis_self_check` refuses to create a cell whose generation config is invalid.

**Commitment interaction (important):** for a generative collection the mint commitment cannot bind the traits, because they do not exist until the seed is fixed. So the preimage check (`blake2b(salt ‖ attributes ‖ image_uri) == metadata_hash`) is *replaced* by the generation check for these collections; `metadata_hash` is only a non-zero placeholder. Consequences: traits are bound by the immutable record; **`image_uri` and `salt` are not bound by anything** in a generative reveal, so the minter can choose the media at reveal time. Binding artwork to traits is an open decision (see below).

**Randomness boundary:** superseded by G5a. There is no `SeedSource`/`SuppliedSeed` and no seed input; see below.

## Open items for review (from G1; some now addressed)

- `NftPayload::Revealed` has nowhere to record `generation_version` or a seed reference. Adding one changes the entry shape (and, if bound into the commitment, the preimage that `client/src/commitment.ts` mirrors) — needs an explicit decision.
- Cross-language use (TS client reproducing `generate`) requires the same blake2b + encoding; not implemented.
- (Resolved in G5a) The seed is derived from verified per-NFT randomness, not supplied.
- Editing integrity code changes the nft DNA hash; existing cells/clones will not match a rebuilt DNA.

## G3a: canonical generation seed

`randomness::generation_seed` derives the `GenerationSeed` fed to the frozen G1 `generate` from a randomness result:

```text
put("hnft.generation.seed") ‖ u32_le(1) ‖ collection_id[32] ‖ nft[39]
‖ u32_le(generation_version) ‖ put(provider_id) ‖ put(event_id) ‖ randomness[32]     (blake2b-256)
```

It is pinned by a golden vector (`a74698536ffcad86…d7937c`, computed independently in Python). It is wired into `GenerationRecord` validation in G5a, and no production randomness provider exists (the only verifier is the dev-only, insecure `dev-insecure` scheme; see [randomness-verification.md](randomness-verification.md)). The G1 category roll is unchanged and frozen.


## G5a: verified per-NFT randomness for generative NFTs

```text
NFT mint -> GenerationRandomness (bound to the NFT, verified locally)
         -> generation_seed(...)   (frozen encoding above)
         -> generate(...)          (frozen G1)
         -> GenerationRecord       (cites the randomness; seed is derived)
         -> reveal_generated
```

**`GenerationRandomness`** `{ nft, provider_id, event_id (≤ 64), randomness[32], proof (≤ 4096) }`. The collection id and generation version are not stored: they come from the cell's DNA properties.

**Verification** is an additive path parallel to (and not modifying) the frozen G4 batch boundary: `generation_randomness.rs` has `GenerationVerificationContext { collection_id, nft, generation_version }`, a `GenerationRandomnessVerifier` trait, a `generation_verifier_for(provider_id)` registry, typed errors (`Malformed`, `ProviderMismatch`, `UnsupportedProvider`, `ProofRejected`, `BindingMismatch`) and a `VerificationLevel`. It is deterministic and performs no network access. It never uses a `BatchClose`.

**`dev-insecure`** is the only verifier. Its proof is a public, forgeable hash, so anyone (including the minter) can produce a valid proof for any randomness. It is not production-grade (`InsecureDev`):

```text
put("hnft.dev-insecure.generation-proof") ‖ u32_le(1) ‖ collection_id[32] ‖ nft[39]
‖ u32_le(generation_version) ‖ put(provider_id) ‖ put(domain) ‖ put(event_id) ‖ randomness[32]   (blake2b-256)
```

`verifier_params` is empty or `domain:<name>` (same rule as the G4 dev verifier).

**Validation.**
- `GenerationRandomness`: minter-authored (the NFT's mint author), at most one per NFT on the minter's own chain, never after a reveal; the collection must be generative with a `RandomnessSpec` and `collection_id`; the provider must match the spec and the proof must verify for exactly this collection, NFT and generation version.
- `GenerationRecord`: must cite a `GenerationRandomness` for the same NFT from the configured provider, and `seed == generation_seed(collection_id, nft, generation_version, provider_id, event_id, randomness)`, `generated_traits == generate(config, seed)`. A minter-chosen seed is rejected.
- Reveal: unchanged (attributes must equal `generate(config, record.seed)`), and impossible without a record.

**Ordering** is causal, not timestamp-based: the randomness references the NFT's mint hash, the record references the randomness, and the reveal follows the record on the minter's chain. Uniqueness is per honest linear author chain (a forked chain can present two; no global uniqueness).

**Configuration limits** (integrity): at most 32 categories, 256 traits per category, `display_name` ≤ 64 bytes; ids ≤ 64 bytes, non-empty and unique; total weight > 0; zero weights allowed. Ids are exact byte strings: no normalization or charset rule. A generative collection must set `collection_id` and `randomness`. The configuration lives in DNA properties and is immutable for the life of the DNA.

**Limitations.** `dev-insecure` is forgeable, so the randomness is not secure and the minter can still choose favourable outcomes; a production provider (request-bound, verified) is required and does not exist. A minter can withhold recording randomness or the record (liveness). The image URI is minter-supplied and not bound to the traits; there is no renderer or media storage. Delegated reveals and reveal agreements are unsupported for generative collections. Blind batches remain a separate, non-generative product.
