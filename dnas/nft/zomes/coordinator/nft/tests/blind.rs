//! G3a blind fixed-inventory batches: the full lifecycle plus the adversarial
//! cases reachable through the public coordinator API.
//!
//! Lifecycle under test:
//! create batch -> mint sealed NFTs -> finalize inventory -> purchases ->
//! close (1 <= K <= N) -> randomness (DEV-ONLY verifier) -> deterministic
//! assignment -> release transfer -> accept -> reveal.
//!
//! Cases that need a hand-forged raw entry (e.g. a `TransferCommitment` naming
//! the wrong recipient) can't be produced through the coordinator, which always
//! derives the recipient from the purchase; those rules are covered by the pure
//! unit tests in `blind_collection.rs`. What IS exercised end-to-end here is
//! that validation independently rejects a wrong NFT for a purchase, an
//! unselected NFT, a twin NFT, and the ordinary-transfer bypass.

use holochain::prelude::*;
use holochain::sweettest::*;
use nft::blind::*;
use nft::mint::{MintNftInput, MintNftOutput};
use nft::reveal::RevealNftInput;
use nft::transfer::{CommitmentRef, InitiateTransferInput, InitiateTransferOutput};
use nft_integrity::*;

mod common;
use common::*;

struct Pre {
    salt: [u8; 32],
    attributes: Vec<NftAttribute>,
    image_uri: String,
    commitment: [u8; 32],
}

fn pre(slot: u32) -> Pre {
    let salt = [slot as u8 + 1; 32];
    let attributes = vec![NftAttribute {
        trait_type: "slot-art".into(),
        value: format!("{slot}"),
    }];
    let image_uri = format!("ipfs://blind/{slot}");
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri));
    Pre {
        salt,
        attributes,
        image_uri,
        commitment,
    }
}

struct Batch {
    batch: ActionHash,
    nfts: Vec<ActionHash>,
    pre: Vec<Pre>,
}

const ALICE: usize = 0;

fn z(cells: &[SweetCell], i: usize) -> SweetZome {
    cells[i].zome(ZOME)
}

/// Alice creates a batch of `n`, mints all NFTs, finalizes the inventory.
async fn build_batch(conductors: &SweetConductorBatch, cells: &[SweetCell], n: u32) -> Batch {
    let c = conductors.get(ALICE).unwrap();
    let alice = z(cells, ALICE);
    let batch: ActionHash = c
        .call(
            &alice,
            "create_blind_batch",
            CreateBlindBatchInput {
                batch_size: n,
                sale_window_secs: 0,
            },
        )
        .await;
    let pre: Vec<Pre> = (0..n).map(pre).collect();
    let nfts: Vec<ActionHash> = c
        .call(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: batch.clone(),
                start_slot: 0,
                metadata_hashes: pre.iter().map(|p| p.commitment).collect(),
            },
        )
        .await;
    let _inventory: ActionHash = c.call(&alice, "finalize_inventory", batch.clone()).await;
    await_consistency(cells).await.unwrap();
    Batch { batch, nfts, pre }
}

async fn reserve(
    conductors: &SweetConductorBatch,
    cells: &[SweetCell],
    who: usize,
    batch: &ActionHash,
) -> ActionHash {
    let out: ReserveBlindPurchaseOutput = conductors
        .get(who)
        .unwrap()
        .call(
            &z(cells, who),
            "reserve_blind_purchase",
            ReserveBlindPurchaseInput {
                batch: batch.clone(),
            },
        )
        .await;
    out.purchase
}

async fn close(
    conductors: &SweetConductorBatch,
    cells: &[SweetCell],
    batch: &ActionHash,
    purchases: Option<Vec<ActionHash>>,
) -> Result<ActionHash, String> {
    conductors
        .get(ALICE)
        .unwrap()
        .call_fallible(
            &z(cells, ALICE),
            "close_batch",
            CloseBatchInput {
                batch: batch.clone(),
                purchases,
            },
        )
        .await
        .map_err(|e| format!("{e:?}"))
}

/// Record dev randomness for `close`, authored by `by` (anyone may).
async fn randomize(
    conductors: &SweetConductorBatch,
    cells: &[SweetCell],
    by: usize,
    close: &ActionHash,
    randomness: [u8; 32],
) -> ActionHash {
    let c = conductors.get(by).unwrap();
    let event_id = b"event-1".to_vec();
    let proof: Vec<u8> = c
        .call(
            &z(cells, by),
            "dev_randomness_proof",
            DevProofInput {
                close: close.clone(),
                event_id: event_id.clone(),
                randomness,
            },
        )
        .await;
    c.call(
        &z(cells, by),
        "record_randomness",
        RecordRandomnessInput {
            close: close.clone(),
            event_id,
            randomness,
            proof,
        },
    )
    .await
}

