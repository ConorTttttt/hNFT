use holochain::prelude::*;
use holochain::sweettest::*;
use nft::mint::{MintNftInput, MintNftOutput};
use nft::reveal::RevealNftInput;
use nft::query::GetNftsForAgentInput;
use nft::transfer::{CommitmentRef, InitiateTransferInput, InitiateTransferOutput};
use nft_integrity::*;

mod common;
use common::*;

/// Only the agent named in the commitment may accept it.
#[tokio::test(flavor = "multi_thread")]
async fn wrong_agent_cannot_accept() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (cells[0].zome(ZOME), cells[1].zome(ZOME), cells[2].zome(ZOME));
    let (ca, cb, cc) = (
        conductors.get(0).unwrap(),
        conductors.get(1).unwrap(),
        conductors.get(2).unwrap(),
    );
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(1) })
        .await;
    await_consistency(&cells).await.unwrap();

    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    // Carol is not the named buyer.
    let by_carol: Result<Record, _> = cc
        .call_fallible(&zc, "accept_transfer", CommitmentRef { commitment: offer.commitment.clone() })
        .await;
    assert!(by_carol.is_err(), "a non-recipient must not be able to accept");

    // Bob (the named buyer) still can.
    let by_bob: Result<Record, _> = cb
        .call_fallible(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    assert!(by_bob.is_ok(), "the named buyer should be able to accept");
}

/// A commitment may be accepted at most once, even by the right buyer.
#[tokio::test(flavor = "multi_thread")]
async fn double_accept_is_rejected() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(2) })
        .await;
    await_consistency(&cells).await.unwrap();

    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment.clone() })
        .await;
    await_consistency(&cells).await.unwrap();

    let again: Result<Record, _> = cb
        .call_fallible(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    assert!(again.is_err(), "a commitment must not be accepted twice");
}

/// `drop_stale_ownership_link` removes only the seller's own derived index link;
/// the provenance chain (authoritative) and the buyer's view are unaffected.
#[tokio::test(flavor = "multi_thread")]
async fn drop_stale_ownership_link_removes_only_the_sellers_index_entry() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(3) })
        .await;
    await_consistency(&cells).await.unwrap();

    let mine_before: Vec<Record> = ca
        .call(&za, "get_nfts_for_agent", GetNftsForAgentInput { agent: ka.clone() })
        .await;
    assert_eq!(mine_before.len(), 1);

    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    // Seller's index link is stale but still present; owner is already Bob.
    let seller_stale: Vec<Record> = ca
        .call(&za, "get_nfts_for_agent", GetNftsForAgentInput { agent: ka.clone() })
        .await;
    assert_eq!(seller_stale.len(), 1, "stale seller link present until dropped");
    let buyer_view: Vec<Record> = cb
        .call(&zb, "get_nfts_for_agent", GetNftsForAgentInput { agent: kb.clone() })
        .await;
    assert_eq!(buyer_view.len(), 1);
    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, kb);

    // Seller drops the stale link.
    let _: () = ca.call(&za, "drop_stale_ownership_link", minted.nft.clone()).await;
    await_consistency(&cells).await.unwrap();

    let seller_after: Vec<Record> = ca
        .call(&za, "get_nfts_for_agent", GetNftsForAgentInput { agent: ka })
        .await;
    assert_eq!(seller_after.len(), 0, "seller's index entry gone");
    let buyer_after: Vec<Record> = cb
        .call(&zb, "get_nfts_for_agent", GetNftsForAgentInput { agent: kb.clone() })
        .await;
    assert_eq!(buyer_after.len(), 1, "buyer unaffected");
    let owner_after: AgentPubKey = cb.call(&zb, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner_after, kb, "provenance unaffected");
}

