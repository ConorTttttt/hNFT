//! G4: the provider-independent randomness VERIFICATION boundary.
//!
//! ```text
//! RandomnessResult ── provider_id ──> verifier_for() ──> ProviderVerifier
//!                                                           ├─ parse verifier_params (deterministic)
//!                                                           ├─ verify proof against the context
//!                                                           └─ check event / binding to the BatchClose
//!                                                         => VerifiedRandomness | error
//! ```
//!
//! **Validation never touches a network.** This crate depends only on `hdi`,
//! which has no network host functions, and every verifier receives everything
//! it needs as plain values: the `RandomnessResult`, the collection's
//! `RandomnessSpec`, and a [`VerificationContext`] built from the referenced
//! `BatchClose`/batch. If a provider needs an API call to *obtain* randomness,
//! that happens outside validation: some client or service fetches it and creates
//! the `RandomnessResult`, and validators check the embedded proof locally.
//!
//! What lives where:
//! * On-chain (schema unchanged): `close`, `provider_id`, `event_id`,
//!   `randomness`, `proof`, plus `RandomnessSpec { provider_id, verifier_params }`
//!   in DNA properties.
//! * Interpreted by the verifier: `proof`, `event_id`, `verifier_params`, and
//!   which context fields the proof covers. All provider-specific.
//! * NOT enforceable here: non-equivocation (one result per close). A verifier
//!   cannot see other results, so this remains a provider property.
//!
//! No production provider exists. The only registered verifier is the
//! development-only [`DevInsecureVerifier`], which is intentionally forgeable and
//! reports [`VerificationLevel::InsecureDev`] so it can never be mistaken for a
//! production provider.

use crate::blind_collection::BatchClose;
use crate::canonical::{preimage, put_action_hash, put_lp};
use crate::nft_manifest::commit_hash;
use crate::randomness::{RandomnessResult, RandomnessSpec};
use hdi::prelude::*;

/// Provider id of the INSECURE dev/test verifier. A collection opts in by
/// setting `RandomnessSpec.provider_id` to this. Anyone can compute a valid
/// proof for any randomness.
pub const INSECURE_DEV_PROVIDER_ID: &str = "dev-insecure";

/// How much trust a successful verification carries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerificationLevel {
    /// Forgeable development scheme. Proves NOTHING about where the randomness
    /// came from. Never production.
    InsecureDev,
    /// A provider verifier established authenticity and binding cryptographically.
    /// (No such verifier exists yet.)
    ProviderVerified,
}

/// A verified randomness result, carrying its trust level.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VerifiedRandomness {
    pub level: VerificationLevel,
    pub provider_id: String,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
}

impl VerifiedRandomness {
    /// `true` only for a provider verifier; always `false` for the dev scheme.
    pub fn is_production_grade(&self) -> bool {
        self.level == VerificationLevel::ProviderVerified
    }
}

/// Why a result was not accepted. Kept distinct so callers and tests can tell a
/// syntactic problem from a provider mismatch, an unsupported provider, a bad
/// proof, and a binding failure.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RandomnessVerificationError {
    /// Bounds or format (result fields, `verifier_params`, proof shape).
    Malformed(String),
    /// `result.provider_id` differs from the collection's `RandomnessSpec`.
    ProviderMismatch { expected: String, found: String },
    /// No verifier is registered for this provider id.
    UnsupportedProvider(String),
    /// The proof does not verify.
    ProofRejected(String),
    /// The result is not bound to the close being verified.
    BindingMismatch(String),
}

impl std::fmt::Display for RandomnessVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed randomness result: {m}"),
            Self::ProviderMismatch { expected, found } => write!(
                f,
                "provider_id '{found}' does not match the collection's RandomnessSpec '{expected}'"
            ),
            Self::UnsupportedProvider(p) => {
                write!(f, "no verifier is implemented for provider '{p}'")
            }
            Self::ProofRejected(m) => write!(f, "randomness proof rejected: {m}"),
            Self::BindingMismatch(m) => write!(f, "randomness is not bound to this close: {m}"),
        }
    }
}

/// Everything a verifier may use, all derived from records validation already
/// holds. Deliberately in-memory only: not a schema.
pub struct VerificationContext<'a> {
    pub collection_id: &'a [u8; 32],
    /// The `BatchClose` action the result must be bound to.
    pub close_hash: &'a ActionHash,
    pub close: &'a BatchClose,
    /// N, the batch's committed inventory size.
    pub batch_size: u32,
}

/// One provider's deterministic, network-free verification. Implementations must
/// be pure functions of their arguments.
pub trait RandomnessVerifier {
    fn security_level(&self) -> VerificationLevel;

