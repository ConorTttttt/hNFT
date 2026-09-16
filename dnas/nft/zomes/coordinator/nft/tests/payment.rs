//! `initiate_transfer_from_rave` — the demo-mode Unyt settlement bridge in
//! `payment.rs`. Per that module's own doc, this zome does NOT re-verify the
//! claimed RAVE against the Unyt network; it only checks internal
//! consistency of what the caller (UI, here: the test) hands it, and gates
//! that check on the same authoritative ownership check `initiate_transfer`
//! already uses. These tests cover exactly that surface.

use holochain::prelude::*;
use holochain::sweettest::*;
use nft::mint::{MintNftInput, MintNftOutput};
use nft::payment::SettleFromRaveInput;
use nft::transfer::{CommitmentRef, InitiateTransferOutput};

mod common;
use common::*;

/// A fake RAVE action hash — never fetched by `payment.rs` in this demo
/// version (see its module doc), so any distinct hash is a valid fixture.
fn fake_rave(seed: u8) -> ActionHash {
    let mut bytes = [seed; 32];
    bytes[0] = seed.wrapping_add(1);
    ActionHash::from_raw_32(bytes.to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn matching_settlement_completes_the_transfer() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(60) })
        .await;
    await_consistency(&cells).await.unwrap();

    let out: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
                rave: fake_rave(1),
                fulfilled: true,
                nft_id: minted.nft.to_string(),
                new_owner: kb.to_string(),
                previous_owner: ka.to_string(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // The resulting commitment is a normal TransferCommitment — the buyer
    // accepts it through the existing, untouched accept_transfer path.
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: out.commitment })
        .await;
    await_consistency(&cells).await.unwrap();
    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft).await;
    assert_eq!(owner, kb, "ownership moved through the same provenance chain as a free offer");
}

#[tokio::test(flavor = "multi_thread")]
async fn unfulfilled_settlement_is_rejected() {
    let (conductors, cells) = setup(2, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(61) })
        .await;
    await_consistency(&cells).await.unwrap();

    let result: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
                rave: fake_rave(2),
                fulfilled: false,
                nft_id: minted.nft.to_string(),
                new_owner: kb.to_string(),
                previous_owner: ka.to_string(),
            },
        )
        .await;
    assert!(result.is_err(), "an unfulfilled settlement must not authorize a transfer");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_nft_id_is_rejected() {
    let (conductors, cells) = setup(2, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(62) })
        .await;
    await_consistency(&cells).await.unwrap();

    let result: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
                rave: fake_rave(3),
                fulfilled: true,
                nft_id: "not-this-nft".into(),
                new_owner: kb.to_string(),
                previous_owner: ka.to_string(),
            },
        )
        .await;
    assert!(result.is_err(), "a settlement for a different nft_id must not authorize this transfer");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_new_owner_is_rejected() {
    let (conductors, cells) = setup(3, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let (ka, kb, kc) = (
        cells[0].agent_pubkey().clone(),
        cells[1].agent_pubkey().clone(),
        cells[2].agent_pubkey().clone(),
    );

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(63) })
        .await;
    await_consistency(&cells).await.unwrap();

    // The RAVE claims Carol as the new owner, but the offer names Bob.
    let result: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft.clone(),
                to_agent: kb,
                rave: fake_rave(4),
                fulfilled: true,
                nft_id: minted.nft.to_string(),
                new_owner: kc.to_string(),
                previous_owner: ka.to_string(),
            },
        )
        .await;
    assert!(result.is_err(), "a settlement naming a different buyer must not authorize this offer");
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_previous_owner_is_rejected() {
    let (conductors, cells) = setup(2, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(64) })
        .await;
    await_consistency(&cells).await.unwrap();

    let result: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft.clone(),
                to_agent: kb.clone(),
                rave: fake_rave(5),
                fulfilled: true,
                nft_id: minted.nft.to_string(),
                new_owner: kb.to_string(),
                // Some other payout key, not the seller's.
                previous_owner: "uhCAk_someone_else".into(),
            },
        )
        .await;
    assert!(
        result.is_err(),
        "a settlement whose previous_owner doesn't match the seller's payout key must be rejected"
    );
}

/// The existing free-offer path is untouched by this addition.
#[tokio::test(flavor = "multi_thread")]
async fn free_initiate_transfer_still_works_unconditionally() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(65) })
        .await;
    await_consistency(&cells).await.unwrap();

    let out: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            nft::transfer::InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: out.commitment })
        .await;
    await_consistency(&cells).await.unwrap();
    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft).await;
    assert_eq!(owner, kb, "the pre-existing free offer path is unaffected by payment.rs");
}
