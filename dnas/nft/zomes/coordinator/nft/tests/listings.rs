//! Listings: a public asking price + "express interest" discovery layer in
//! front of the existing, unmodified transfer flow. `initiate_transfer` /
//! `accept_transfer` (covered by `flows.rs` / `edge.rs`) still do the actual
//! ownership move — these tests only cover the listing/interest layer itself,
//! plus one end-to-end check that the existing transfer flow still completes
//! a sale sourced from a listing.

use holochain::prelude::*;
use holochain::sweettest::*;
use nft::listing::*;
use nft::mint::{MintNftInput, MintNftOutput};
use nft::transfer::{CommitmentRef, InitiateTransferInput, InitiateTransferOutput};

mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn create_update_cancel_listing_author_only() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(50) })
        .await;
    await_consistency(&cells).await.unwrap();

    let listed: Record = ca
        .call(
            &za,
            "create_listing",
            CreateListingInput { nft: minted.nft.clone(), price: 100, currency: "HOT".into() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let listing_hash = listed.action_address().clone();

    // Bob cannot edit Alice's listing.
    let bad_update: Result<Record, _> = cb
        .call_fallible(
            &zb,
            "update_listing_price",
            UpdateListingInput {
                previous_action_hash: listing_hash.clone(),
                price: 1,
                currency: "HOT".into(),
            },
        )
        .await;
    assert!(bad_update.is_err(), "only the lister may edit their listing");

    // Alice can.
    let updated: Record = ca
        .call(
            &za,
            "update_listing_price",
            UpdateListingInput {
                previous_action_hash: listing_hash.clone(),
                price: 150,
                currency: "HOT".into(),
            },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _ = updated;

    let view: Option<ListingView> = cb.call(&zb, "listing_for_nft", minted.nft.clone()).await;
    assert_eq!(view.expect("resolves").price, 150);

    // Bob cannot cancel it either.
    let bad_cancel: Result<(), _> = cb.call_fallible(&zb, "cancel_listing", listing_hash.clone()).await;
    assert!(bad_cancel.is_err(), "only the lister may cancel their listing");

    let _: () = ca.call(&za, "cancel_listing", listing_hash).await;
    await_consistency(&cells).await.unwrap();
    let after: Option<ListingView> = cb.call(&zb, "listing_for_nft", minted.nft).await;
    assert!(after.is_none(), "a cancelled listing is no longer active");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_current_owner_can_list() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let ca = conductors.get(0).unwrap();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(51) })
        .await;
    await_consistency(&cells).await.unwrap();

    let bad: Result<Record, _> = conductors
        .get(1)
        .unwrap()
        .call_fallible(
            &zb,
            "create_listing",
            CreateListingInput { nft: minted.nft, price: 10, currency: "HOT".into() },
        )
        .await;
    assert!(bad.is_err(), "a non-owner may not list an NFT");
}

#[tokio::test(flavor = "multi_thread")]
async fn listing_for_nft_reflects_active_state() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(52) })
        .await;
    await_consistency(&cells).await.unwrap();

    let _: Record = ca
        .call(
            &za,
            "create_listing",
            CreateListingInput { nft: minted.nft.clone(), price: 5, currency: "HOT".into() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let active: Option<ListingView> = cb.call(&zb, "listing_for_nft", minted.nft.clone()).await;
    assert!(active.is_some());

    // Alice sells to Bob directly (the existing, untouched transfer flow) —
    // the listing entry itself still exists but is now stale.
    let offer: InitiateTransferOutput = ca
        .call(
            &za,
            "initiate_transfer",
            InitiateTransferInput { nft: minted.nft.clone(), to_agent: kb },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let _: Record = cb
        .call(&zb, "accept_transfer", CommitmentRef { commitment: offer.commitment })
        .await;
    await_consistency(&cells).await.unwrap();

    let stale: Option<ListingView> = cb.call(&zb, "listing_for_nft", minted.nft).await;
    assert!(stale.is_none(), "a listing must go stale once sold another way");
}

#[tokio::test(flavor = "multi_thread")]
async fn express_interest_notifies_seller_and_lists_buyer() {
    let (conductors, cells) = setup(2, None).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());
    let kb = cells[1].agent_pubkey().clone();

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(53) })
        .await;
    await_consistency(&cells).await.unwrap();
    let listed: Record = ca
        .call(
            &za,
            "create_listing",
            CreateListingInput { nft: minted.nft.clone(), price: 20, currency: "HOT".into() },
        )
        .await;
    await_consistency(&cells).await.unwrap();
    let listing_hash = listed.action_address().clone();

    let _: () = cb.call(&zb, "express_interest", listing_hash.clone()).await;
    await_consistency(&cells).await.unwrap();

    let buyers: Vec<AgentPubKey> = ca.call(&za, "interested_buyers", listing_hash).await;
    assert_eq!(buyers, vec![kb.clone()]);

    // The seller completes the sale through the existing, unmodified flow.
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
    let owner: AgentPubKey = cb.call(&zb, "current_owner", minted.nft).await;
    assert_eq!(owner, kb);
}

#[tokio::test(flavor = "multi_thread")]
async fn all_listings_visible_to_third_party() {
    let (conductors, cells) = setup(3, None).await;
    let (za, zb, zc) = (cells[0].zome(ZOME), cells[1].zome(ZOME), cells[2].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let a: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: metadata_hash(54) })
        .await;
    let b: MintNftOutput = cb
        .call(&zb, "mint_nft", MintNftInput { metadata_hash: metadata_hash(55) })
        .await;
    await_consistency(&cells).await.unwrap();

    let _: Record = ca
        .call(
            &za,
            "create_listing",
            CreateListingInput { nft: a.nft.clone(), price: 1, currency: "HOT".into() },
        )
        .await;
    let _: Record = cb
        .call(
            &zb,
            "create_listing",
            CreateListingInput { nft: b.nft.clone(), price: 2, currency: "HOT".into() },
        )
        .await;
    await_consistency(&cells).await.unwrap();

    let all: Vec<ListingView> = conductors.get(2).unwrap().call(&zc, "all_listings", ()).await;
    let nfts: Vec<ActionHash> = all.iter().map(|v| v.nft.clone()).collect();
    assert!(nfts.contains(&a.nft));
    assert!(nfts.contains(&b.nft));
}
