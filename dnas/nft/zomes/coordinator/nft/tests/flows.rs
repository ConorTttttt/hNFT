use holochain::prelude::*;
use holochain::sweettest::*;
use nft::mint::{MintNftInput, MintNftOutput, MintPolicyView, MintStatus};
use nft::reveal::RevealNftInput;
use nft::transfer::{CommitmentRef, InitiateTransferInput, InitiateTransferOutput};
use nft_integrity::*;

mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn mint_and_reveal() {
    let (conductors, cells) = setup(1, None).await;
    let alice = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();

    let salt = [7u8; 32];
    let attributes = vec![NftAttribute {
        trait_type: "background".into(),
        value: "cobalt".into(),
    }];
    let image_uri = "ipfs://demo".to_string();
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri));

    let minted: MintNftOutput = c
        .call(
            &alice,
            "mint_nft",
            MintNftInput {
                metadata_hash: commitment,
            },
        )
        .await;

    // Sealed at mint.
    let manifest = NftManifest::try_from(minted.record.clone()).unwrap();
    assert!(matches!(manifest.payload, NftPayload::Sealed));

    // Reveal with the true preimage succeeds.
    let revealed: Record = c
        .call(
            &alice,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: minted.nft.clone(),
                salt,
                attributes: attributes.clone(),
                image_uri: image_uri.clone(),
                force: false,
            },
        )
        .await;
    let revealed_manifest = NftManifest::try_from(revealed).unwrap();
    assert!(matches!(
        revealed_manifest.payload,
        NftPayload::Revealed { .. }
    ));

    // Reveal with a wrong preimage is rejected by validation.
    let (conductors2, cells2) = setup(1, None).await;
    let bob = cells2[0].zome(ZOME);
    let c2 = conductors2.get(0).unwrap();
    let minted2: MintNftOutput = c2
        .call(
            &bob,
            "mint_nft",
            MintNftInput {
                metadata_hash: commitment,
            },
        )
        .await;
    let bad: Result<Record, _> = c2
        .call_fallible(
            &bob,
            "reveal_nft",
            RevealNftInput {
                nft: minted2.nft.clone(),
                previous_action_hash: minted2.nft.clone(),
                salt: [9u8; 32], // wrong salt
                attributes,
                image_uri,
                force: false,
            },
        )
        .await;
    assert!(bad.is_err(), "reveal with wrong preimage must fail");
}

#[tokio::test(flavor = "multi_thread")]
async fn full_transfer_updates_ownership_and_provenance() {
    let (conductors, cells) = setup(2, None).await;
    let alice = cells[0].zome(ZOME);
    let bob = cells[1].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let cb = conductors.get(1).unwrap();
    let bob_key = cells[1].agent_pubkey().clone();
    let alice_key = cells[0].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &alice,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(1),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let offer: InitiateTransferOutput = ca
        .call(
            &alice,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: bob_key.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let _receipt: Record = cb
        .call(
            &bob,
            "accept_transfer",
            CommitmentRef {
                commitment: offer.commitment.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // Authoritative owner is now Bob.
    let owner: AgentPubKey = cb.call(&bob, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, bob_key);

    // Provenance has exactly one hop, alice -> bob, sequence 1.
    let hops: Vec<nft::query::ProvenanceHop> =
        cb.call(&bob, "get_provenance", minted.nft.clone()).await;
    assert_eq!(hops.len(), 1);
    assert_eq!(hops[0].sequence, 1);
    assert_eq!(hops[0].from_agent, alice_key);
    assert_eq!(hops[0].to_agent, bob_key);

    // Alice can no longer offer the NFT (she doesn't hold seq-0 state anymore).
    let bad: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &alice,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: bob_key.clone(),
            },
        )
        .await;
    assert!(bad.is_err(), "seller must not re-offer a transferred NFT");
}

/// Architecture doc §7.1 — the item flagged to spike before trusting the cap.
/// Confirms `validate_create_nft_manifest`'s `must_get_agent_activity` walk and
/// entry-type filtering actually reject the (max_supply + 1)-th mint.
#[tokio::test(flavor = "multi_thread")]
async fn max_supply_cap_is_enforced_for_a_single_minter() {
    // The minter key must be known BEFORE the DNA is built, since it goes into
    // `modifiers.properties` (part of the DNA hash). Pre-generate it, then install
    // the app for exactly that agent.
    let mut conductor = SweetConductor::standard().await;
    let minter = SweetAgents::one(conductor.keystore()).await;
    let dna_file = dna(Some(dev_props(Some(minter.clone()), 2, 300))).await;
    let app = conductor
        .setup_app_for_agent("nft", minter.clone(), &[dna_file])
        .await
        .unwrap();
    let zome = app.cells()[0].zome(ZOME);

    let _m1: MintNftOutput = conductor
        .call(
            &zome,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(1),
            },
        )
        .await;
    let _m2: MintNftOutput = conductor
        .call(
            &zome,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(2),
            },
        )
        .await;

    let third: Result<MintNftOutput, _> = conductor
        .call_fallible(
            &zome,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(3),
            },
        )
        .await;
    assert!(
        third.is_err(),
        "the 3rd mint must be rejected when max_supply = 2"
    );
}

