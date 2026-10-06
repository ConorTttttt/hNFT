//! G3a: provider-independent randomness boundary.
//!
//! ```text
//! RandomnessVerifier ── production: a future real provider verifier
//!                    └─ dev/test:   `DevInsecureVerifier` (NOT random, NOT secure)
//! ```
//!
//! A `RandomnessResult` is bound to a *context* (for blind batches, the raw 39
//! bytes of the `BatchClose` `ActionHash`). The verifier decides whether the
//! `(context, event_id, randomness, proof)` tuple is acceptable for the
//! collection's `RandomnessSpec`. Everything downstream (assignment, generation
//! seed) sees only the 32 randomness bytes plus the ids, never the provider.
//!
//! No real provider exists yet. Provider non-equivocation / idempotence per
//! context is a provider-level requirement; this module does not (cannot)
//! enforce a single accepted result across authors.

use crate::blind_collection::{load_batch_close, load_batch_commitment};
use crate::canonical::{preimage, put_action_hash, put_lp, put_u32};
use crate::dna_properties::CollectionProperties;
use crate::nft_manifest::commit_hash;
use crate::randomness_verifier::{verify_randomness_result, VerificationContext};
use hdi::prelude::*;

pub const MAX_PROVIDER_ID_LEN: usize = 64;
pub const MAX_EVENT_ID_LEN: usize = 64;
pub const MAX_PROOF_LEN: usize = 4096;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RandomnessSpec {
    pub provider_id: String,
    /// Opaque provider-specific verifier configuration.
    #[serde(default)]
    pub verifier_params: String,
}