async fn assignments(
    conductors: &SweetConductorBatch,
    cells: &[SweetCell],
    close: &ActionHash,
    result: &ActionHash,
) -> Vec<AssignmentView> {
    conductors
        .get(ALICE)
        .unwrap()
        .call(
            &z(cells, ALICE),
            "get_assignments",
            GetAssignmentsInput {
                close: close.clone(),
                result: result.clone(),
            },
        )
        .await
}

async fn release(
    conductors: &SweetConductorBatch,
    cells: &[SweetCell],
    nft: &ActionHash,
    close: &ActionHash,
    result: &ActionHash,
    purchase: &ActionHash,
) -> Result<ReleaseBatchTransferOutput, String> {
    conductors
        .get(ALICE)
        .unwrap()
        .call_fallible(
            &z(cells, ALICE),
            "release_batch_transfer",
            ReleaseBatchTransferInput {
                nft: nft.clone(),
                close: close.clone(),
                result: result.clone(),
                purchase: purchase.clone(),
            },
        )
        .await
        .map_err(|e| format!("{e:?}"))
}

fn agent_index(cells: &[SweetCell], key: &AgentPubKey) -> usize {
    cells
        .iter()
        .position(|c| c.agent_pubkey() == key)
        .expect("buyer is one of the test agents")
}

/// The whole flow for `n` NFTs and the given buyers (agent index per purchase).
async fn run_lifecycle(n: u32, buyers: &[usize], agents: usize) {
    let (conductors, cells) = setup(agents, Some(blind_props(300))).await;
    let alice = z(&cells, ALICE);
    let ca = conductors.get(ALICE).unwrap();
    let b = build_batch(&conductors, &cells, n).await;

    let state: BatchStateView = ca.call(&alice, "get_batch_state", b.batch.clone()).await;
    assert_eq!(state.state, "SALES_OPEN");

    let mut purchases = Vec::new();
    for who in buyers {
        purchases.push(reserve(&conductors, &cells, *who, &b.batch).await);
    }
    await_consistency(&cells).await.unwrap();

    let k = buyers.len() as u32;
    let close_hash = close(&conductors, &cells, &b.batch, None)
        .await
        .expect("K in 1..=N closes");
    await_consistency(&cells).await.unwrap();
    let state: BatchStateView = ca.call(&alice, "get_batch_state", b.batch.clone()).await;
    assert_eq!(state.state, "CLOSED");

    // Anyone (here: the last agent) may record the randomness.
    let by = agents - 1;
    let result = randomize(&conductors, &cells, by, &close_hash, [7u8; 32]).await;
    await_consistency(&cells).await.unwrap();
    let state: BatchStateView = ca.call(&alice, "get_batch_state", b.batch.clone()).await;
    assert_eq!(state.state, "RANDOMNESS_FIXED");

    let views = assignments(&conductors, &cells, &close_hash, &result).await;
    assert_eq!(views.len() as u32, k, "exactly K assignments");
    let mut used: Vec<u32> = views.iter().map(|v| v.slot).collect();
    used.sort();
    used.dedup();
    assert_eq!(used.len() as u32, k, "no NFT assigned twice");
    assert!(views.iter().all(|v| v.slot < n));

    // Independent recomputation with the shared pure functions.
    let seed = assignment_seed(
        &BLIND_COLLECTION_ID,
        &b.batch,
        &close_hash,
        INSECURE_DEV_PROVIDER_ID,
        b"event-1",
        &[7u8; 32],
    );
    let slots = assign_slots(&seed, n, k).unwrap();
    let sorted = canonical_purchase_order(purchases.clone());
    for (i, v) in views.iter().enumerate() {
        assert_eq!(v.purchase, sorted[i]);
        assert_eq!(v.slot, slots[i]);
        assert_eq!(v.nft, b.nfts[slots[i] as usize]);
    }

    // Wrong NFT for a purchase is rejected by validation (needs >= 2 purchases).
    if views.len() >= 2 {
        let r = release(
            &conductors,
            &cells,
            &views[1].nft,
            &close_hash,
            &result,
            &views[0].purchase,
        )
        .await;
        assert!(
            r.is_err(),
            "an NFT assigned to another purchase must be rejected"
        );
    }
    // An unselected NFT has no release path (K < N).
    let unselected: Vec<u32> = (0..n).filter(|s| !used.contains(s)).collect();
    for s in &unselected {
        let r = release(
            &conductors,
            &cells,
            &b.nfts[*s as usize],
            &close_hash,
            &result,
            &views[0].purchase,
        )
        .await;
        assert!(r.is_err(), "unselected slot {s} must stay locked");
    }
    // Ordinary transfer cannot bypass the assignment: selected or not.
    let bypass_to = cells[agents - 1].agent_pubkey().clone();
    for nft in [&views[0].nft]
        .into_iter()
        .chain(unselected.first().map(|s| &b.nfts[*s as usize]))
    {
        let r: Result<InitiateTransferOutput, _> = ca
            .call_fallible(
                &alice,
                "initiate_transfer",
                InitiateTransferInput {
                    nft: nft.clone(),
                    to_agent: bypass_to.clone(),
                },
            )
            .await;
        assert!(
            r.is_err(),
            "ordinary initiate_transfer of a batch NFT must fail"
        );
    }

    // Correct release -> the assigned buyer accepts -> ownership follows.
    for v in &views {
        let out = release(
            &conductors,
            &cells,
            &v.nft,
            &close_hash,
            &result,
            &v.purchase,
        )
        .await
        .expect("the assigned NFT releases to the assigned buyer");
        await_consistency(&cells).await.unwrap();
        let bi = agent_index(&cells, &v.buyer);
        let _receipt: Record = conductors
            .get(bi)
            .unwrap()
            .call(
                &z(&cells, bi),
                "accept_transfer",
                CommitmentRef {
                    commitment: out.commitment,
                },
            )
            .await;
        await_consistency(&cells).await.unwrap();
        let owner: AgentPubKey = ca.call(&alice, "current_owner", v.nft.clone()).await;
        assert_eq!(owner, v.buyer);
    }

    // Reveal after a valid transfer.
    let first = &views[0];
    let slot = first.slot as usize;
    let revealed: Record = ca
        .call(
            &alice,
            "reveal_nft",
            RevealNftInput {
                nft: first.nft.clone(),
                previous_action_hash: first.nft.clone(),
                salt: b.pre[slot].salt,
                attributes: b.pre[slot].attributes.clone(),
                image_uri: b.pre[slot].image_uri.clone(),
                force: false,
            },
        )
        .await;
    assert!(matches!(
        NftManifest::try_from(revealed).unwrap().payload,
        NftPayload::Revealed { .. }
    ));
}

