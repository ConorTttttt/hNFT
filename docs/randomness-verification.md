# Randomness verification boundary (G4)

G4 defines the provider-independent boundary through which a `RandomnessResult` is accepted or rejected. **It does not choose, integrate or recommend a production randomness provider.** The only verifier that exists is `dev-insecure`, a development/test mechanism that is **insecure by design** and provides no entropy guarantee.

Code: `dnas/nft/zomes/integrity/nft/src/randomness_verifier.rs` (boundary, registry, dev verifier, unit tests) and `randomness.rs` (`RandomnessSpec`, `RandomnessResult`, the entry validator, `generation_seed`).

## Architecture

```text
RandomnessResult
       |
       v
provider_id
       |
       v
verifier_for(provider_id)
       |
       v
RandomnessVerifier
       |
       +--> deterministic local verification
       +--> proof verification
       +--> context/binding verification
       |
       v
VerifiedRandomness  ── level: InsecureDev | ProviderVerified
```

- **`verifier_for(provider_id)`** is the registry: a static dispatch from provider id to a verifier, or `UnsupportedProvider`. A future provider is added as one arm here.
- **`RandomnessVerifier`** is a trait whose `verify(spec, ctx, result)` is a pure function returning a typed `VerifiedRandomness` or a typed error.
- **`verify_randomness_result`** is the full acceptance decision: result bounds and spec syntax, provider identity, binding to the close, then the provider's own proof check.
- Future providers plug into this boundary. They do **not** require changes to the blind-batch schemas, `RandomnessResult`, the assignment algorithm, the generation algorithm, or the batch state machine.

Neither the `RandomnessResult` schema nor the `CollectionProperties.randomness` (`RandomnessSpec { provider_id, verifier_params }`) schema was changed by G4.

## VerificationContext

A verifier receives an **in-memory** `VerificationContext` built by the validator from records it already holds:

| Field | Meaning |
|---|---|
| `collection_id` | the collection's 32-byte id (from DNA properties) |
| `close_hash` | the `BatchClose` action the result must be bound to |
| `close` | the decoded `BatchClose` |
| `batch_size` | N, the batch's committed inventory size |

It is not an on-chain entry or property and adds no schema field.

## Validation flow

`validate_create_randomness_result`:

1. Checks the bounds of the `RandomnessResult` (`provider_id ≤ 64`, `event_id ≤ 64`, `proof ≤ 4096`).
2. Loads the collection's `RandomnessSpec` and `collection_id` from DNA properties.
3. Loads the referenced `BatchClose` and its batch through the **type-checked loaders** (the action's entry type is verified, so a look-alike entry cannot be substituted).
4. Builds the `VerificationContext`.
5. Calls `verify_randomness_result`, which dispatches through the verifier registry.
6. That call checks, in order: syntax, provider identity (`result.provider_id` equals the spec's), that `result.close` is the close being verified, and finally the provider-specific proof.
7. Returns a typed `VerifiedRandomness` or a typed verification error. An error is surfaced as `ValidateCallbackResult::Invalid`.
8. The whole path is deterministic and performs no network access.

The verification level is computed but **not consulted or stored** by validation: a result is acceptable to the collection that configured its provider. Consumers that care about trust inspect the provider.

## Error model

Verification-boundary errors (`RandomnessVerificationError`), which validation reports as invalid:

| Error | Meaning |
|---|---|
| `Malformed` | bounds or format: result fields, `verifier_params`, proof shape |
| `ProviderMismatch` | `result.provider_id` differs from the collection's `RandomnessSpec` |
| `UnsupportedProvider` | no verifier is registered for the provider id |
| `ProofRejected` | the proof does not verify |
| `BindingMismatch` | the result is not bound to the close being verified |

## Verification level

```text
InsecureDev       forgeable development scheme; NOT production-grade
ProviderVerified  reserved for a provider verifier that establishes authenticity and binding
```

`VerifiedRandomness::is_production_grade()` is `false` for `InsecureDev`. **No verifier that produces `ProviderVerified` exists yet**; the variant is the seam for one.

## The `dev-insecure` verifier

- Its provider id is exactly `dev-insecure`, and it is used only by collections whose `RandomnessSpec.provider_id` is that string.
- Its "proof" is a **public, forgeable hash by design**: anyone, including the minter, can compute a valid proof for any randomness. It is insecure and suitable only for exercising the protocol in development and tests. It is not an oracle, not an entropy source, and not a production randomness mechanism.
- The proof binds the collection, the batch and the close, plus the event, the randomness and a configured domain:
  ```text
  put("hnft.dev-insecure.proof") ‖ u32_le(2) ‖ collection_id[32] ‖ batch[39]
  ‖ close[39] ‖ put(domain) ‖ put(event_id) ‖ randomness[32]        (blake2b-256)
  ```
- It requires a non-empty `event_id` and a proof of exactly 32 bytes; otherwise the result is `Malformed`.
- `verifier_params` is either empty or `domain:<name>` (1 to 32 characters of `A-Za-z0-9_-`); the domain is mixed into the proof. Any other value is `Malformed`.
- It reports `VerificationLevel::InsecureDev`, so it can never be mistaken for a provider verifier.

The coordinator's `dev_randomness_proof` and the UI's "Record dev randomness (insecure)" action exist only to drive this development scheme.

## Network and I/O boundary

> Randomness verification performed by integrity validation must be deterministic and must not perform HTTP, RPC, or other network access.

The integrity crate depends only on `hdi`, which has no network host functions, and a verifier is a pure function of its arguments. Any external interaction needed to *obtain* randomness or proof material belongs outside validation: some client or service obtains it and creates the on-chain `RandomnessResult`, and validators verify the supplied proof locally. No verifier fetches anything from an external service during validation. A unit test also guards the verifier sources against network primitives.

## Non-equivocation

- **Multiple valid `RandomnessResult` entries for the same close can exist** when authored by different agents (the dev provider deliberately has no idempotence, and a sweettest exercises exactly this).
- **The validator does not attempt to enforce global uniqueness** of results across authors: it cannot see other authors' entries.
- Each result yields its own deterministic assignment, and a first transfer cites one specific result in its `BatchRelease`.
- **Non-equivocation remains a provider property.** G4 does not provide it. Whether a future provider can offer suitable uniqueness (a single acceptable result per context) is a provider-specific question that is deferred.

## What remains deferred (provider-specific)

G4 did not define, and no code assumes:

- a proof format;
- `event_id` semantics;
- the `verifier_params` format for any real provider;
- how a provider's proof cryptographically binds to the verification context;
- the authenticity mechanism;
- whether a provider can guarantee uniqueness / non-equivocation;
- whether validation should ever gate on `VerificationLevel` (for example refusing `InsecureDev` in a production-flagged collection).

## Unchanged by G4

Blind-batch entry schemas, the batch state machine, the frozen assignment algorithm and seed, the frozen generation seed and G1 category roll, and all golden vectors are untouched.