/// Phase J.1: two independently whitelisted agents, each capped at their own
/// allowance, minting from two separate conductors. Confirms the per-agent
/// chain-walk generalizes correctly — Alice reaching her cap must not affect
/// Bob's, and vice versa.
#[tokio::test(flavor = "multi_thread")]
async fn whitelist_enforces_independent_per_agent_allowances() {
    let mut conductors = SweetConductorBatch::standard(2).await;
    // Agent keys must be known BEFORE the DNA is built (properties are baked
    // into the DNA hash), same requirement as the single-minter cap above.
    let alice_key = SweetAgents::one(conductors.get(0).unwrap().keystore()).await;
    let bob_key = SweetAgents::one(conductors.get(1).unwrap().keystore()).await;

    let dna_file = dna(Some(whitelist_props(
        vec![(alice_key.clone(), 2), (bob_key.clone(), 1)],
        0,
        300,
    )))
    .await;

    let apps = conductors
        .setup_app_for_zipped_agents("nft", &[alice_key.clone(), bob_key.clone()], &[dna_file])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    // Alice mints her 2 allowed NFTs.
    let _: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    let _: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(2) })
        .await;
    // Bob mints his 1 allowed NFT — unaffected by Alice's count.
    let _: MintNftOutput = cb
        .call(&zb, "mint_nft", MintNftInput { metadata_hash: metadata_hash(3) })
        .await;

    // Alice's 3rd mint is rejected (her allowance is 2).
    let alice_third: Result<MintNftOutput, _> = ca
        .call_fallible(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(4) })
        .await;
    assert!(alice_third.is_err(), "Alice must be rejected past her allowance of 2");

    // Bob's 2nd mint is rejected (his allowance is 1).
    let bob_second: Result<MintNftOutput, _> = cb
        .call_fallible(&zb, "mint_nft", MintNftInput { metadata_hash: metadata_hash(5) })
        .await;
    assert!(bob_second.is_err(), "Bob must be rejected past his allowance of 1");
}

