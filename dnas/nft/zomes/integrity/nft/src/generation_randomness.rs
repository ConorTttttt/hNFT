//! G5a: verified, per-NFT randomness for open generative collections (Mode A).
//!
//! ```text
//! NFT mint ── ActionHash exists
//!    └─ GenerationRandomness      bound to that NFT, verified locally
//!         └─ generation_seed(...)  frozen encoding (randomness.rs)
//!              └─ generate(...)    frozen G1 generator
//!                   └─ GenerationRecord ── reveal_generated
//! ```
//!
//! This is an ADDITIVE path parallel to the frozen G4 batch boundary
//! (`randomness_verifier.rs`). It does not modify G4 and never uses a
//! `BatchClose`. It reuses only G4's `VerificationLevel` label and its
//! `parse_dev_params` helper (read-only).
//!
//! Like G4, verification is deterministic and network-free: a verifier is a pure
//! function of `(spec, context, result)`. Anything a provider must fetch happens
//! outside validation; the proof travels in the entry.
//!
//! Authority and ordering: a `GenerationRandomness` is authored by the NFT's
//! minter, at most once per NFT on the minter's own chain (fork caveat: uniqueness
//! is per honest linear chain, not global), and before any reveal. It references
//! the NFT's mint hash, so it provably comes after the mint (causal, no
//! timestamps); the `GenerationRecord` then references it, and the reveal comes
//! after the record.
//!
//! The dev verifier is INSECURE and forgeable by design. No production provider
//! exists; nothing here makes the randomness secure until one does.

use crate::canonical::{preimage, put_action_hash, put_lp, put_u32};
use crate::dna_properties::CollectionProperties;
use crate::nft_manifest::{commit_hash, is_create_of, is_nft_manifest_create};
use crate::randomness::RandomnessSpec;
use crate::randomness_verifier::{parse_dev_params, VerificationLevel, INSECURE_DEV_PROVIDER_ID};
use crate::UnitEntryTypes;
use hdi::prelude::*;

pub const MAX_GEN_PROVIDER_ID_LEN: usize = 64;
pub const MAX_GEN_EVENT_ID_LEN: usize = 64;
pub const MAX_GEN_PROOF_LEN: usize = 4096;

/// Verified randomness for ONE NFT. The collection id and generation version are
/// not stored: they come deterministically from the cell's DNA properties.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct GenerationRandomness {
    /// The NFT (mint Create action) this randomness is bound to.
    pub nft: ActionHash,
    pub provider_id: String,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
    pub proof: Vec<u8>,
}

impl GenerationRandomness {
    pub fn check_bounds(&self) -> Result<(), String> {
        if self.provider_id.len() > MAX_GEN_PROVIDER_ID_LEN {
            return Err(format!(
                "provider_id exceeds {MAX_GEN_PROVIDER_ID_LEN} bytes"
            ));
        }
        if self.event_id.len() > MAX_GEN_EVENT_ID_LEN {
            return Err(format!("event_id exceeds {MAX_GEN_EVENT_ID_LEN} bytes"));
        }
        if self.proof.len() > MAX_GEN_PROOF_LEN {
            return Err(format!("proof exceeds {MAX_GEN_PROOF_LEN} bytes"));
        }
        Ok(())
    }
}

/// In-memory context for verifying generative randomness (not a schema).
pub struct GenerationVerificationContext<'a> {
    pub collection_id: &'a [u8; 32],
    pub nft: &'a ActionHash,
    pub generation_version: u32,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerifiedGenerationRandomness {
    pub level: VerificationLevel,
    pub provider_id: String,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
}

impl VerifiedGenerationRandomness {
    /// Always `false` for the dev scheme.
    pub fn is_production_grade(&self) -> bool {
        self.level == VerificationLevel::ProviderVerified
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GenerationRandomnessVerificationError {
    Malformed(String),
    ProviderMismatch {
        expected: String,
        found: String,
    },
    UnsupportedProvider(String),
    ProofRejected(String),
    /// The result is not bound to the NFT being verified.
    BindingMismatch(String),
}

impl std::fmt::Display for GenerationRandomnessVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed generation randomness: {m}"),
            Self::ProviderMismatch { expected, found } => write!(
                f,
                "provider_id '{found}' does not match the collection's RandomnessSpec '{expected}'"
            ),
            Self::UnsupportedProvider(p) => {
                write!(
                    f,
                    "no generation verifier is implemented for provider '{p}'"
                )
            }
            Self::ProofRejected(m) => write!(f, "generation randomness proof rejected: {m}"),
            Self::BindingMismatch(m) => {
                write!(f, "generation randomness is not bound to this NFT: {m}")
            }
        }
    }
}