impl RandomnessSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.provider_id.is_empty() {
            return Err("randomness provider_id must not be empty".into());
        }
        if self.provider_id.len() > MAX_PROVIDER_ID_LEN {
            return Err(format!(
                "randomness provider_id exceeds {MAX_PROVIDER_ID_LEN} bytes"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct RandomnessResult {
    /// The `BatchClose` this randomness is bound to (Mode B context).
    pub close: ActionHash,
    pub provider_id: String,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
    pub proof: Vec<u8>,
}

impl RandomnessResult {
    pub fn check_bounds(&self) -> Result<(), String> {
        if self.provider_id.len() > MAX_PROVIDER_ID_LEN {
            return Err(format!("provider_id exceeds {MAX_PROVIDER_ID_LEN} bytes"));
        }
        if self.event_id.len() > MAX_EVENT_ID_LEN {
            return Err(format!("event_id exceeds {MAX_EVENT_ID_LEN} bytes"));
        }
        if self.proof.len() > MAX_PROOF_LEN {
            return Err(format!("proof exceeds {MAX_PROOF_LEN} bytes"));
        }
        Ok(())
    }
}

/// Mode A canonical generation seed (fed to the frozen G1 `generate`).
///
/// ```text
/// put("hnft.generation.seed") ‖ u32_le(1) ‖ collection_id[32] ‖ nft[39]
/// ‖ u32_le(generation_version) ‖ put(provider_id) ‖ put(event_id) ‖ randomness[32]
/// ```
pub fn generation_seed(
    collection_id: &[u8; 32],
    nft: &ActionHash,
    generation_version: u32,
    provider_id: &str,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> crate::generation::GenerationSeed {
    let mut buf = preimage("hnft.generation.seed", 1);
    buf.extend_from_slice(collection_id);
    put_action_hash(&mut buf, nft);
    put_u32(&mut buf, generation_version);
    put_lp(&mut buf, provider_id.as_bytes());
    put_lp(&mut buf, event_id);
    buf.extend_from_slice(randomness);
    crate::generation::GenerationSeed(commit_hash(&buf))
}

pub fn validate_create_randomness_result(
    _action: TypedAction<EntryCreationData>,
    entry: RandomnessResult,
) -> ExternResult<ValidateCallbackResult> {
    // Cheap syntactic check first, so oversized data never reaches a record fetch.
    if let Err(e) = entry.check_bounds() {
        return Ok(ValidateCallbackResult::Invalid(e));
    }
    let props = CollectionProperties::get()?;
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
    // Everything the verifier may use comes from records validation holds; there
    // is no network access. The close must exist and be a valid BatchClose.
    let (_, close) = load_batch_close(&entry.close)?;
    let (_, batch) = load_batch_commitment(&close.batch)?;
    let ctx = VerificationContext {
        collection_id: &collection_id,
        close_hash: &entry.close,
        close: &close,
        batch_size: batch.batch_size,
    };
    match verify_randomness_result(&spec, &ctx, &entry) {
        // The level is deliberately not persisted or consulted here: a result is
        // acceptable to the collection that configured this provider. Consumers
        // that care about trust inspect the provider (dev vs production).
        Ok(_verified) => Ok(ValidateCallbackResult::Valid),
        Err(e) => Ok(ValidateCallbackResult::Invalid(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::parse_hex32;

    fn nft0() -> ActionHash {
        let mut raw = vec![0x84, 0x29, 0x24];
        raw.extend_from_slice(&[0xA0; 32]);
        raw.extend_from_slice(&[0, 0, 0, 0]);
        ActionHash::from_raw_39(raw)
    }

    #[test]
    fn generation_seed_golden_vector() {
        // Independently computed with Python hashlib.blake2b (see docs).
        let seed = generation_seed(&[0xC0; 32], &nft0(), 1, "dev-test", b"event-1", &[0x5A; 32]);
        assert_eq!(
            seed.0
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            "a74698536ffcad8661ecd13bc4fae8371370c2fb9a56e22a7b8885d7fcd7937c"
        );
    }

    #[test]
    fn generation_seed_depends_on_every_input() {
        let base = generation_seed(&[0xC0; 32], &nft0(), 1, "dev-test", b"event-1", &[0x5A; 32]);
        assert_ne!(
            base,
            generation_seed(&[0xC1; 32], &nft0(), 1, "dev-test", b"event-1", &[0x5A; 32])
        );
        assert_ne!(
            base,
            generation_seed(&[0xC0; 32], &nft0(), 2, "dev-test", b"event-1", &[0x5A; 32])
        );
        assert_ne!(
            base,
            generation_seed(&[0xC0; 32], &nft0(), 1, "dev-tesu", b"event-1", &[0x5A; 32])
        );
        assert_ne!(
            base,
            generation_seed(&[0xC0; 32], &nft0(), 1, "dev-test", b"event-2", &[0x5A; 32])
        );
        assert_ne!(
            base,
            generation_seed(&[0xC0; 32], &nft0(), 1, "dev-test", b"event-1", &[0x5B; 32])
        );
    }

    #[test]
    fn spec_validation() {
        let ok = RandomnessSpec {
            provider_id: "x".into(),
            verifier_params: "anything".into(),
        };
        assert!(ok.validate().is_ok());
        assert!(RandomnessSpec {
            provider_id: "".into(),
            verifier_params: "".into()
        }
        .validate()
        .is_err());
        assert!(RandomnessSpec {
            provider_id: "a".repeat(65),
            verifier_params: "".into()
        }
        .validate()
        .is_err());
        assert!(RandomnessSpec {
            provider_id: "a".repeat(64),
            verifier_params: "".into()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn result_bounds() {
        let r = |p: usize, e: usize, pr: usize| RandomnessResult {
            close: nft0(),
            provider_id: "a".repeat(p),
            event_id: vec![0; e],
            randomness: [0; 32],
            proof: vec![0; pr],
        };
        assert!(r(64, 64, 4096).check_bounds().is_ok());
        assert!(r(65, 0, 0).check_bounds().is_err());
        assert!(r(0, 65, 0).check_bounds().is_err());
        assert!(r(0, 0, 4097).check_bounds().is_err());
    }

    #[test]
    fn collection_id_hex_rules() {
        assert_eq!(parse_hex32(&"c0".repeat(32)).unwrap(), [0xC0; 32]);
        assert!(parse_hex32(&"c0".repeat(31)).is_err());
        assert!(parse_hex32(&"c0".repeat(33)).is_err());
        assert!(parse_hex32(&"C0".repeat(32)).is_err(), "uppercase rejected");
        assert!(parse_hex32(&"zz".repeat(32)).is_err());
        assert!(parse_hex32("").is_err());
    }
}