    fn verify(
        &self,
        spec: &RandomnessSpec,
        ctx: &VerificationContext,
        result: &RandomnessResult,
    ) -> Result<VerifiedRandomness, RandomnessVerificationError>;
}

static DEV_INSECURE: DevInsecureVerifier = DevInsecureVerifier;

/// The registry: provider id -> verifier. A real provider adds an arm here; no
/// assignment, generation or batch code changes.
pub fn verifier_for(
    provider_id: &str,
) -> Result<&'static dyn RandomnessVerifier, RandomnessVerificationError> {
    match provider_id {
        INSECURE_DEV_PROVIDER_ID => Ok(&DEV_INSECURE),
        other => Err(RandomnessVerificationError::UnsupportedProvider(
            other.to_string(),
        )),
    }
}

/// The full deterministic acceptance decision for a `RandomnessResult`:
/// syntax, provider identity, binding to the close, then the provider's proof.
pub fn verify_randomness_result(
    spec: &RandomnessSpec,
    ctx: &VerificationContext,
    result: &RandomnessResult,
) -> Result<VerifiedRandomness, RandomnessVerificationError> {
    use RandomnessVerificationError as E;
    result.check_bounds().map_err(E::Malformed)?;
    spec.validate().map_err(E::Malformed)?;
    if result.provider_id != spec.provider_id {
        return Err(E::ProviderMismatch {
            expected: spec.provider_id.clone(),
            found: result.provider_id.clone(),
        });
    }
    if &result.close != ctx.close_hash {
        return Err(E::BindingMismatch(
            "result.close is not the close being verified".into(),
        ));
    }
    verifier_for(&spec.provider_id)?.verify(spec, ctx, result)
}

// ---------------------------------------------------------------------------
// Dev-only insecure verifier
// ---------------------------------------------------------------------------

const MAX_DEV_DOMAIN_LEN: usize = 32;

/// Dev `verifier_params`: `""` (no domain) or `"domain:<1..=32 of [A-Za-z0-9_-]>"`.
/// The domain is mixed into the dev proof, so the same result under a different
/// configuration does not verify. Anything else is malformed.
pub fn parse_dev_params(params: &str) -> Result<String, RandomnessVerificationError> {
    if params.is_empty() {
        return Ok(String::new());
    }
    let Some(domain) = params.strip_prefix("domain:") else {
        return Err(RandomnessVerificationError::Malformed(
            "dev verifier_params must be empty or 'domain:<name>'".into(),
        ));
    };
    let ok = !domain.is_empty()
        && domain.len() <= MAX_DEV_DOMAIN_LEN
        && domain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !ok {
        return Err(RandomnessVerificationError::Malformed(
            "dev domain must be 1..=32 of [A-Za-z0-9_-]".into(),
        ));
    }
    Ok(domain.to_string())
}