/// One provider's deterministic, network-free verification for generative NFTs.
pub trait GenerationRandomnessVerifier {
    fn security_level(&self) -> VerificationLevel;

    fn verify(
        &self,
        spec: &RandomnessSpec,
        ctx: &GenerationVerificationContext,
        result: &GenerationRandomness,
    ) -> Result<VerifiedGenerationRandomness, GenerationRandomnessVerificationError>;
}

static DEV_GENERATION: DevInsecureGenerationVerifier = DevInsecureGenerationVerifier;

/// Registry: provider id -> generative verifier. Production providers are added
/// here in the future; none exists.
pub fn generation_verifier_for(
    provider_id: &str,
) -> Result<&'static dyn GenerationRandomnessVerifier, GenerationRandomnessVerificationError> {
    match provider_id {
        INSECURE_DEV_PROVIDER_ID => Ok(&DEV_GENERATION),
        other => Err(GenerationRandomnessVerificationError::UnsupportedProvider(
            other.to_string(),
        )),
    }
}

/// The full deterministic acceptance decision: syntax, provider identity, binding
/// to the NFT, then the provider's proof.
pub fn verify_generation_randomness(
    spec: &RandomnessSpec,
    ctx: &GenerationVerificationContext,
    result: &GenerationRandomness,
) -> Result<VerifiedGenerationRandomness, GenerationRandomnessVerificationError> {
    use GenerationRandomnessVerificationError as E;
    result.check_bounds().map_err(E::Malformed)?;
    spec.validate().map_err(E::Malformed)?;
    if result.provider_id != spec.provider_id {
        return Err(E::ProviderMismatch {
            expected: spec.provider_id.clone(),
            found: result.provider_id.clone(),
        });
    }
    if &result.nft != ctx.nft {
        return Err(E::BindingMismatch(
            "result.nft is not the NFT being verified".into(),
        ));
    }
    generation_verifier_for(&spec.provider_id)?.verify(spec, ctx, result)
}

// ---------------------------------------------------------------------------
// Dev-only insecure verifier
// ---------------------------------------------------------------------------