/// K < N: 3 buyers' purchases select 3 of 5 NFTs; the other 2 stay locked.
#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_k_less_than_n() {
    run_lifecycle(5, &[1, 1, 2], 3).await;
}

/// K == N.
#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_k_equals_n() {
    run_lifecycle(2, &[1, 2], 3).await;
}

/// K == 1.
#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_k_equals_one() {
    run_lifecycle(3, &[1], 2).await;
}

/// Finalization and close are single-shot; a purchase needs a finalized inventory;
/// a finalization needs every slot; the minter chooses the set when K would exceed N.
#[tokio::test(flavor = "multi_thread")]
async fn single_shot_and_ordering_rules() {
    let (conductors, cells) = setup(3, Some(blind_props(300))).await;
    let alice = z(&cells, ALICE);
    let ca = conductors.get(ALICE).unwrap();

    // DRAFT: no purchase yet, and finalizing with missing slots fails.
    let batch: ActionHash = ca
        .call(
            &alice,
            "create_blind_batch",
            CreateBlindBatchInput {
                batch_size: 2,
                sale_window_secs: 0,
            },
        )
        .await;
    let r: Result<ActionHash, _> = ca
        .call_fallible(&alice, "finalize_inventory", batch.clone())
        .await;
    assert!(r.is_err(), "cannot finalize with no NFTs minted");
    let r: Result<ReserveBlindPurchaseOutput, _> = conductors
        .get(1)
        .unwrap()
        .call_fallible(
            &z(&cells, 1),
            "reserve_blind_purchase",
            ReserveBlindPurchaseInput {
                batch: batch.clone(),
            },
        )
        .await;
    assert!(r.is_err(), "no purchase before the inventory is finalized");

    // Mint slot 0 only, still cannot finalize.
    let p = [pre(0), pre(1)];
    let _: Vec<ActionHash> = ca
        .call(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: batch.clone(),
                start_slot: 0,
                metadata_hashes: vec![p[0].commitment],
            },
        )
        .await;
    let r: Result<ActionHash, _> = ca
        .call_fallible(&alice, "finalize_inventory", batch.clone())
        .await;
    assert!(r.is_err(), "slot 1 is still missing");
    let _: Vec<ActionHash> = ca
        .call(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: batch.clone(),
                start_slot: 1,
                metadata_hashes: vec![p[1].commitment],
            },
        )
        .await;
    let _: ActionHash = ca.call(&alice, "finalize_inventory", batch.clone()).await;
    let r: Result<ActionHash, _> = ca
        .call_fallible(&alice, "finalize_inventory", batch.clone())
        .await;
    assert!(
        r.is_err(),
        "a batch has exactly one finalization on the author's chain"
    );
    await_consistency(&cells).await.unwrap();

    // Three purchases for a 2-NFT batch: close must be given an explicit set.
    let mut purchases = Vec::new();
    for who in [1usize, 2, 1] {
        purchases.push(reserve(&conductors, &cells, who, &batch).await);
    }
    await_consistency(&cells).await.unwrap();
    assert!(
        close(&conductors, &cells, &batch, None).await.is_err(),
        "3 purchases > N=2"
    );
    let chosen = canonical_purchase_order(purchases[..2].to_vec());
    let close_hash = close(&conductors, &cells, &batch, Some(chosen.clone()))
        .await
        .expect("explicit K=2 <= N");
    assert!(
        close(&conductors, &cells, &batch, Some(chosen))
            .await
            .is_err(),
        "a batch closes once"
    );
    await_consistency(&cells).await.unwrap();

    // A dev proof for one close cannot be replayed against another.
    let batch2 = build_batch(&conductors, &cells, 1).await;
    let p2 = reserve(&conductors, &cells, 1, &batch2.batch).await;
    await_consistency(&cells).await.unwrap();
    let close2 = close(&conductors, &cells, &batch2.batch, Some(vec![p2]))
        .await
        .unwrap();
    await_consistency(&cells).await.unwrap();
    let proof_for_close1: Vec<u8> = ca
        .call(
            &alice,
            "dev_randomness_proof",
            DevProofInput {
                close: close_hash.clone(),
                event_id: b"event-1".to_vec(),
                randomness: [7; 32],
            },
        )
        .await;
    let replay: Result<ActionHash, _> = ca
        .call_fallible(
            &alice,
            "record_randomness",
            RecordRandomnessInput {
                close: close2,
                event_id: b"event-1".to_vec(),
                randomness: [7; 32],
                proof: proof_for_close1.clone(),
            },
        )
        .await;
    assert!(
        replay.is_err(),
        "a result bound to one close is rejected for another"
    );
    // And a wrong proof for the right close is rejected too.
    let bad: Result<ActionHash, _> = ca
        .call_fallible(
            &alice,
            "record_randomness",
            RecordRandomnessInput {
                close: close_hash.clone(),
                event_id: b"event-1".to_vec(),
                randomness: [8; 32],
                proof: proof_for_close1,
            },
        )
        .await;
    assert!(bad.is_err(), "proof must match the randomness");
}

