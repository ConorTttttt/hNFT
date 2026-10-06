//! Phase J.3 — network-wide NFT-collection discovery.
//!
//! The most important property under test: Peer B discovers Peer A's
//! collection with NO shared localStorage, NO manually copied metadata, and
//! NO prior knowledge — purely by querying the `social` DHT both peers
//! already share (see `social_integrity::nft_collection_registry`'s doc for
//! why that DHT, not the `nft` clone's own, is where this has to live).

use holochain::prelude::*;
use holochain::sweettest::*;
use social::nft_collection_registry::*;
use social_integrity::NftCollectionRegistration;

mod common;
use common::*;

/// A distinct fake `DnaHash` per `seed` — this test suite never actually
/// creates an `nft` clone (out of scope for the `social` DNA's own tests);
/// `dna_hash` is opaque to `social`'s validation either way (see the
/// integrity module doc), so a fabricated hash exercises the same code path
/// a real one would.
fn fake_dna_hash(seed: u8) -> DnaHash {
    let mut bytes = [seed; 32];
    bytes[0] = seed.wrapping_add(1);
    DnaHash::from_raw_32(bytes.to_vec())
}

fn valid_input(seed: u8) -> RegisterNftCollectionInput {
    RegisterNftCollectionInput {
        dna_hash: fake_dna_hash(seed),
        network_seed: format!("seed-{seed}"),
        properties_json: format!(r#"{{"name":"Collection {seed}","symbol":"C{seed}"}}"#),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn peer_b_discovers_peer_a_collection_with_no_shared_state() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let ca = conductors.get(0).unwrap();
    let cb = conductors.get(1).unwrap();

    let input = valid_input(1);
    let created: Record = ca.call(&za, "register_nft_collection", &input).await;
    await_consistency(&cells).await.unwrap();

    // Peer B never called register, has no local knowledge of this
    // collection, and queries purely through the network discovery path.
    let discovered: Vec<NftCollectionDiscoveryView> =
        cb.call(&zb, "discover_nft_collections", ()).await;

    assert_eq!(discovered.len(), 1);
    let view = &discovered[0];
    assert_eq!(view.dna_hash, input.dna_hash);
    assert_eq!(view.network_seed, input.network_seed);
    assert_eq!(view.properties_json, input.properties_json);
    assert_eq!(view.registration, *created.action_address());
    assert_eq!(&view.registered_by, cells[0].agent_pubkey());
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_network_seed_is_rejected() {
    let (conductors, cells) = setup(1).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let mut input = valid_input(2);
    input.network_seed = "".into();
    let result: Result<Record, _> = ca.call_fallible(&za, "register_nft_collection", &input).await;
    assert!(result.is_err(), "an empty network_seed must be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_properties_json_is_rejected() {
    let (conductors, cells) = setup(1).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let mut input = valid_input(3);
    input.properties_json = "".into();
    let result: Result<Record, _> = ca.call_fallible(&za, "register_nft_collection", &input).await;
    assert!(result.is_err(), "empty properties_json must be rejected");
}

#[tokio::test(flavor = "multi_thread")]
async fn non_json_properties_is_rejected() {
    let (conductors, cells) = setup(1).await;
    let za = cells[0].zome(ZOME);
    let ca = conductors.get(0).unwrap();
    let mut input = valid_input(4);
    input.properties_json = "not json at all".into();
    let result: Result<Record, _> = ca.call_fallible(&za, "register_nft_collection", &input).await;
    assert!(result.is_err(), "non-JSON-looking properties_json must be rejected");
}

/// Two independent agents CAN both register the same `dna_hash` — nothing in
/// `validate()` can stop that (see the integrity module doc). The discovery
/// QUERY resolves this deterministically: the earliest registration wins,
/// and — critically — Eve's later registration does NOT let her hijack the
/// displayed `registered_by` for Alice's collection. That is the honest form
/// of "cannot forge" this architecture provides.
#[tokio::test(flavor = "multi_thread")]
async fn duplicate_registration_for_the_same_dna_hash_keeps_the_earliest() {
    let (conductors, cells) = setup(2).await;
    let (za, ze) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, ce) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let shared_dna_hash = fake_dna_hash(5);
    let alice_input = RegisterNftCollectionInput {
        dna_hash: shared_dna_hash.clone(),
        network_seed: "alice-seed".into(),
        properties_json: r#"{"name":"Alice's"}"#.into(),
    };
    let _alice_reg: Record = ca.call(&za, "register_nft_collection", &alice_input).await;
    await_consistency(&cells).await.unwrap();

    // Eve, a different agent, registers the SAME dna_hash with different
    // (bogus) addressing info, hoping to hijack the listing.
    let eve_input = RegisterNftCollectionInput {
        dna_hash: shared_dna_hash.clone(),
        network_seed: "eve-bogus-seed".into(),
        properties_json: r#"{"name":"Eve's hijack attempt"}"#.into(),
    };
    let eve_reg: Record = ce.call(&ze, "register_nft_collection", &eve_input).await;
    await_consistency(&cells).await.unwrap();
    // Eve's own entry is NOT rejected by validation — that's the honest limit.
    assert_eq!(eve_reg.action().author(), cells[1].agent_pubkey());

    let discovered: Vec<NftCollectionDiscoveryView> =
        ca.call(&za, "discover_nft_collections", ()).await;
    let matching: Vec<_> = discovered.iter().filter(|v| v.dna_hash == shared_dna_hash).collect();
    assert_eq!(matching.len(), 1, "exactly one listing per dna_hash, regardless of duplicate registrations");
    assert_eq!(&matching[0].registered_by, cells[0].agent_pubkey(), "the earliest registration wins the listing");
    assert_eq!(matching[0].network_seed, "alice-seed");
}

#[tokio::test(flavor = "multi_thread")]
async fn unregister_removes_it_from_discovery() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let input = valid_input(6);
    let created: Record = ca.call(&za, "register_nft_collection", &input).await;
    await_consistency(&cells).await.unwrap();

    let before: Vec<NftCollectionDiscoveryView> = cb.call(&zb, "discover_nft_collections", ()).await;
    assert_eq!(before.len(), 1);

    let _: () = ca
        .call(&za, "unregister_nft_collection", created.action_address().clone())
        .await;
    await_consistency(&cells).await.unwrap();

    let after: Vec<NftCollectionDiscoveryView> = cb.call(&zb, "discover_nft_collections", ()).await;
    assert!(after.is_empty(), "an unregistered collection must no longer be discoverable");
}

#[tokio::test(flavor = "multi_thread")]
async fn unregister_is_author_only() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let input = valid_input(7);
    let created: Record = ca.call(&za, "register_nft_collection", &input).await;
    await_consistency(&cells).await.unwrap();

    let bad: Result<(), _> = cb
        .call_fallible(&zb, "unregister_nft_collection", created.action_address().clone())
        .await;
    assert!(bad.is_err(), "only the registrant may unregister their own announcement");
}

#[tokio::test(flavor = "multi_thread")]
async fn my_registrations_only_lists_this_agents_own() {
    let (conductors, cells) = setup(2).await;
    let (za, zb) = (cells[0].zome(ZOME), cells[1].zome(ZOME));
    let (ca, cb) = (conductors.get(0).unwrap(), conductors.get(1).unwrap());

    let _: Record = ca.call(&za, "register_nft_collection", &valid_input(8)).await;
    let _: Record = cb.call(&zb, "register_nft_collection", &valid_input(9)).await;
    await_consistency(&cells).await.unwrap();

    let alice_mine: Vec<Record> = ca.call(&za, "my_nft_collection_registrations", ()).await;
    assert_eq!(alice_mine.len(), 1);
    let entry: NftCollectionRegistration = alice_mine[0].entry().to_app_option().unwrap().unwrap();
    assert_eq!(entry.dna_hash, fake_dna_hash(8));
}
