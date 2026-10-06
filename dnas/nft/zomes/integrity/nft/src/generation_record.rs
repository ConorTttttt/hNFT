//! G2/G5a: immutable generation provenance for a generative NFT.
//!
//! A `GenerationRecord` fixes, for one NFT, the seed + generation version +
//! resulting traits. It is authored by the NFT's minter, linked from the mint
//! action via `NftToGeneration`, and MUST cite a verified `GenerationRandomness`
//! for the same NFT (G5a). The seed is never supplied: validators recompute
//!
//! ```text
//! seed = generation_seed(collection_id, nft, generation_version,
//!                        result.provider_id, result.event_id, result.randomness)
//! traits = generate(collection config, seed)
//! ```
//!
//! and require the record to match exactly. Reveal then requires the revealed
//! attributes to equal `generate(config, record.seed)`.
//!
//! What this does NOT provide: secure randomness. The only verifier is the
//! insecure `dev-insecure` scheme; until a production provider exists the
//! randomness is forgeable by the minter. The image URI is not bound to the
//! traits.

use crate::dna_properties::CollectionProperties;
use crate::generation::{generate, GeneratedTraits, GenerationConfig, GenerationSeed};
use crate::generation_randomness::{load_generation_randomness, GenerationRandomness};
use crate::nft_manifest::{is_create_of, is_nft_manifest_create, NftAttribute};
use crate::randomness::generation_seed;
use crate::UnitEntryTypes;
use hdi::prelude::*;

/// Where the seed's randomness came from. There is exactly one accepted form:
/// a verified `GenerationRandomness`. There is no unattested or supplied-seed
/// fallback.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RandomnessProvenance {
    /// The `GenerationRandomness` (Create action) this record's seed derives from.
    Verified { result: ActionHash },
}

#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct GenerationRecord {
    /// The NFT this record covers (its mint Create action).
    pub nft: ActionHash,
    /// Must equal the collection's `generation.generation_version`.
    pub generation_version: u32,
    /// Derived, never supplied: validators recompute it from `randomness`.
    pub seed: GenerationSeed,
    /// Must equal `generate(collection config, seed)`; recomputed by validators.
    pub generated_traits: GeneratedTraits,
    pub randomness: RandomnessProvenance,
}

/// The collection's validated generation config, or an explanation why there
/// isn't one.
fn collection_generation_config() -> ExternResult<Result<GenerationConfig, String>> {
    let props = CollectionProperties::get()?;
    Ok(match props.generation {
        None => Err("this collection is not generative".into()),
        Some(cfg) => cfg.validate().map(|_| cfg),
    })
}

/// Check a record against a config: version, recomputed traits.
pub fn check_record_against_config(
    record: &GenerationRecord,
    cfg: &GenerationConfig,
) -> Result<(), String> {
    if record.generation_version != cfg.generation_version {
        return Err("record generation_version does not match the collection's".into());
    }
    let expected = generate(cfg, &record.seed)?;
    if expected != record.generated_traits {
        return Err("generated_traits do not match generate(config, seed)".into());
    }
    Ok(())
}

/// The pure heart of record validation: the record must derive from exactly this
/// verified randomness. Nothing here trusts a supplied seed.
pub fn check_verified_record(
    record: &GenerationRecord,
    cfg: &GenerationConfig,
    collection_id: &[u8; 32],
    result_hash: &ActionHash,
    result: &GenerationRandomness,
) -> Result<(), String> {
    let RandomnessProvenance::Verified { result: cited } = &record.randomness;
    if cited != result_hash {
        return Err("record cites a different randomness result".into());
    }
    if result.nft != record.nft {
        return Err("the randomness result is bound to a different NFT".into());
    }
    check_record_against_config(record, cfg)?;
    let expected = generation_seed(
        collection_id,
        &record.nft,
        cfg.generation_version,
        &result.provider_id,
        &result.event_id,
        &result.randomness,
    );
    if record.seed != expected {
        return Err("record seed is not generation_seed(...) of the cited randomness".into());
    }
    Ok(())
}