/// Manifest batch rules, twin NFTs, and mode separation.
#[tokio::test(flavor = "multi_thread")]
async fn manifest_rules_twins_and_mode_separation() {
    let (conductors, cells) = setup(2, Some(blind_props(300))).await;
    let alice = z(&cells, ALICE);
    let ca = conductors.get(ALICE).unwrap();
    let b = build_batch(&conductors, &cells, 2).await;

    // Slot outside the batch.
    let r: Result<Vec<ActionHash>, _> = ca
        .call_fallible(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: b.batch.clone(),
                start_slot: 2,
                metadata_hashes: vec![[9; 32]],
            },
        )
        .await;
    assert!(r.is_err(), "slot >= batch_size");
    // A non-author cannot mint into someone else's batch.
    let r: Result<Vec<ActionHash>, _> = conductors
        .get(1)
        .unwrap()
        .call_fallible(
            &z(&cells, 1),
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: b.batch.clone(),
                start_slot: 0,
                metadata_hashes: vec![[9; 32]],
            },
        )
        .await;
    assert!(r.is_err(), "manifest author must equal batch author");
    // batch must be a BlindBatchCommitment.
    let ordinary: MintNftOutput = ca
        .call(
            &alice,
            "mint_nft",
            MintNftInput {
                metadata_hash: [5; 32],
            },
        )
        .await;
    let r: Result<Vec<ActionHash>, _> = ca
        .call_fallible(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: ordinary.nft,
                start_slot: 0,
                metadata_hashes: vec![[9; 32]],
            },
        )
        .await;
    assert!(r.is_err(), "batch reference is not a batch");

    // Close and randomize first, so we know which slot the buyer is assigned.
    let buyer = reserve(&conductors, &cells, 1, &b.batch).await;
    await_consistency(&cells).await.unwrap();
    let close_hash = close(&conductors, &cells, &b.batch, None).await.unwrap();
    await_consistency(&cells).await.unwrap();
    let result = randomize(&conductors, &cells, 1, &close_hash, [3u8; 32]).await;
    await_consistency(&cells).await.unwrap();
    let views = assignments(&conductors, &cells, &close_hash, &result).await;
    assert_eq!(views[0].purchase, buyer);
    let assigned_slot = views[0].slot;

    // A twin for the ASSIGNED slot is mintable (no mint-time uniqueness) but is
    // not the committed NFT, so it is unusable.
    let twin: Vec<ActionHash> = ca
        .call(
            &alice,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: b.batch.clone(),
                start_slot: assigned_slot,
                metadata_hashes: vec![b.pre[assigned_slot as usize].commitment],
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let r = release(&conductors, &cells, &twin[0], &close_hash, &result, &buyer).await;
    assert!(r.is_err(), "a twin is not the committed NFT for its slot");
    let r: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &alice,
            "initiate_transfer",
            InitiateTransferInput {
                nft: twin[0].clone(),
                to_agent: cells[1].agent_pubkey().clone(),
            },
        )
        .await;
    assert!(
        r.is_err(),
        "a twin batch NFT cannot bypass the protocol either"
    );
    // The committed NFT still releases.
    let ok = release(
        &conductors,
        &cells,
        &views[0].nft,
        &close_hash,
        &result,
        &buyer,
    )
    .await;
    assert!(ok.is_ok(), "the committed NFT releases normally");

    // Mode separation and collection_id requirement (separate DNAs).
    let mut gen_props = blind_props(300);
    gen_props.generation = Some(GenerationConfig {
        generation_version: 1,
        categories: vec![TraitCategory {
            id: "eyes".into(),
            display_name: "Eyes".into(),
            traits: vec![TraitOption {
                id: "a".into(),
                display_name: "A".into(),
                weight: 1,
            }],
        }],
    });
    let (c2, cells2) = setup(1, Some(gen_props)).await;
    let r: Result<ActionHash, _> = c2
        .get(0)
        .unwrap()
        .call_fallible(
            &z(&cells2, 0),
            "create_blind_batch",
            CreateBlindBatchInput {
                batch_size: 2,
                sale_window_secs: 0,
            },
        )
        .await;
    assert!(
        r.is_err(),
        "blind batches cannot exist in a generative collection"
    );

    let (c3, cells3) = setup(1, Some(dev_props(None, 0, 300))).await;
    let r: Result<ActionHash, _> = c3
        .get(0)
        .unwrap()
        .call_fallible(
            &z(&cells3, 0),
            "create_blind_batch",
            CreateBlindBatchInput {
                batch_size: 2,
                sale_window_secs: 0,
            },
        )
        .await;
    assert!(r.is_err(), "blind batches need a collection_id");
}