/// The dev proof: a PUBLIC hash of the whole context, forgeable by anyone.
///
/// ```text
/// put("hnft.dev-insecure.generation-proof") ‖ u32_le(1) ‖ collection_id[32]
/// ‖ nft[39] ‖ u32_le(generation_version) ‖ put(provider_id) ‖ put(domain)
/// ‖ put(event_id) ‖ randomness[32]                              (blake2b-256)
/// ```
pub fn dev_generation_proof(
    domain: &str,
    ctx: &GenerationVerificationContext,
    provider_id: &str,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> Vec<u8> {
    let mut buf = preimage("hnft.dev-insecure.generation-proof", 1);
    buf.extend_from_slice(ctx.collection_id);
    put_action_hash(&mut buf, ctx.nft);
    put_u32(&mut buf, ctx.generation_version);
    put_lp(&mut buf, provider_id.as_bytes());
    put_lp(&mut buf, domain.as_bytes());
    put_lp(&mut buf, event_id);
    buf.extend_from_slice(randomness);
    commit_hash(&buf).to_vec()
}

/// The dev proof for a collection's spec (parses its `verifier_params`).
pub fn dev_generation_proof_for_spec(
    spec: &RandomnessSpec,
    ctx: &GenerationVerificationContext,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> Result<Vec<u8>, GenerationRandomnessVerificationError> {
    let domain = parse_dev_params(&spec.verifier_params)
        .map_err(|e| GenerationRandomnessVerificationError::Malformed(e.to_string()))?;
    Ok(dev_generation_proof(
        &domain,
        ctx,
        &spec.provider_id,
        event_id,
        randomness,
    ))
}

/// DEVELOPMENT ONLY, INSECURE. Proves nothing about the origin of the randomness.
pub struct DevInsecureGenerationVerifier;

impl GenerationRandomnessVerifier for DevInsecureGenerationVerifier {
    fn security_level(&self) -> VerificationLevel {
        VerificationLevel::InsecureDev
    }

    fn verify(
        &self,
        spec: &RandomnessSpec,
        ctx: &GenerationVerificationContext,
        result: &GenerationRandomness,
    ) -> Result<VerifiedGenerationRandomness, GenerationRandomnessVerificationError> {
        use GenerationRandomnessVerificationError as E;
        if spec.provider_id != INSECURE_DEV_PROVIDER_ID {
            return Err(E::UnsupportedProvider(spec.provider_id.clone()));
        }
        let domain =
            parse_dev_params(&spec.verifier_params).map_err(|e| E::Malformed(e.to_string()))?;
        if result.event_id.is_empty() {
            return Err(E::Malformed("dev event_id must not be empty".into()));
        }
        if result.proof.len() != 32 {
            return Err(E::Malformed("dev proof must be exactly 32 bytes".into()));
        }
        let expected = dev_generation_proof(
            &domain,
            ctx,
            &result.provider_id,
            &result.event_id,
            &result.randomness,
        );
        if result.proof != expected {
            return Err(E::ProofRejected(
                "dev proof does not match the collection, NFT, version, provider, event and randomness"
                    .into(),
            ));
        }
        Ok(VerifiedGenerationRandomness {
            level: self.security_level(),
            provider_id: result.provider_id.clone(),
            event_id: result.event_id.clone(),
            randomness: result.randomness,
        })
    }
}

// ---------------------------------------------------------------------------
// Loader (type-checked: look-alike entries can decode as each other)
// ---------------------------------------------------------------------------

pub fn load_generation_randomness(
    hash: &ActionHash,
) -> ExternResult<(Record, GenerationRandomness)> {
    let record = must_get_valid_record(hash.clone())?;
    if !is_create_of(record.action(), UnitEntryTypes::GenerationRandomness) {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not the Create of a GenerationRandomness".into()
        )));
    }
    let entry: GenerationRandomness = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "not a GenerationRandomness".into()
        )))?;
    Ok((record, entry))
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_create_generation_randomness(
    action: TypedAction<EntryCreationData>,
    entry: GenerationRandomness,
) -> ExternResult<ValidateCallbackResult> {
    if let Err(e) = entry.check_bounds() {
        return Ok(ValidateCallbackResult::Invalid(e));
    }
    let props = CollectionProperties::get()?;
    let Some(cfg) = props.generation.clone() else {
        return Ok(ValidateCallbackResult::Invalid(
            "this collection is not generative".into(),
        ));
    };
    if let Err(e) = cfg.validate() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "invalid generation config: {e}"
        )));
    }
    let Some(spec) = props.randomness.clone() else {
        return Ok(ValidateCallbackResult::Invalid(
            "this collection has no RandomnessSpec".into(),
        ));
    };
    let Ok(collection_id) = props.collection_id_bytes_checked() else {
        return Ok(ValidateCallbackResult::Invalid(
            "this collection needs a valid collection_id".into(),
        ));
    };

    // The NFT must be a real mint by this same author.
    let mint = must_get_action(entry.nft.clone())?;
    if !is_nft_manifest_create(mint.action()) {
        return Ok(ValidateCallbackResult::Invalid(
            "GenerationRandomness.nft is not an NftManifest mint".into(),
        ));
    }
    if mint.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the NFT's minter may author its GenerationRandomness".into(),
        ));
    }

    // Deterministic, network-free verification.
    let ctx = GenerationVerificationContext {
        collection_id: &collection_id,
        nft: &entry.nft,
        generation_version: cfg.generation_version,
    };
    if let Err(e) = verify_generation_randomness(&spec, &ctx, &entry) {
        return Ok(ValidateCallbackResult::Invalid(e.to_string()));
    }

    // One per NFT on the author's own chain, and never after a reveal.
    let prev = action
        .prev_action()
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "randomness has no prev_action".into()
            ))
        })?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    for a in activity.iter() {
        let act = a.action.action();
        if let ActionData::Update(u) = &act.data {
            if u.original_action_address == entry.nft {
                return Ok(ValidateCallbackResult::Invalid(
                    "cannot record randomness after the NFT was revealed".into(),
                ));
            }
        }
        if is_create_of(act, UnitEntryTypes::GenerationRandomness) {
            let (_, other) = load_generation_randomness(a.action.as_hash())?;
            if other.nft == entry.nft {
                return Ok(ValidateCallbackResult::Invalid(
                    "this NFT already has a GenerationRandomness".into(),
                ));
            }
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

/// NFT mint action -> GenerationRandomness (discovery only; validators never
/// enumerate links).
pub fn validate_create_link_nft_to_generation_randomness(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let base = action
        .data
        .base_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link base is not an ActionHash".into()
        )))?;
    let target = action
        .data
        .target_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link target is not an ActionHash".into()
        )))?;
    let (_, result) = load_generation_randomness(&target)?;
    if result.nft != base {
        return Ok(ValidateCallbackResult::Invalid(
            "NftToGenerationRandomness base does not match the result's nft".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use GenerationRandomnessVerificationError as E;

    fn ah(byte: u8, loc: u8) -> ActionHash {
        let mut raw = vec![0x84, 0x29, 0x24];
        raw.extend_from_slice(&[byte; 32]);
        raw.extend_from_slice(&[0, 0, 0, loc]);
        ActionHash::from_raw_39(raw)
    }
    fn spec(params: &str) -> RandomnessSpec {
        RandomnessSpec {
            provider_id: INSECURE_DEV_PROVIDER_ID.into(),
            verifier_params: params.into(),
        }
    }
    const CID: [u8; 32] = [0xC0; 32];

    fn ctx<'a>(
        cid: &'a [u8; 32],
        nft: &'a ActionHash,
        version: u32,
    ) -> GenerationVerificationContext<'a> {
        GenerationVerificationContext {
            collection_id: cid,
            nft,
            generation_version: version,
        }
    }
    fn result(spec: &RandomnessSpec, nft: &ActionHash) -> GenerationRandomness {
        let c = ctx(&CID, nft, 1);
        let event_id = b"event-1".to_vec();
        let randomness = [7u8; 32];
        GenerationRandomness {
            nft: nft.clone(),
            provider_id: spec.provider_id.clone(),
            proof: dev_generation_proof_for_spec(spec, &c, &event_id, &randomness).unwrap(),
            event_id,
            randomness,
        }
    }

    #[test]
    fn valid_dev_result_is_accepted_and_marked_insecure() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let v = verify_generation_randomness(&s, &ctx(&CID, &nft, 1), &result(&s, &nft)).unwrap();
        assert_eq!(v.level, VerificationLevel::InsecureDev);
        assert!(!v.is_production_grade());
        assert_eq!(v.randomness, [7u8; 32]);
    }

    #[test]
    fn wrong_nft_is_rejected() {
        let (a, b) = (ah(0xA0, 1), ah(0xA1, 2));
        let s = spec("");
        let r = result(&s, &a);
        // Verifying against another NFT: the result names a different NFT.
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &b, 1), &r),
            Err(E::BindingMismatch(_))
        ));
        // Re-labelled to the other NFT with the old proof: proof fails.
        let mut relabelled = r.clone();
        relabelled.nft = b.clone();
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &b, 1), &relabelled),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn wrong_collection_is_rejected() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let r = result(&s, &nft);
        let other = [0xC1u8; 32];
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&other, &nft, 1), &r),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn wrong_generation_version_is_rejected() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let r = result(&s, &nft);
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &nft, 2), &r),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn wrong_provider_is_rejected() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let mut r = result(&s, &nft);
        r.provider_id = "someone-else".into();
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &nft, 1), &r),
            Err(E::ProviderMismatch { .. })
        ));
    }

    #[test]
    fn wrong_event_and_randomness_are_rejected() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let mut e = result(&s, &nft);
        e.event_id = b"event-2".to_vec();
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &nft, 1), &e),
            Err(E::ProofRejected(_))
        ));
        let mut r = result(&s, &nft);
        r.randomness = [8u8; 32];
        assert!(matches!(
            verify_generation_randomness(&s, &ctx(&CID, &nft, 1), &r),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn malformed_proof_and_event_are_rejected() {
        let nft = ah(0xA0, 1);
        let s = spec("");
        let c = ctx(&CID, &nft, 1);
        let mut short = result(&s, &nft);
        short.proof = vec![0; 31];
        assert!(matches!(
            verify_generation_randomness(&s, &c, &short),
            Err(E::Malformed(_))
        ));
        let mut huge = result(&s, &nft);
        huge.proof = vec![0; 4097];
        assert!(matches!(
            verify_generation_randomness(&s, &c, &huge),
            Err(E::Malformed(_))
        ));
        let mut long_event = result(&s, &nft);
        long_event.event_id = vec![1; 65];
        assert!(matches!(
            verify_generation_randomness(&s, &c, &long_event),
            Err(E::Malformed(_))
        ));
        let mut empty_event = result(&s, &nft);
        empty_event.event_id = vec![];
        assert!(matches!(
            verify_generation_randomness(&s, &c, &empty_event),
            Err(E::Malformed(_))
        ));
        let mut long_provider = result(&s, &nft);
        long_provider.provider_id = "a".repeat(65);
        assert!(matches!(
            verify_generation_randomness(&s, &c, &long_provider),
            Err(E::Malformed(_))
        ));
    }

    #[test]
    fn malformed_verifier_params_are_rejected() {
        let nft = ah(0xA0, 1);
        let good = spec("domain:alpha");
        let r = result(&good, &nft);
        let bad = spec("nonsense");
        assert!(matches!(
            verify_generation_randomness(&bad, &ctx(&CID, &nft, 1), &r),
            Err(E::Malformed(_))
        ));
        assert!(dev_generation_proof_for_spec(&bad, &ctx(&CID, &nft, 1), b"e", &[0; 32]).is_err());
        // Params are interpreted deterministically: another domain does not verify.
        assert!(verify_generation_randomness(&good, &ctx(&CID, &nft, 1), &r).is_ok());
        assert!(
            verify_generation_randomness(&spec("domain:beta"), &ctx(&CID, &nft, 1), &r).is_err()
        );
        assert!(verify_generation_randomness(&spec(""), &ctx(&CID, &nft, 1), &r).is_err());
    }

    #[test]
    fn unsupported_provider_is_rejected_cleanly() {
        let nft = ah(0xA0, 1);
        let s = RandomnessSpec {
            provider_id: "not-registered".into(),
            verifier_params: "".into(),
        };
        let mut r = result(&spec(""), &nft);
        r.provider_id = "not-registered".into();
        match verify_generation_randomness(&s, &ctx(&CID, &nft, 1), &r) {
            Err(E::UnsupportedProvider(p)) => assert_eq!(p, "not-registered"),
            other => panic!("expected UnsupportedProvider, got {other:?}"),
        }
        assert_eq!(
            generation_verifier_for(INSECURE_DEV_PROVIDER_ID)
                .unwrap()
                .security_level(),
            VerificationLevel::InsecureDev
        );
    }

    #[test]
    fn dev_proof_is_deterministic_and_binds_every_field() {
        let nft = ah(0xA0, 1);
        let base = dev_generation_proof("d", &ctx(&CID, &nft, 1), "p", b"e", &[1; 32]);
        assert_eq!(
            base,
            dev_generation_proof("d", &ctx(&CID, &nft, 1), "p", b"e", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("x", &ctx(&CID, &nft, 1), "p", b"e", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&CID, &nft, 1), "q", b"e", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&CID, &nft, 1), "p", b"f", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&CID, &nft, 1), "p", b"e", &[2; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&CID, &nft, 2), "p", b"e", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&[0xC1; 32], &nft, 1), "p", b"e", &[1; 32])
        );
        assert_ne!(
            base,
            dev_generation_proof("d", &ctx(&CID, &ah(0xA1, 1), 1), "p", b"e", &[1; 32])
        );
    }
}