pub fn validate_create_generation_record(
    action: TypedAction<EntryCreationData>,
    record: GenerationRecord,
) -> ExternResult<ValidateCallbackResult> {
    let cfg = match collection_generation_config()? {
        Ok(cfg) => cfg,
        Err(e) => return Ok(ValidateCallbackResult::Invalid(e)),
    };
    let props = CollectionProperties::get()?;
    let Ok(collection_id) = props.collection_id_bytes_checked() else {
        return Ok(ValidateCallbackResult::Invalid(
            "this collection needs a valid collection_id".into(),
        ));
    };
    let Some(spec) = props.randomness.as_ref() else {
        return Ok(ValidateCallbackResult::Invalid(
            "this collection has no RandomnessSpec".into(),
        ));
    };
    // The cited randomness must exist, be valid (it was verified when created),
    // belong to this NFT, and come from the collection's configured provider.
    //
    // NOTE: this does NOT re-run `verify_generation_randomness` here. It relies
    // on `load_generation_randomness`'s `must_get_valid_record`, whose contract
    // is that the record is "consistently reported as valid by the visible
    // network" -- i.e. the cited `GenerationRandomness` already passed its own
    // `validate_create_generation_randomness` (which is unconditional and is
    // where the actual proof check happens) before any validator can treat it
    // as valid. So the fields below are trusted transitively through that
    // dependency-validation guarantee, not re-verified live. Same pattern as
    // `blind_collection.rs`'s `load_batch_close`/`load_randomness_result`.
    let RandomnessProvenance::Verified {
        result: result_hash,
    } = &record.randomness;
    let (_, result) = load_generation_randomness(result_hash)?;
    if result.provider_id != spec.provider_id {
        return Ok(ValidateCallbackResult::Invalid(
            "randomness provider does not match the collection's RandomnessSpec".into(),
        ));
    }
    if let Err(e) = check_verified_record(&record, &cfg, &collection_id, result_hash, &result) {
        return Ok(ValidateCallbackResult::Invalid(e));
    }

    let mint = must_get_action(record.nft.clone())?;
    if !is_nft_manifest_create(mint.action()) {
        return Ok(ValidateCallbackResult::Invalid(
            "GenerationRecord.nft is not an NftManifest mint".into(),
        ));
    }
    if mint.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the NFT's minter may author its GenerationRecord".into(),
        ));
    }

    // The author's own chain is strictly ordered, so a prior record for this
    // NFT, or a prior reveal, is visible here. This makes the record
    // single-shot and forces it to precede the reveal.
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("record has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    for a in activity.iter() {
        let act = a.action.action();
        if let ActionData::Update(u) = &act.data {
            if u.original_action_address == record.nft {
                return Ok(ValidateCallbackResult::Invalid(
                    "cannot record generation after the NFT was revealed".into(),
                ));
            }
        }
        if is_create_of(act, UnitEntryTypes::GenerationRecord) {
            let other = load_record(a.action.as_hash().clone())?;
            if other.nft == record.nft {
                return Ok(ValidateCallbackResult::Invalid(
                    "this NFT already has a GenerationRecord".into(),
                ));
            }
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

fn load_record(hash: ActionHash) -> ExternResult<GenerationRecord> {
    must_get_valid_record(hash)?
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "not a GenerationRecord".into()
        )))
}