/// Invalid collection_id / RandomnessSpec cannot create a cell.
#[tokio::test(flavor = "multi_thread")]
async fn invalid_g3_properties_cannot_create_a_cell() {
    for bad in [
        CollectionProperties {
            collection_id: Some("C0".repeat(32)),
            ..blind_props(300)
        },
        CollectionProperties {
            collection_id: Some("c0".repeat(31)),
            ..blind_props(300)
        },
        CollectionProperties {
            collection_id: Some("zz".repeat(32)),
            ..blind_props(300)
        },
        CollectionProperties {
            randomness: Some(RandomnessSpec {
                provider_id: "a".repeat(65),
                verifier_params: String::new(),
            }),
            ..blind_props(300)
        },
    ] {
        let mut conductors = SweetConductorBatch::standard(1).await;
        let dna_file = dna(Some(bad)).await;
        assert!(conductors.setup_app("nft", &[dna_file]).await.is_err());
    }
}

/// G3a rules still govern duplicates: there is no cross-author uniqueness for
/// `RandomnessResult`, so two valid results for one close are both accepted and
/// each yields its own (deterministic) assignment. One accepted result per close
/// is a PROVIDER property (idempotence / non-equivocation), which the dev
/// provider deliberately does not have.
#[tokio::test(flavor = "multi_thread")]
async fn duplicate_randomness_results_are_not_globally_unique() {
    let (conductors, cells) = setup(2, Some(blind_props(300))).await;
    let b = build_batch(&conductors, &cells, 4).await;
    let _p = reserve(&conductors, &cells, 1, &b.batch).await;
    await_consistency(&cells).await.unwrap();
    let close_hash = close(&conductors, &cells, &b.batch, None).await.unwrap();
    await_consistency(&cells).await.unwrap();

    let r1 = randomize(&conductors, &cells, 0, &close_hash, [1u8; 32]).await;
    let r2 = randomize(&conductors, &cells, 1, &close_hash, [2u8; 32]).await;
    assert_ne!(r1, r2, "both results were accepted as distinct entries");
    await_consistency(&cells).await.unwrap();

    let a1 = assignments(&conductors, &cells, &close_hash, &r1).await;
    let a2 = assignments(&conductors, &cells, &close_hash, &r2).await;
    assert_eq!((a1.len(), a2.len()), (1, 1));
    // Each is exactly the pure function of its own result.
    for (res, view) in [([1u8; 32], &a1[0]), ([2u8; 32], &a2[0])] {
        let seed = assignment_seed(
            &BLIND_COLLECTION_ID,
            &b.batch,
            &close_hash,
            INSECURE_DEV_PROVIDER_ID,
            b"event-1",
            &res,
        );
        assert_eq!(view.slot, assign_slots(&seed, 4, 1).unwrap()[0]);
    }
}

