//! G5a: NFT -> verified randomness -> deterministic seed -> traits ->
//! GenerationRecord -> reveal. The randomness verifier is the DEV-ONLY, INSECURE
//! `dev-insecure` scheme: these tests exercise the protocol, not secure randomness.

use holochain::prelude::*;
use holochain::sweettest::*;
use nft::generation::{
    CreateGenerationRandomnessInput, CreateGenerationRecordInput, DevGenerationProofInput,
    RevealGeneratedInput,
};
use nft::mint::{MintNftInput, MintNftOutput};
use nft::reveal::RevealNftInput;
use nft_integrity::*;

mod common;
use common::*;

fn opt(id: &str, weight: u32) -> TraitOption {
    TraitOption {
        id: id.into(),
        display_name: id.into(),
        weight,
    }
}

fn gen_config() -> GenerationConfig {
    GenerationConfig {
        generation_version: 1,
        categories: vec![
            TraitCategory {
                id: "background".into(),
                display_name: "Background".into(),
                traits: vec![
                    opt("blue", 50),
                    opt("red", 30),
                    opt("cosmic", 15),
                    opt("gold", 5),
                ],
            },
            TraitCategory {
                id: "eyes".into(),
                display_name: "Eyes".into(),
                traits: vec![opt("normal", 70), opt("laser", 20), opt("diamond", 10)],
            },
        ],
    }
}

/// A generative collection: config + explicit collection_id + dev randomness spec.
fn gen_props() -> CollectionProperties {
    CollectionProperties {
        generation: Some(gen_config()),
        ..blind_props(300)
    }
}

const EVENT: &[u8] = b"event-1";
const RANDOM: [u8; 32] = [7u8; 32];

async fn mint(c: &SweetConductor, cell: &SweetZome) -> ActionHash {
    // Traits don't exist at mint, so the commitment is only a non-zero placeholder.
    let out: MintNftOutput = c
        .call(
            cell,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(1),
            },
        )
        .await;
    out.nft
}

async fn dev_proof(
    c: &SweetConductor,
    cell: &SweetZome,
    nft: &ActionHash,
    event: &[u8],
    randomness: [u8; 32],
) -> Vec<u8> {
    c.call(
        cell,
        "dev_generation_randomness_proof",
        DevGenerationProofInput {
            nft: nft.clone(),
            event_id: event.to_vec(),
            randomness,
        },
    )
    .await
}

fn rand_input(
    nft: &ActionHash,
    event: &[u8],
    randomness: [u8; 32],
    proof: Vec<u8>,
) -> CreateGenerationRandomnessInput {
    CreateGenerationRandomnessInput {
        nft: nft.clone(),
        event_id: event.to_vec(),
        randomness,
        proof,
    }
}

/// Record valid dev randomness for `nft`.
async fn record_randomness(c: &SweetConductor, cell: &SweetZome, nft: &ActionHash) -> ActionHash {
    let proof = dev_proof(c, cell, nft, EVENT, RANDOM).await;
    c.call(
        cell,
        "create_generation_randomness",
        rand_input(nft, EVENT, RANDOM, proof),
    )
    .await
}

fn record_input(nft: &ActionHash, result: &ActionHash) -> CreateGenerationRecordInput {
    CreateGenerationRecordInput {
        nft: nft.clone(),
        result: result.clone(),
    }
}

fn reveal_input(nft: &ActionHash, attributes: Vec<NftAttribute>) -> RevealNftInput {
    RevealNftInput {
        nft: nft.clone(),
        previous_action_hash: nft.clone(),
        salt: [0u8; 32],
        attributes,
        image_uri: "ipfs://demo".into(),
        force: false,
    }
}

