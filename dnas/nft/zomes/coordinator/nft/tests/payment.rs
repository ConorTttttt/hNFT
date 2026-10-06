use holochain::prelude::*;
use holochain::sweettest::*;
use nft::mint::{MintNftInput, MintNftOutput};
use nft::payment::SettleFromRaveInput;
use nft::transfer::{CommitmentRef, InitiateTransferOutput};

mod common;
use common::*;

/// A fake RAVE action hash used by the current demo verifier.
fn fake_rave(seed: u8) -> ActionHash {
    let mut bytes = [seed; 32];
    bytes[0] = seed.wrapping_add(1);
    ActionHash::from_raw_32(bytes.to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn rave_settlement_completes_the_transfer() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(60),
            },
        )
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
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let _: Record = cb
        .call(
            &zb,
            "accept_transfer",
            CommitmentRef {
                commitment: out.commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft).await;

    assert_eq!(
        owner, kb,
        "ownership moved through the same provenance chain as a free offer"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rave_hash_is_accepted_as_the_settlement_identifier() {
    let (conductors, cells) = setup(2, None).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(61),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let rave = fake_rave(2);

    let result: Result<InitiateTransferOutput, _> = ca
        .call_fallible(
            &za,
            "initiate_transfer_from_rave",
            SettleFromRaveInput {
                nft: minted.nft,
                to_agent: kb,
                rave,
            },
        )
        .await;

    assert!(
        result.is_ok(),
        "the demo verifier should accept the supplied RAVE identifier"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn free_initiate_transfer_still_works_unconditionally() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(
            &za,
            "mint_nft",
            MintNftInput {
                metadata_hash: metadata_hash(65),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let out: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            nft::transfer::InitiateTransferInput {
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
                commitment: out.commitment,
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft).await;

    assert_eq!(
        owner, kb,
        "the pre-existing free offer path is unaffected by payment.rs"
    );
}