/// Phase J.2: WHITELIST mint policy gates blind-box batch minting through the
/// SAME `validate_create_nft_manifest` every mint already goes through — no
/// change to G3a semantics required. The batch author must be whitelisted
/// (and under their allowance) to mint slots into it; the batch's own
/// internal author-consistency rule (`validate_manifest_batch_fields`, "every
/// slot's author must equal the batch's author") is unchanged and unaffected.
#[tokio::test(flavor = "multi_thread")]
async fn whitelist_gates_blind_batch_minting_without_changing_g3a() {
    let mut conductors = SweetConductorBatch::standard(2).await;
    let alice_key = SweetAgents::one(conductors.get(0).unwrap().keystore()).await;
    let eve_key = SweetAgents::one(conductors.get(1).unwrap().keystore()).await;

    let props = CollectionProperties {
        collection_id: Some("c0".repeat(32)),
        randomness: Some(RandomnessSpec {
            provider_id: INSECURE_DEV_PROVIDER_ID.into(),
            verifier_params: String::new(),
        }),
        ..whitelist_props(vec![(alice_key.clone(), 2)], 0, 300)
    };
    let dna_file = dna(Some(props)).await;
    let apps = conductors
        .setup_app_for_zipped_agents("nft", &[alice_key.clone(), eve_key.clone()], &[dna_file])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    let (za, ca) = (cells[0].zome(ZOME), conductors.get(0).unwrap());
    let (ze, ce) = (cells[1].zome(ZOME), conductors.get(1).unwrap());

    // Alice (whitelisted, allowance 2) creates a 2-slot batch and mints both.
    let batch: ActionHash = ca
        .call(
            &za,
            "create_blind_batch",
            CreateBlindBatchInput { batch_size: 2, sale_window_secs: 0 },
        )
        .await;
    let nfts: Vec<ActionHash> = ca
        .call(
            &za,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: batch.clone(),
                start_slot: 0,
                metadata_hashes: vec![metadata_hash(1), metadata_hash(2)],
            },
        )
        .await;
    assert_eq!(nfts.len(), 2);

    // Eve (not whitelisted) cannot mint into her own batch either.
    let eve_batch: ActionHash = ce
        .call(
            &ze,
            "create_blind_batch",
            CreateBlindBatchInput { batch_size: 1, sale_window_secs: 0 },
        )
        .await;
    let eve_mint: Result<Vec<ActionHash>, _> = ce
        .call_fallible(
            &ze,
            "mint_batch_nfts",
            MintBatchNftsInput {
                batch: eve_batch,
                start_slot: 0,
                metadata_hashes: vec![metadata_hash(3)],
            },
        )
        .await;
    assert!(
        eve_mint.is_err(),
        "a non-whitelisted agent must not be able to mint blind-batch slots"
    );
}