fn reveal_generated_input(nft: &ActionHash) -> RevealGeneratedInput {
    RevealGeneratedInput {
        nft: nft.clone(),
        previous_action_hash: nft.clone(),
        salt: [0u8; 32],
        image_uri: "ipfs://demo".into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn verified_randomness_to_reveal_end_to_end() {
    let (conductors, cells) = setup(1, Some(gen_props())).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let nft = mint(c, &alice).await;

    // 1. valid generation randomness (discoverable by link)
    let result = record_randomness(c, &alice, &nft).await;
    let stored_result: Option<(ActionHash, GenerationRandomness)> = c
        .call(&alice, "get_generation_randomness", nft.clone())
        .await;
    let (stored_result_hash, stored_result) = stored_result.unwrap();
    assert_eq!(stored_result_hash, result);
    assert_eq!(stored_result.nft, nft);
    assert_eq!(stored_result.provider_id, INSECURE_DEV_PROVIDER_ID);

    // 2-4. valid record; seed is the frozen derivation; traits are the frozen generator's
    let rec: Record = c
        .call(
            &alice,
            "create_generation_record",
            record_input(&nft, &result),
        )
        .await;
    let stored = GenerationRecord::try_from(rec).unwrap();
    let expected_seed = generation_seed(
        &BLIND_COLLECTION_ID,
        &nft,
        1,
        INSECURE_DEV_PROVIDER_ID,
        EVENT,
        &RANDOM,
    );
    assert_eq!(
        stored.seed, expected_seed,
        "the seed is derived, never supplied"
    );
    assert_eq!(
        stored.generated_traits,
        generate(&gen_config(), &expected_seed).unwrap()
    );
    assert_eq!(
        stored.randomness,
        RandomnessProvenance::Verified {
            result: result.clone()
        }
    );
    assert_eq!(stored.generation_version, 1);
    let fetched: Option<GenerationRecord> =
        c.call(&alice, "get_generation_record", nft.clone()).await;
    assert_eq!(fetched.unwrap(), stored);

    // 5. reveal succeeds with exactly the generated attributes
    let revealed: Record = c
        .call(&alice, "reveal_generated", reveal_generated_input(&nft))
        .await;
    match NftManifest::try_from(revealed).unwrap().payload {
        NftPayload::Revealed { attributes, .. } => {
            assert_eq!(attributes, stored.generated_traits.to_attributes())
        }
        _ => panic!("expected Revealed"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_supplied_seed_and_reveal_needs_verified_randomness() {
    let (conductors, cells) = setup(1, Some(gen_props())).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let nft = mint(c, &alice).await;

    // The old seed-supplied shape is gone: the input has no `result`, so it cannot
    // even be constructed against the new extern.
    #[derive(serde::Serialize, Debug)]
    struct LegacyInput {
        nft: ActionHash,
        seed: [u8; 32],
    }
    let legacy: Result<Record, _> = c
        .call_fallible(
            &alice,
            "create_generation_record",
            LegacyInput {
                nft: nft.clone(),
                seed: [0x42; 32],
            },
        )
        .await;
    assert!(
        legacy.is_err(),
        "an arbitrary supplied seed must not be accepted"
    );

    // Reveal before any randomness or record: every route fails.
    let attrs = generate(&gen_config(), &GenerationSeed([0x42; 32]))
        .unwrap()
        .to_attributes();
    let r: Result<Record, _> = c
        .call_fallible(&alice, "reveal_nft", reveal_input(&nft, attrs.clone()))
        .await;
    assert!(r.is_err(), "reveal without verified randomness must fail");
    let r: Result<Record, _> = c
        .call_fallible(&alice, "reveal_generated", reveal_generated_input(&nft))
        .await;
    assert!(r.is_err());

    // Randomness alone is not enough: still no record, still no reveal.
    let result = record_randomness(c, &alice, &nft).await;
    let r: Result<Record, _> = c
        .call_fallible(&alice, "reveal_generated", reveal_generated_input(&nft))
        .await;
    assert!(r.is_err(), "randomness without a record cannot reveal");

    let _: Record = c
        .call(
            &alice,
            "create_generation_record",
            record_input(&nft, &result),
        )
        .await;
    let good = generate(
        &gen_config(),
        &generation_seed(
            &BLIND_COLLECTION_ID,
            &nft,
            1,
            INSECURE_DEV_PROVIDER_ID,
            EVENT,
            &RANDOM,
        ),
    )
    .unwrap()
    .to_attributes();
    // Altered or minter-chosen attributes are rejected; the derived ones are accepted.
    let mut altered = good.clone();
    altered[0].value = "gold".into();
    if altered[0].value == good[0].value {
        altered[0].value = "blue".into();
    }
    for bad in [altered, attrs, vec![]] {
        let r: Result<Record, _> = c
            .call_fallible(&alice, "reveal_nft", reveal_input(&nft, bad))
            .await;
        assert!(r.is_err(), "non-derived attributes must be rejected");
    }
    let ok: Record = c.call(&alice, "reveal_nft", reveal_input(&nft, good)).await;
    assert!(matches!(
        NftManifest::try_from(ok).unwrap().payload,
        NftPayload::Revealed { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn randomness_is_bound_to_nft_collection_event_and_value() {
    let (conductors, cells) = setup(1, Some(gen_props())).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let nft1 = mint(c, &alice).await;
    let nft2 = mint(c, &alice).await;

    let proof1 = dev_proof(c, &alice, &nft1, EVENT, RANDOM).await;

    // Randomness for another NFT: nft1's proof cannot authorise nft2.
    let r: Result<ActionHash, _> = c
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&nft2, EVENT, RANDOM, proof1.clone()),
        )
        .await;
    assert!(r.is_err(), "another NFT's randomness must be rejected");

    // Another collection: a proof computed for a different collection_id.
    let other_cid = [0xC1u8; 32];
    let ctx = GenerationVerificationContext {
        collection_id: &other_cid,
        nft: &nft1,
        generation_version: 1,
    };
    let foreign = dev_generation_proof("", &ctx, INSECURE_DEV_PROVIDER_ID, EVENT, &RANDOM);
    let r: Result<ActionHash, _> = c
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&nft1, EVENT, RANDOM, foreign),
        )
        .await;
    assert!(
        r.is_err(),
        "another collection's randomness must be rejected"
    );

    // Wrong generation version: a proof computed for version 2.
    let ctx_v2 = GenerationVerificationContext {
        collection_id: &BLIND_COLLECTION_ID,
        nft: &nft1,
        generation_version: 2,
    };
    let v2 = dev_generation_proof("", &ctx_v2, INSECURE_DEV_PROVIDER_ID, EVENT, &RANDOM);
    let r: Result<ActionHash, _> = c
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&nft1, EVENT, RANDOM, v2),
        )
        .await;
    assert!(
        r.is_err(),
        "a proof for another generation_version must be rejected"
    );

    // Wrong event, wrong randomness, malformed proof.
    for bad in [
        rand_input(&nft1, b"event-2", RANDOM, proof1.clone()),
        rand_input(&nft1, EVENT, [8u8; 32], proof1.clone()),
        rand_input(&nft1, EVENT, RANDOM, vec![0u8; 31]),
        rand_input(&nft1, &[], RANDOM, proof1.clone()),
    ] {
        let r: Result<ActionHash, _> = c
            .call_fallible(&alice, "create_generation_randomness", bad)
            .await;
        assert!(r.is_err());
    }

    // The right one is accepted, and a record cannot cite another NFT's result.
    let r1 = record_randomness(c, &alice, &nft1).await;
    let r2 = record_randomness(c, &alice, &nft2).await;
    let cross: Result<Record, _> = c
        .call_fallible(&alice, "create_generation_record", record_input(&nft1, &r2))
        .await;
    assert!(
        cross.is_err(),
        "a record must cite randomness bound to its own NFT"
    );
    let _: Record = c
        .call(&alice, "create_generation_record", record_input(&nft1, &r1))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unsupported_provider_is_rejected() {
    let mut props = gen_props();
    props.randomness = Some(RandomnessSpec {
        provider_id: "not-registered".into(),
        verifier_params: String::new(),
    });
    let (conductors, cells) = setup(1, Some(props)).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let nft = mint(c, &alice).await;
    let r: Result<ActionHash, _> = c
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&nft, EVENT, RANDOM, vec![0u8; 32]),
        )
        .await;
    assert!(r.is_err(), "no verifier exists for this provider");
}

#[tokio::test(flavor = "multi_thread")]
async fn authority_and_single_shot_rules() {
    let (conductors, cells) = setup(2, Some(gen_props())).await;
    let alice = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let nft = mint(ca, &alice).await;
    await_consistency(&cells).await.unwrap();

    // A non-minter can neither record randomness nor a generation record.
    let bob = cells[1].zome(ZOME);
    let cb = conductors.get(1).unwrap();
    let proof = dev_proof(cb, &bob, &nft, EVENT, RANDOM).await;
    let r: Result<ActionHash, _> = cb
        .call_fallible(
            &bob,
            "create_generation_randomness",
            rand_input(&nft, EVENT, RANDOM, proof.clone()),
        )
        .await;
    assert!(
        r.is_err(),
        "only the NFT's minter may record its randomness"
    );

    let result = record_randomness(ca, &alice, &nft).await;
    // Second randomness for the same NFT (even with different, valid values) is rejected.
    let proof2 = dev_proof(ca, &alice, &nft, b"event-2", [9u8; 32]).await;
    let dup: Result<ActionHash, _> = ca
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&nft, b"event-2", [9u8; 32], proof2),
        )
        .await;
    assert!(
        dup.is_err(),
        "one GenerationRandomness per NFT on the minter's chain"
    );
    await_consistency(&cells).await.unwrap();

    let r: Result<Record, _> = cb
        .call_fallible(
            &bob,
            "create_generation_record",
            record_input(&nft, &result),
        )
        .await;
    assert!(r.is_err(), "only the minter may author the record");

    let _: Record = ca
        .call(
            &alice,
            "create_generation_record",
            record_input(&nft, &result),
        )
        .await;
    let dup: Result<Record, _> = ca
        .call_fallible(
            &alice,
            "create_generation_record",
            record_input(&nft, &result),
        )
        .await;
    assert!(
        dup.is_err(),
        "one GenerationRecord per NFT on the minter's chain"
    );
    let stored: Option<GenerationRecord> =
        ca.call(&alice, "get_generation_record", nft.clone()).await;
    assert_eq!(
        stored.unwrap().seed,
        generation_seed(
            &BLIND_COLLECTION_ID,
            &nft,
            1,
            INSECURE_DEV_PROVIDER_ID,
            EVENT,
            &RANDOM
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn non_generative_collection_is_unchanged() {
    let (conductors, cells) = setup(1, None).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();
    let salt = [3u8; 32];
    let attributes = vec![NftAttribute {
        trait_type: "k".into(),
        value: "v".into(),
    }];
    let uri = "ipfs://x".to_string();
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &uri));
    let out: MintNftOutput = c
        .call(
            &alice,
            "mint_nft",
            MintNftInput {
                metadata_hash: commitment,
            },
        )
        .await;

    // Generative externs are refused: this collection isn't generative.
    let r: Result<ActionHash, _> = c
        .call_fallible(
            &alice,
            "create_generation_randomness",
            rand_input(&out.nft, EVENT, RANDOM, vec![0u8; 32]),
        )
        .await;
    assert!(r.is_err());

    let mut bad = reveal_input(&out.nft, attributes.clone());
    bad.salt = [9u8; 32];
    bad.image_uri = uri.clone();
    assert!(c
        .call_fallible::<_, Record>(&alice, "reveal_nft", bad)
        .await
        .is_err());
    let mut good = reveal_input(&out.nft, attributes);
    good.salt = salt;
    good.image_uri = uri;
    let rec: Record = c.call(&alice, "reveal_nft", good).await;
    assert!(matches!(
        NftManifest::try_from(rec).unwrap().payload,
        NftPayload::Revealed { .. }
    ));
}

/// Generative collections need collection_id + randomness + a valid, bounded config.
#[tokio::test(flavor = "multi_thread")]
async fn invalid_generative_collections_cannot_create_a_cell() {
    let mut zero = gen_config();
    zero.categories[0]
        .traits
        .iter_mut()
        .for_each(|t| t.weight = 0);
    let too_many = GenerationConfig {
        generation_version: 1,
        categories: (0..33)
            .map(|i| TraitCategory {
                id: format!("c{i}"),
                display_name: "C".into(),
                traits: vec![opt("t", 1)],
            })
            .collect(),
    };
    for bad in [
        CollectionProperties {
            generation: Some(zero),
            ..gen_props()
        },
        CollectionProperties {
            generation: Some(too_many),
            ..gen_props()
        },
        CollectionProperties {
            collection_id: None,
            ..gen_props()
        },
        CollectionProperties {
            randomness: None,
            ..gen_props()
        },
    ] {
        let mut conductors = SweetConductorBatch::standard(1).await;
        let dna_file = dna(Some(bad)).await;
        assert!(conductors.setup_app("nft", &[dna_file]).await.is_err());
    }
}