/// Phase J.1: an agent not on the whitelist must be rejected outright, even
/// when the collection has remaining aggregate supply.
#[tokio::test(flavor = "multi_thread")]
async fn whitelist_rejects_a_non_whitelisted_agent() {
    let mut conductors = SweetConductorBatch::standard(2).await;
    let alice_key = SweetAgents::one(conductors.get(0).unwrap().keystore()).await;
    let eve_key = SweetAgents::one(conductors.get(1).unwrap().keystore()).await;

    let dna_file = dna(Some(whitelist_props(vec![(alice_key.clone(), 5)], 0, 300))).await;

    let apps = conductors
        .setup_app_for_zipped_agents("nft", &[alice_key.clone(), eve_key.clone()], &[dna_file])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    let (ze, ce) = (cells[1].zome(ZOME), conductors.get(1).unwrap());

    let eve_mint: Result<MintNftOutput, _> = ce
        .call_fallible(&ze, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    assert!(
        eve_mint.is_err(),
        "an agent absent from the whitelist must never be able to mint"
    );
}

/// Phase J.1: a whitelist whose allowances sum above `max_supply` is refused
/// at the `mint_mode()` level, which means it fails EVERY mint attempt in the
/// collection (fails closed) rather than silently permitting the aggregate
/// cap to be exceeded — see `CollectionProperties::mint_mode`'s doc comment.
#[tokio::test(flavor = "multi_thread")]
async fn whitelist_summing_above_max_supply_rejects_every_mint() {
    let mut conductor = SweetConductor::standard().await;
    let alice_key = SweetAgents::one(conductor.keystore()).await;

    // allowance (5) > max_supply (3): misconfigured at creation time.
    let dna_file = dna(Some(whitelist_props(vec![(alice_key.clone(), 5)], 3, 300))).await;
    let app = conductor
        .setup_app_for_agent("nft", alice_key.clone(), &[dna_file])
        .await
        .unwrap();
    let zome = app.cells()[0].zome(ZOME);

    let attempt: Result<MintNftOutput, _> = conductor
        .call_fallible(&zome, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    assert!(
        attempt.is_err(),
        "a whitelist misconfigured above max_supply must reject minting entirely, not just the overflow"
    );
}

/// Phase J.2: `mint_status()` reflects the same eligibility/allowance picture
/// `validate_create_nft_manifest` enforces, across all three policies, for
/// two independent agents on two conductors (cross-peer, not localStorage).
#[tokio::test(flavor = "multi_thread")]
async fn mint_status_reflects_whitelist_eligibility_and_progress() {
    let mut conductors = SweetConductorBatch::standard(2).await;
    let alice_key = SweetAgents::one(conductors.get(0).unwrap().keystore()).await;
    let eve_key = SweetAgents::one(conductors.get(1).unwrap().keystore()).await;

    let dna_file = dna(Some(whitelist_props(vec![(alice_key.clone(), 2)], 100, 300))).await;
    let apps = conductors
        .setup_app_for_zipped_agents("nft", &[alice_key.clone(), eve_key.clone()], &[dna_file])
        .await
        .unwrap();
    let cells = apps.cells_flattened();
    let (za, ca) = (cells[0].zome(ZOME), conductors.get(0).unwrap());
    let (ze, ce) = (cells[1].zome(ZOME), conductors.get(1).unwrap());

    // Alice, before minting: eligible, allowance 2, minted 0.
    let before: MintStatus = ca.call(&za, "mint_status", ()).await;
    assert_eq!(before.policy, MintPolicyView::Whitelist);
    assert!(before.eligible);
    assert_eq!(before.my_allowance, Some(2));
    assert_eq!(before.minted_by_me, 0);

    let _: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;

    // Alice, after 1 of 2: still eligible, minted_by_me updated.
    let mid: MintStatus = ca.call(&za, "mint_status", ()).await;
    assert!(mid.eligible);
    assert_eq!(mid.minted_by_me, 1);

    let _: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(2) })
        .await;

    // Alice, after 2 of 2: allowance reached, no longer eligible.
    let after: MintStatus = ca.call(&za, "mint_status", ()).await;
    assert!(!after.eligible);
    assert_eq!(after.minted_by_me, 2);
    assert_eq!(after.my_allowance, Some(2));

    // Eve: not on the whitelist at all — ineligible, no personal allowance.
    let eve_status: MintStatus = ce.call(&ze, "mint_status", ()).await;
    assert_eq!(eve_status.policy, MintPolicyView::Whitelist);
    assert!(!eve_status.eligible);
    assert_eq!(eve_status.my_allowance, None);
}