/// Anyone can read an NFT's provenance and current owner from the DHT, not just
/// the parties to the transfer.
#[tokio::test(flavor = "multi_thread")]
async fn provenance_visible_to_an_uninvolved_third_party() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (cells[0].zome(ZOME), cells[1].zome(ZOME), cells[2].zome(ZOME));
    let (ca, cb, cc) = (
        conductors.get(0).unwrap(),
        conductors.get(1).unwrap(),
        conductors.get(2).unwrap(),
    );
    let (ka, kb) = (cells[0].agent_pubkey().clone(), cells[1].agent_pubkey().clone());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(4) })
        .await;
    await_consistency(&cells).await.unwrap();
    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb.clone() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    // Carol was never involved.
    let hops: Vec<nft::query::ProvenanceHop> =
        cc.call(&zc, "get_provenance", minted.nft.clone()).await;
    assert_eq!(hops.len(), 1);
    assert_eq!((hops[0].sequence, &hops[0].from_agent, &hops[0].to_agent), (1, &ka, &kb));

    let owner: AgentPubKey = cc.call(&zc, "current_owner", minted.nft.clone()).await;
    assert_eq!(owner, kb);

    let bobs: Vec<Record> = cc
        .call(&zc, "get_nfts_for_agent", GetNftsForAgentInput { agent: kb })
        .await;
    assert_eq!(bobs.len(), 1);
}

/// Only the offerer may cancel a commitment — even after the window, when the
/// time-gate would otherwise allow it.
#[tokio::test(flavor = "multi_thread")]
async fn cancel_by_non_offerer_is_rejected() {
    const WINDOW: u64 = 4;
    let (conductors, cells) = setup(2, Some(dev_props(None, 0, WINDOW))).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(5) })
        .await;
    await_consistency(&cells).await.unwrap();
    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(WINDOW + 3)).await;

    let by_bob: Result<Record, _> = cb
        .call_fallible(&zb, "cancel_transfer", CommitmentRef { commitment: offer.commitment.clone() })
        .await;
    assert!(by_bob.is_err(), "only the offerer may cancel");

    let by_alice: Result<Record, _> = ca
        .call_fallible(&za, "cancel_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    assert!(by_alice.is_ok(), "the offerer may cancel after the window");
}

/// A reveal cannot be repeated by branching a second Update off the sealed mint.
#[tokio::test(flavor = "multi_thread")]
async fn reveal_cannot_be_repeated_by_branching_off_the_mint() {
    let (conductors, cells) = setup(1, None).await;
    let z = cells[0].zome(ZOME);
    let c = conductors.get(0).unwrap();

    let salt = [5u8; 32];
    let attributes = vec![NftAttribute { trait_type: "k".into(), value: "v".into() }];
    let image_uri = "ipfs://z".to_string();
    let commitment = commit_hash(&reveal_preimage(&salt, &attributes, &image_uri));

    let minted: MintNftOutput = c
        .call(&z, "mint_nft", MintNftInput { metadata_hash: commitment })
        .await;

    let reveal = || RevealNftInput {
        nft: minted.nft.clone(),
        previous_action_hash: minted.nft.clone(), // branch off the mint each time
        salt,
        attributes: attributes.clone(),
        image_uri: image_uri.clone(),
        force: false,
    };

    let first: Result<Record, _> = c.call_fallible(&z, "reveal_nft", reveal()).await;
    assert!(first.is_ok(), "first reveal should succeed");

    let second: Result<Record, _> = c.call_fallible(&z, "reveal_nft", reveal()).await;
    assert!(second.is_err(), "a second reveal off the mint must be rejected");
}

/// `get_all_nfts` lists every mint in the collection, regardless of who minted
/// or currently owns it, and is visible to an uninvolved third party.
#[tokio::test(flavor = "multi_thread")]
async fn get_all_nfts_lists_every_mint_regardless_of_owner() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (cells[0].zome(ZOME), cells[1].zome(ZOME), cells[2].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let a: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(20) })
        .await;
    let b: MintNftOutput = cb
        .call(&zb, "mint_nft", MintNftInput { metadata_hash: metadata_hash(21) })
        .await;
    await_consistency(&cells).await.unwrap();

    // A third, uninvolved agent (didn't mint or own either NFT) sees both.
    let all: Vec<Record> = conductors
        .get(2)
        .unwrap()
        .call(&zc, "get_all_nfts", ())
        .await;
    let hashes: Vec<ActionHash> = all.iter().map(|r| r.action_address().clone()).collect();
    assert!(hashes.contains(&a.nft), "third party sees agent A's mint");
    assert!(hashes.contains(&b.nft), "third party sees agent B's mint");
}