/// Reveal-time check for a generative collection.
///
/// `author_activity` is the revealing author's chain before the reveal (already
/// fetched by the caller). The record must be on it, i.e. authored by the
/// revealer, who must be the minter (the caller enforces that; delegated
/// reveals are not supported for generative collections in G2).
pub fn validate_generated_reveal(
    cfg: &GenerationConfig,
    mint_hash: &ActionHash,
    author_activity: &[AgentActivity],
    attributes: &[NftAttribute],
) -> ExternResult<ValidateCallbackResult> {
    if let Err(e) = cfg.validate() {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "invalid generation config: {e}"
        )));
    }
    let mut found: Option<GenerationRecord> = None;
    for a in author_activity.iter() {
        if is_create_of(a.action.action(), UnitEntryTypes::GenerationRecord) {
            let r = load_record(a.action.as_hash().clone())?;
            if &r.nft == mint_hash {
                found = Some(r);
                break;
            }
        }
    }
    let Some(record) = found else {
        return Ok(ValidateCallbackResult::Invalid(
            "a generative NFT cannot be revealed without a GenerationRecord".into(),
        ));
    };
    if let Err(e) = check_record_against_config(&record, cfg) {
        return Ok(ValidateCallbackResult::Invalid(e));
    }
    // Recompute from the seed; never trust caller-supplied traits.
    let expected = match generate(cfg, &record.seed) {
        Ok(g) => g.to_attributes(),
        Err(e) => return Ok(ValidateCallbackResult::Invalid(e)),
    };
    // Exact vector equality: order and multiplicity are part of the result.
    if attributes != expected.as_slice() {
        return Ok(ValidateCallbackResult::Invalid(
            "revealed attributes differ from generate(config, recorded seed)".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::{TraitCategory, TraitOption};

    const CID: [u8; 32] = [0xC0; 32];

    fn ah(byte: u8) -> ActionHash {
        let mut raw = vec![0x84, 0x29, 0x24];
        raw.extend_from_slice(&[byte; 32]);
        raw.extend_from_slice(&[0, 0, 0, byte]);
        ActionHash::from_raw_39(raw)
    }

    fn cfg() -> GenerationConfig {
        GenerationConfig {
            generation_version: 1,
            categories: vec![TraitCategory {
                id: "eyes".into(),
                display_name: "Eyes".into(),
                traits: vec![
                    TraitOption {
                        id: "normal".into(),
                        display_name: "Normal".into(),
                        weight: 70,
                    },
                    TraitOption {
                        id: "laser".into(),
                        display_name: "Laser".into(),
                        weight: 30,
                    },
                ],
            }],
        }
    }
    fn result(nft: &ActionHash, randomness: u8) -> GenerationRandomness {
        GenerationRandomness {
            nft: nft.clone(),
            provider_id: "dev-insecure".into(),
            event_id: b"event-1".to_vec(),
            randomness: [randomness; 32],
            proof: vec![0; 32],
        }
    }
    /// A record derived exactly as validation requires.
    fn record_for(
        nft: &ActionHash,
        res: &GenerationRandomness,
        res_hash: &ActionHash,
    ) -> GenerationRecord {
        let seed = generation_seed(
            &CID,
            nft,
            1,
            &res.provider_id,
            &res.event_id,
            &res.randomness,
        );
        GenerationRecord {
            nft: nft.clone(),
            generation_version: 1,
            seed,
            generated_traits: generate(&cfg(), &seed).unwrap(),
            randomness: RandomnessProvenance::Verified {
                result: res_hash.clone(),
            },
        }
    }

    #[test]
    fn record_derived_from_verified_randomness_is_accepted() {
        let (nft, rh) = (ah(1), ah(9));
        let res = result(&nft, 7);
        let rec = record_for(&nft, &res, &rh);
        assert!(check_verified_record(&rec, &cfg(), &CID, &rh, &res).is_ok());
    }

    #[test]
    fn an_arbitrary_supplied_seed_is_rejected() {
        let (nft, rh) = (ah(1), ah(9));
        let res = result(&nft, 7);
        let mut rec = record_for(&nft, &res, &rh);
        // A minter-chosen seed with internally consistent traits is still refused.
        let chosen = GenerationSeed([0x42; 32]);
        rec.seed = chosen;
        rec.generated_traits = generate(&cfg(), &chosen).unwrap();
        assert!(check_verified_record(&rec, &cfg(), &CID, &rh, &res).is_err());
    }

    #[test]
    fn wrong_randomness_nft_collection_and_version_are_rejected() {
        let (nft, rh) = (ah(1), ah(9));
        let res = result(&nft, 7);
        let rec = record_for(&nft, &res, &rh);
        // Different randomness value.
        assert!(check_verified_record(&rec, &cfg(), &CID, &rh, &result(&nft, 8)).is_err());
        // Randomness bound to another NFT.
        let other = ah(2);
        assert!(check_verified_record(&rec, &cfg(), &CID, &rh, &result(&other, 7)).is_err());
        // Another collection changes the derived seed.
        assert!(check_verified_record(&rec, &cfg(), &[0xC1; 32], &rh, &res).is_err());
        // A record for another NFT cites a result for this NFT.
        let mut wrong_nft = rec.clone();
        wrong_nft.nft = other;
        assert!(check_verified_record(&wrong_nft, &cfg(), &CID, &rh, &res).is_err());
        // Wrong generation version, and a config of another version.
        let mut wrong_version = rec.clone();
        wrong_version.generation_version = 2;
        assert!(check_verified_record(&wrong_version, &cfg(), &CID, &rh, &res).is_err());
        let mut v2 = cfg();
        v2.generation_version = 2;
        assert!(check_verified_record(&rec, &v2, &CID, &rh, &res).is_err());
        // Cites a different result hash.
        assert!(check_verified_record(&rec, &cfg(), &CID, &ah(10), &res).is_err());
    }

    #[test]
    fn altered_generated_traits_are_rejected() {
        let (nft, rh) = (ah(1), ah(9));
        let res = result(&nft, 7);
        let rec = record_for(&nft, &res, &rh);
        // Find the other outcome for this config and splice it in.
        let other = (0..=255u8)
            .map(|b| generate(&cfg(), &GenerationSeed([b; 32])).unwrap())
            .find(|g| *g != rec.generated_traits)
            .expect("two seeds with different output exist");
        let mut forged = rec.clone();
        forged.generated_traits = other;
        assert!(check_verified_record(&forged, &cfg(), &CID, &rh, &res).is_err());
        let mut v = rec;
        v.generated_traits.generation_version = 9;
        assert!(check_verified_record(&v, &cfg(), &CID, &rh, &res).is_err());
    }
}