/// A -> B -> C. Exercises `sequence` incrementing and `prev_state` chaining past
/// the first hop.
#[tokio::test(flavor = "multi_thread")]
async fn two_hop_transfer_chains_provenance() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (
        cells[0].zome(ZOME),
        cells[1].zome(ZOME),
        cells[2].zome(ZOME),
    );
    let (ca, cb, cc) = (
        conductors.get(0).unwrap(),
        conductors.get(1).unwrap(),
        conductors.get(2).unwrap(),
    );
    let (ka, kb, kc) = (
        cells[0].agent_pubkey().clone(),
        cells[1].agent_pubkey().clone(),
        cells[2].agent_pubkey().clone(),
    );

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(1),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // Hop 1: A -> B
    let o1: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(
            &zb,
            "accept_transfer",
            CommitmentRef {
                commitment: o1.commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // Hop 2: B -> C
    let o2: InitiateTransferOutput = cb
        .call(
            &zb,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: kc.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cc
        .call(
            &zc,
            "accept_transfer",
            CommitmentRef {
                commitment: o2.commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let owner: AgentPubKey = cc.call(&zc, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, kc);

    let hops: Vec<nft::query::ProvenanceHop> =
        cc.call(&zc, "get_provenance", minted.nft.clone()).await;
    assert_eq!(hops.len(), 2);
    assert_eq!(
        (hops[0].sequence, &hops[0].from_agent, &hops[0].to_agent),
        (1, &ka, &kb)
    );
    assert_eq!(
        (hops[1].sequence, &hops[1].from_agent, &hops[1].to_agent),
        (2, &kb, &kc)
    );

    // B, no longer the owner, cannot re-offer.
    let bad: Result<InitiateTransferOutput, _> = cb
        .call_fallible(
            &zb,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: ka.clone(),
            },
        )
        .await;
    assert!(bad.is_err(), "a past owner must not be able to re-offer");
}

/// Reveal is restricted to the minter and may only happen once.
#[tokio::test(flavor = "multi_thread")]
async fn reveal_is_minter_only_and_single_shot() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let salt = [3u8; 32];
    let attributes = vec![NftAttribute {
        trait_type: "hue".into(),
        value: "amber".into(),
    }];
    let image_uri = "ipfs://x".to_string();
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri));

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let reveal_input = || RevealNftInput {
        nft: minted.nft.clone(),
        previous_action_hash: minted.nft.clone(),
        salt,
        attributes: attributes.clone(),
        image_uri: image_uri.clone(),
        force: false,
    };

    // Non-minter cannot reveal.
    let by_bob: Result<Record, _> = cb.call_fallible(&zb, "reveal_nft", reveal_input()).await;
    assert!(by_bob.is_err(), "only the minter may reveal");

    // Minter reveals once.
    let revealed: Record = ca.call(&za, "reveal_nft", reveal_input()).await;

    // Second reveal (from the first reveal's head) is rejected.
    let again: Result<Record, _> = ca
        .call_fallible(
            &za,
            "reveal_nft",
            RevealNftInput {
                nft: minted.nft.clone(),
                previous_action_hash: revealed.action_address().clone(),
                salt,
                attributes,
                image_uri,
                force: false,
            },
        )
        .await;
    assert!(again.is_err(), "an NFT may only be revealed once");
}

/// The transfer window makes accept (within) and cancel (after) mutually
/// exclusive, and a cancelled state can be re-offered.
#[tokio::test(flavor = "multi_thread")]
async fn transfer_window_bounds_accept_and_cancel() {
    const WINDOW: u64 = 6;
    let (conductors, cells) = setup(2, Some(dev_props(None, 0, WINDOW))).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(9),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let offer = |c: ActionHash| InitiateTransferInput {
        nft: c,
        to_agent: kb.clone(),
    };

    // Offer #1.
    let o1: InitiateTransferOutput = ca
        .call(&za, "initiate_transfer", offer(minted.nft.clone()))
        .await;

    // Cancel before the window closes: rejected.
    let early_cancel: Result<Record, _> = ca
        .call_fallible(
            &za,
            "cancel_transfer",
            CommitmentRef {
                commitment: o1.commitment.clone(),
            },
        )
        .await;
    assert!(
        early_cancel.is_err(),
        "cannot cancel before the window closes"
    );

    // Wait out the window, then cancel: accepted.
    tokio::time::sleep(std::time::Duration::from_secs(WINDOW + 3)).await;
    let _: Record = ca
        .call(
            &za,
            "cancel_transfer",
            CommitmentRef {
                commitment: o1.commitment.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // The state can now be re-offered and accepted within the fresh window.
    let o2: InitiateTransferOutput = ca
        .call(&za, "initiate_transfer", offer(minted.nft.clone()))
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(
            &zb,
            "accept_transfer",
            CommitmentRef {
                commitment: o2.commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, kb);
}

/// Accepting after the window has closed is rejected.
#[tokio::test(flavor = "multi_thread")]
async fn accept_after_window_is_rejected() {
    const WINDOW: u64 = 4;
    let (conductors, cells) = setup(2, Some(dev_props(None, 0, WINDOW))).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(11),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let o1: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(WINDOW + 3)).await;

    let late_accept: Result<Record, _> = cb
        .call_fallible(
            &zb,
            "accept_transfer",
            CommitmentRef {
                commitment: o1.commitment,
            },
        )
        .await;
    assert!(
        late_accept.is_err(),
        "acceptance after the window must be rejected"
    );
}