/// The dev proof: a PUBLIC hash of the whole context, so anyone can forge it.
///
/// ```text
/// put("hnft.dev-insecure.proof") ‖ u32_le(2) ‖ collection_id[32] ‖ batch[39]
/// ‖ close[39] ‖ put(domain) ‖ put(event_id) ‖ randomness[32]
/// ```
pub fn dev_insecure_proof(
    domain: &str,
    ctx: &VerificationContext,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> Vec<u8> {
    let mut buf = preimage("hnft.dev-insecure.proof", 2);
    buf.extend_from_slice(ctx.collection_id);
    put_action_hash(&mut buf, &ctx.close.batch);
    put_action_hash(&mut buf, ctx.close_hash);
    put_lp(&mut buf, domain.as_bytes());
    put_lp(&mut buf, event_id);
    buf.extend_from_slice(randomness);
    commit_hash(&buf).to_vec()
}

/// The dev proof for a collection's spec (parses its params).
pub fn dev_insecure_proof_for_spec(
    spec: &RandomnessSpec,
    ctx: &VerificationContext,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> Result<Vec<u8>, RandomnessVerificationError> {
    Ok(dev_insecure_proof(
        &parse_dev_params(&spec.verifier_params)?,
        ctx,
        event_id,
        randomness,
    ))
}

/// DEVELOPMENT ONLY, INSECURE. Checks that the proof is the public hash of the
/// context; proves nothing about the origin of the randomness.
pub struct DevInsecureVerifier;

impl RandomnessVerifier for DevInsecureVerifier {
    fn security_level(&self) -> VerificationLevel {
        VerificationLevel::InsecureDev
    }

    fn verify(
        &self,
        spec: &RandomnessSpec,
        ctx: &VerificationContext,
        result: &RandomnessResult,
    ) -> Result<VerifiedRandomness, RandomnessVerificationError> {
        use RandomnessVerificationError as E;
        if spec.provider_id != INSECURE_DEV_PROVIDER_ID {
            return Err(E::UnsupportedProvider(spec.provider_id.clone()));
        }
        let domain = parse_dev_params(&spec.verifier_params)?;
        if result.event_id.is_empty() {
            return Err(E::Malformed("dev event_id must not be empty".into()));
        }
        if result.proof.len() != 32 {
            return Err(E::Malformed("dev proof must be exactly 32 bytes".into()));
        }
        let expected = dev_insecure_proof(&domain, ctx, &result.event_id, &result.randomness);
        if result.proof != expected {
            return Err(E::ProofRejected(
                "dev proof does not match the collection, batch, close, event and randomness"
                    .into(),
            ));
        }
        Ok(VerifiedRandomness {
            level: self.security_level(),
            provider_id: result.provider_id.clone(),
            event_id: result.event_id.clone(),
            randomness: result.randomness,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use RandomnessVerificationError as E;

    fn ah(byte: u8, loc: u8) -> ActionHash {
        let mut raw = vec![0x84, 0x29, 0x24];
        raw.extend_from_slice(&[byte; 32]);
        raw.extend_from_slice(&[0, 0, 0, loc]);
        ActionHash::from_raw_39(raw)
    }
    fn close_entry(batch: u8) -> BatchClose {
        BatchClose {
            batch: ah(batch, 1),
            inventory: ah(0xE0, 3),
            purchases: vec![ah(0x01, 4)],
        }
    }
    fn dev_spec(params: &str) -> RandomnessSpec {
        RandomnessSpec {
            provider_id: INSECURE_DEV_PROVIDER_ID.into(),
            verifier_params: params.into(),
        }
    }
    const CID: [u8; 32] = [0xC0; 32];

    struct Fixture {
        close: BatchClose,
        close_hash: ActionHash,
    }
    impl Fixture {
        fn new() -> Self {
            Self {
                close: close_entry(0xB0),
                close_hash: ah(0xD0, 2),
            }
        }
        fn ctx(&self) -> VerificationContext<'_> {
            VerificationContext {
                collection_id: &CID,
                close_hash: &self.close_hash,
                close: &self.close,
                batch_size: 5,
            }
        }
        fn result(&self, spec: &RandomnessSpec) -> RandomnessResult {
            let randomness = [7u8; 32];
            let event_id = b"event-1".to_vec();
            RandomnessResult {
                close: self.close_hash.clone(),
                provider_id: spec.provider_id.clone(),
                proof: dev_insecure_proof_for_spec(spec, &self.ctx(), &event_id, &randomness)
                    .unwrap(),
                event_id,
                randomness,
            }
        }
    }

    #[test]
    fn valid_dev_result_is_accepted_and_marked_insecure() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let v = verify_randomness_result(&spec, &f.ctx(), &f.result(&spec)).unwrap();
        assert_eq!(v.level, VerificationLevel::InsecureDev);
        assert!(
            !v.is_production_grade(),
            "dev must never look production-grade"
        );
        assert_eq!(v.provider_id, INSECURE_DEV_PROVIDER_ID);
        assert_eq!(v.randomness, [7u8; 32]);
    }

    #[test]
    fn wrong_provider_id_is_rejected() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let mut r = f.result(&spec);
        r.provider_id = "someone-else".into();
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &r),
            Err(E::ProviderMismatch { .. })
        ));
    }

    #[test]
    fn malformed_event_id_is_rejected() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let mut long = f.result(&spec);
        long.event_id = vec![1; 65];
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &long),
            Err(E::Malformed(_))
        ));
        let mut empty = f.result(&spec);
        empty.event_id = vec![];
        empty.proof = dev_insecure_proof("", &f.ctx(), &[], &empty.randomness);
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &empty),
            Err(E::Malformed(_))
        ));
    }

    #[test]
    fn malformed_proof_is_rejected() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let mut short = f.result(&spec);
        short.proof = vec![0; 31];
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &short),
            Err(E::Malformed(_))
        ));
        let mut huge = f.result(&spec);
        huge.proof = vec![0; 4097];
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &huge),
            Err(E::Malformed(_))
        ));
        // Right length, wrong bytes: a proof failure, not a syntax error.
        let mut wrong = f.result(&spec);
        wrong.proof = vec![0; 32];
        assert!(matches!(
            verify_randomness_result(&spec, &f.ctx(), &wrong),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn result_bound_to_a_different_close_is_rejected() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let r = f.result(&spec);
        // Verifying against another close: the result.close no longer matches.
        let other = Fixture {
            close: close_entry(0xB0),
            close_hash: ah(0xD1, 9),
        };
        assert!(matches!(
            verify_randomness_result(&spec, &other.ctx(), &r),
            Err(E::BindingMismatch(_))
        ));
        // A result re-labelled to the other close but keeping the old proof fails the proof.
        let mut relabelled = r.clone();
        relabelled.close = other.close_hash.clone();
        assert!(matches!(
            verify_randomness_result(&spec, &other.ctx(), &relabelled),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn result_for_a_different_batch_or_collection_is_rejected() {
        let f = Fixture::new();
        let spec = dev_spec("");
        let r = f.result(&spec);
        // Same close hash, but the close belongs to another batch.
        let other_batch = Fixture {
            close: close_entry(0xB1),
            close_hash: f.close_hash.clone(),
        };
        assert!(matches!(
            verify_randomness_result(&spec, &other_batch.ctx(), &r),
            Err(E::ProofRejected(_))
        ));
        // Same close, but another collection.
        let other_cid = [0xC1u8; 32];
        let ctx = VerificationContext {
            collection_id: &other_cid,
            close_hash: &f.close_hash,
            close: &f.close,
            batch_size: 5,
        };
        assert!(matches!(
            verify_randomness_result(&spec, &ctx, &r),
            Err(E::ProofRejected(_))
        ));
    }

    #[test]
    fn unsupported_provider_is_rejected_cleanly() {
        let f = Fixture::new();
        let spec = RandomnessSpec {
            provider_id: "not-registered".into(),
            verifier_params: "".into(),
        };
        let mut r = f.result(&dev_spec(""));
        r.provider_id = "not-registered".into();
        match verify_randomness_result(&spec, &f.ctx(), &r) {
            Err(E::UnsupportedProvider(p)) => assert_eq!(p, "not-registered"),
            other => panic!("expected UnsupportedProvider, got {other:?}"),
        }
        assert!(verifier_for("not-registered").is_err());
        assert_eq!(
            verifier_for(INSECURE_DEV_PROVIDER_ID)
                .unwrap()
                .security_level(),
            VerificationLevel::InsecureDev
        );
    }

    #[test]
    fn verifier_params_are_interpreted_deterministically() {
        assert_eq!(parse_dev_params("").unwrap(), "");
        assert_eq!(parse_dev_params("domain:test-1_A").unwrap(), "test-1_A");
        for bad in [
            "domain:",
            "domain:has space",
            "x",
            "domain:é",
            &format!("domain:{}", "a".repeat(33)),
        ] {
            assert!(parse_dev_params(bad).is_err(), "{bad:?}");
        }
        let f = Fixture::new();
        // The same result verifies under the params it was built for, and only those.
        let a = dev_spec("domain:alpha");
        let b = dev_spec("domain:beta");
        let ra = f.result(&a);
        assert!(verify_randomness_result(&a, &f.ctx(), &ra).is_ok());
        assert!(verify_randomness_result(&b, &f.ctx(), &ra).is_err());
        assert!(verify_randomness_result(&dev_spec(""), &f.ctx(), &ra).is_err());
        // Malformed params are a syntax error, not a proof failure.
        let bad = dev_spec("nonsense");
        assert!(matches!(
            verify_randomness_result(&bad, &f.ctx(), &ra),
            Err(E::Malformed(_))
        ));
        // Repeated verification is a pure function of its inputs.
        for _ in 0..5 {
            assert_eq!(
                verify_randomness_result(&a, &f.ctx(), &ra),
                verify_randomness_result(&a, &f.ctx(), &ra)
            );
        }
    }

    #[test]
    fn verification_performs_no_network_io() {
        // Structural: this crate depends only on `hdi` (no network host calls) and
        // the verifier is a pure function of its arguments (checked above). As a
        // guard against regressions, the verifier sources must not mention
        // network primitives. Tokens are assembled at runtime so this test does
        // not match itself.
        let banned = [
            ["std::", "net"].concat(),
            ["req", "west"].concat(),
            ["ht", "tp"].concat(),
            ["Tcp", "Stream"].concat(),
            ["Udp", "Socket"].concat(),
            ["call_", "remote"].concat(),
            ["hdk", "::"].concat(),
        ];
        let sources = [
            include_str!("randomness_verifier.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap(),
            include_str!("randomness.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap(),
        ];
        for src in sources {
            for token in &banned {
                assert!(
                    !src.contains(token.as_str()),
                    "verifier code mentions {token}"
                );
            }
        }
    }
}
