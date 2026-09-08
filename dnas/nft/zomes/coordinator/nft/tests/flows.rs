use holochain::prelude::*;
use holochain::sweettest::*;
use nft::mint::{MintNftInput, MintNftOutput};
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
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri).unwrap());

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
        .call(&bob, "mint_nft", MintNftInput { metadata_hash: commitment })
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
        .call(&zome, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    let _m2: MintNftOutput = conductor
        .call(&zome, "mint_nft", MintNftInput { metadata_hash: metadata_hash(2) })
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

/// A -> B -> C. Exercises `sequence` incrementing and `prev_state` chaining past
/// the first hop.
#[tokio::test(flavor = "multi_thread")]
async fn two_hop_transfer_chains_provenance() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (cells[0].zome(ZOME), cells[1].zome(ZOME), cells[2].zome(ZOME));
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
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    await_consistency(&cells).await.unwrap();

    // Hop 1: A -> B
    let o1: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: o1.commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    // Hop 2: B -> C
    let o2: InitiateTransferOutput = cb
        .call(
            &zb,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kc.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cc
        .call(&zc, "accept_transfer", CommitmentRef { commitment: o2.commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    let owner: AgentPubKey = cc.call(&zc, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, kc);

    let hops: Vec<nft::query::ProvenanceHop> =
        cc.call(&zc, "get_provenance", minted.nft.clone()).await;
    assert_eq!(hops.len(), 2);
    assert_eq!((hops[0].sequence, &hops[0].from_agent, &hops[0].to_agent), (1, &ka, &kb));
    assert_eq!((hops[1].sequence, &hops[1].from_agent, &hops[1].to_agent), (2, &kb, &kc));

    // B, no longer the owner, cannot re-offer.
    let bad: Result<InitiateTransferOutput, _> = cb
        .call_fallible(
            &zb,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: ka.clone() },
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
    let attributes = vec![NftAttribute { trait_type: "hue".into(), value: "amber".into() }];
    let image_uri = "ipfs://x".to_string();
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri).unwrap());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    let reveal_input = || RevealNftInput {
        nft: minted.nft.clone(),
        previous_action_hash: minted.nft.clone(),
        salt,
        attributes: attributes.clone(),
        image_uri: image_uri.clone(),
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
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(9) })
        .await;
    await_consistency(&cells).await.unwrap();

    let offer = |c: ActionHash| InitiateTransferInput { nft: c, to_agent: kb.clone() };

    // Offer #1.
    let o1: InitiateTransferOutput =
        ca.call(&za, "initiate_transfer", offer(minted.nft.clone())).await;

    // Cancel before the window closes: rejected.
    let early_cancel: Result<Record, _> = ca
        .call_fallible(&za, "cancel_transfer", CommitmentRef { commitment: o1.commitment.clone() })
        .await;
    assert!(early_cancel.is_err(), "cannot cancel before the window closes");

    // Wait out the window, then cancel: accepted.
    tokio::time::sleep(std::time::Duration::from_secs(WINDOW + 3)).await;
    let _: Record = ca
        .call(&za, "cancel_transfer", CommitmentRef { commitment: o1.commitment.clone() })
        .await;
    await_consistency(&cells).await.unwrap();

    // The state can now be re-offered and accepted within the fresh window.
    let o2: InitiateTransferOutput =
        ca.call(&za, "initiate_transfer", offer(minted.nft.clone())).await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: o2.commitment })
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
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(11) })
        .await;
    await_consistency(&cells).await.unwrap();

    let o1: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(WINDOW + 3)).await;

    let late_accept: Result<Record, _> = cb
        .call_fallible(&zb, "accept_transfer", CommitmentRef { commitment: o1.commitment })
        .await;
    assert!(late_accept.is_err(), "acceptance after the window must be rejected");
}
