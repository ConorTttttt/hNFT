//! G5a coordinator for open generative NFTs:
//! `create_generation_randomness` -> `create_generation_record` ->
//! `reveal_generated`.
//!
//! There is no seed input anywhere: the seed is derived from verified
//! per-NFT randomness (`generation_seed`), and `validate()` recomputes and checks
//! everything, so nothing here is trusted by peers. The coordinator only fails
//! early with clear messages. The randomness verifier is DEV-ONLY and INSECURE.

use crate::mint::RevealSecretInput;
use crate::reveal::do_reveal;
use hdk::prelude::*;
use nft_integrity::*;

fn guest(msg: impl Into<String>) -> WasmError {
    wasm_error!(WasmErrorInner::Guest(msg.into()))
}

fn config() -> ExternResult<GenerationConfig> {
    CollectionProperties::generation_config()?.ok_or(guest("this collection is not generative"))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateGenerationRandomnessInput {
    pub nft: ActionHash,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
    pub proof: Vec<u8>,
}

fn generation_context_parts() -> ExternResult<(RandomnessSpec, [u8; 32], u32)> {
    let cfg = config()?;
    let props = CollectionProperties::get()?;
    let spec = props
        .randomness
        .clone()
        .ok_or(guest("this collection has no RandomnessSpec"))?;
    Ok((
        spec,
        CollectionProperties::collection_id_bytes()?,
        cfg.generation_version,
    ))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DevGenerationProofInput {
    pub nft: ActionHash,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
}

/// The proof the INSECURE dev verifier accepts for this collection's spec.
/// Public and forgeable by design; there is no production provider.
#[hdk_extern]
pub fn dev_generation_randomness_proof(input: DevGenerationProofInput) -> ExternResult<Vec<u8>> {
    let (spec, collection_id, generation_version) = generation_context_parts()?;
    let ctx = GenerationVerificationContext {
        collection_id: &collection_id,
        nft: &input.nft,
        generation_version,
    };
    dev_generation_proof_for_spec(&spec, &ctx, &input.event_id, &input.randomness)
        .map_err(|e| guest(e.to_string()))
}

/// Record verified randomness for an NFT (minter-only, once per NFT, before any
/// reveal; validation verifies the proof locally). `provider_id` comes from the
/// collection's `RandomnessSpec`.
#[hdk_extern]
pub fn create_generation_randomness(
    input: CreateGenerationRandomnessInput,
) -> ExternResult<ActionHash> {
    let (spec, _, _) = generation_context_parts()?;
    let hash = create_entry(&EntryTypes::GenerationRandomness(GenerationRandomness {
        nft: input.nft.clone(),
        provider_id: spec.provider_id,
        event_id: input.event_id,
        randomness: input.randomness,
        proof: input.proof,
    }))?;
    create_link(
        input.nft,
        hash.clone(),
        LinkTypes::NftToGenerationRandomness,
        (),
    )?;
    Ok(hash)
}

/// Returns the randomness's own action hash alongside the entry: the UI needs
/// that hash later, to pass as `CreateGenerationRecordInput.result` (which the
/// discovery link alone doesn't give a caller without a second round trip).
#[hdk_extern]
pub fn get_generation_randomness(
    nft: ActionHash,
) -> ExternResult<Option<(ActionHash, GenerationRandomness)>> {
    let Some(hash) = first_link_target(&nft, LinkTypes::NftToGenerationRandomness)? else {
        return Ok(None);
    };
    let entry: GenerationRandomness = get(hash.clone(), GetOptions::default())?
        .ok_or(guest("GenerationRandomness not found"))?
        .entry()
        .to_app_option()
        .map_err(|e| guest(format!("{e:?}")))?
        .ok_or(guest("not a GenerationRandomness"))?;
    Ok(Some((hash, entry)))
}

fn first_link_target(base: &ActionHash, link_type: LinkTypes) -> ExternResult<Option<ActionHash>> {
    let mut links = get_links(
        LinkQuery::try_new(base.clone(), link_type)?,
        GetStrategy::Network,
    )?;
    links.sort_by_key(|l| l.timestamp);
    Ok(links.into_iter().find_map(|l| l.target.into_action_hash()))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateGenerationRecordInput {
    pub nft: ActionHash,
    /// The `GenerationRandomness` this record derives from. There is no seed
    /// input: the seed is derived from it.
    pub result: ActionHash,
}

pub(crate) fn generation_record_hash_for(nft: &ActionHash) -> ExternResult<Option<ActionHash>> {
    first_link_target(nft, LinkTypes::NftToGeneration)
}

pub(crate) fn load_generation_record(nft: &ActionHash) -> ExternResult<Option<GenerationRecord>> {
    let Some(hash) = generation_record_hash_for(nft)? else {
        return Ok(None);
    };
    get(hash, GetOptions::default())?
        .ok_or(guest("GenerationRecord not found"))?
        .entry()
        .to_app_option()
        .map_err(|e| guest(format!("{e:?}")))
}

/// Attributes a generative NFT must reveal: `generate(config, recorded seed)`.
pub(crate) fn expected_attributes(nft: &ActionHash) -> ExternResult<Vec<NftAttribute>> {
    let cfg = config()?;
    let record = load_generation_record(nft)?.ok_or(guest(
        "a generative NFT needs a GenerationRecord before it can be revealed",
    ))?;
    check_record_against_config(&record, &cfg).map_err(guest)?;
    Ok(generate(&cfg, &record.seed).map_err(guest)?.to_attributes())
}

/// Derive the seed from the cited verified randomness and record the traits.
/// Minter-only, single-shot, before reveal (all enforced again by `validate`).
/// The seed is never supplied.
#[hdk_extern]
pub fn create_generation_record(input: CreateGenerationRecordInput) -> ExternResult<Record> {
    let cfg = config()?;
    let collection_id = CollectionProperties::collection_id_bytes()?;
    let mint =
        get(input.nft.clone(), GetOptions::default())?.ok_or(guest("NftManifest not found"))?;
    if mint.action().author() != &agent_info()?.agent_initial_pubkey {
        return Err(guest("only the NFT's minter may record its generation"));
    }
    let result_rec = get(input.result.clone(), GetOptions::default())?
        .ok_or(guest("GenerationRandomness not found"))?;
    let result: GenerationRandomness = result_rec
        .entry()
        .to_app_option()
        .map_err(|e| guest(format!("{e:?}")))?
        .ok_or(guest("not a GenerationRandomness"))?;

    let seed = generation_seed(
        &collection_id,
        &input.nft,
        cfg.generation_version,
        &result.provider_id,
        &result.event_id,
        &result.randomness,
    );
    let generated_traits = generate(&cfg, &seed).map_err(guest)?;
    let record = GenerationRecord {
        nft: input.nft.clone(),
        generation_version: cfg.generation_version,
        seed,
        generated_traits,
        randomness: RandomnessProvenance::Verified {
            result: input.result,
        },
    };
    let hash = create_entry(&EntryTypes::GenerationRecord(record))?;
    create_link(input.nft, hash.clone(), LinkTypes::NftToGeneration, ())?;
    get(hash, GetOptions::default())?.ok_or(guest("GenerationRecord not found"))
}

#[hdk_extern]
pub fn get_generation_record(nft: ActionHash) -> ExternResult<Option<GenerationRecord>> {
    load_generation_record(&nft)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RevealGeneratedInput {
    pub nft: ActionHash,
    pub previous_action_hash: ActionHash,
    pub salt: [u8; 32],
    pub image_uri: String,
}

/// Reveal a generative NFT with the attributes derived from its record.
#[hdk_extern]
pub fn reveal_generated(input: RevealGeneratedInput) -> ExternResult<Record> {
    let attributes = expected_attributes(&input.nft)?;
    do_reveal(
        input.nft,
        input.previous_action_hash,
        RevealSecretInput {
            salt: input.salt,
            attributes,
            image_uri: input.image_uri,
        },
    )
}
