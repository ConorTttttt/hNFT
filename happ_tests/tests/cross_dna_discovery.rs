//! Phase J.4 — network-wide NFT discovery, proven end to end across BOTH
//! DNAs together (`nft` + `social`), exactly as `workdir/happ.yaml` installs
//! them for a real agent. Every other sweettest in this workspace installs a
//! single DNA in isolation; this crate exists specifically to prove the
//! cross-DNA claim the J.3/J.4 architecture depends on: that `social` (one
//! shared DHT) is what lets a peer find and then join/query an `nft`
//! collection DHT it was never part of.
//!
//! There is deliberately NO new Rust protocol under test here beyond J.3's
//! `nft_collection_registry` (already covered by its own unit tests) — J.4's
//! actual conclusion (see the J.4 report) is that NFT discovery needs no new
//! DHT entry type at all: a collection's own, unmodified `get_all_nfts` IS
//! the NFT-discovery mechanism, once J.3 gets a peer into that collection.
//! This test proves exactly that composition, with no shared localStorage
//! and no manually copied metadata between the two peers.
//!
//! This crate depends on NEITHER `nft`/`nft_integrity` NOR
//! `social`/`social_integrity` as Rust crates — only `holochain`'s sweettest
//! harness, which loads both DNAs from their packed `.dna` bundles exactly
//! as the real conductor does. Linking both integrity crates' generated
//! `#[no_mangle]` unit-enum symbols (`__num_entry_types`/`__num_link_types`)
//! into one native test binary is a hard conflict — each is only ever meant
//! to be the sole integrity zome in its own compiled wasm module. The fix,
//! and the one this codebase's own pitfalls checklist already prescribes for
//! cross-DNA calls, is local mirror structs (below) instead of importing the
//! real crates.

use holochain::prelude::*;
use holochain::sweettest::*;
use holochain_serialized_bytes::SerializedBytes;
use serde::{Deserialize, Serialize};
use std::path::Path;

const NFT_DNA_PATH: &str = "../dnas/nft/workdir/nft.dna";
const SOCIAL_DNA_PATH: &str = "../dnas/social/workdir/social.dna";
const NFT_ZOME: &str = "nft";
const SOCIAL_ZOME: &str = "social";

// --- local mirrors of the wire shapes this test needs (see the module doc) --

#[derive(Serialize, Deserialize, SerializedBytes, Debug, Clone)]
struct CollectionPropertiesMirror {
    name: String,
    symbol: String,
    max_supply: u32,
    minter_pubkey: Option<String>,
    royalty_bps: u16,
    transfer_window_secs: u64,
    #[serde(default)]
    blind_resolution_window_secs: u64,
    #[serde(default)]
    collection_id: Option<String>,
    #[serde(default)]
    randomness: Option<serde_json::Value>,
    #[serde(default)]
    generation: Option<serde_json::Value>,
    #[serde(default)]
    whitelist: Option<serde_json::Value>,
}

fn collection_properties(name: &str) -> CollectionPropertiesMirror {
    CollectionPropertiesMirror {
        name: name.into(),
        symbol: "XDNA".into(),
        max_supply: 0,
        minter_pubkey: None, // open minting — simplest fixture for this test
        royalty_bps: 0,
        transfer_window_secs: 300,
        blind_resolution_window_secs: 0,
        collection_id: None,
        randomness: None,
        generation: None,
        whitelist: None,
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct MintNftInput {
    metadata_hash: [u8; 32],
}

#[derive(Deserialize, Debug)]
struct MintNftOutput {
    nft: ActionHash,
}

#[derive(Serialize, Deserialize, Debug)]
struct RegisterNftCollectionInput {
    dna_hash: DnaHash,
    network_seed: String,
    properties_json: String,
}

#[derive(Deserialize, Debug, Clone)]
struct NftCollectionDiscoveryView {
    dna_hash: DnaHash,
    network_seed: String,
    properties_json: String,
}

/// Builds the `nft` DnaFile for a specific "collection" — a specific
/// `network_seed` + `properties` pair, exactly what a real client's
/// `createCloneCell` (Phase J.3's `openDiscoveredCollection`) does with the
/// SAME ingredients to reproduce an IDENTICAL clone. Two calls with the same
/// `seed`/`name` always yield the same `DnaHash` — that equality IS the
/// thing under test.
async fn nft_dna_for_collection(seed: &str, name: &str) -> DnaFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(NFT_DNA_PATH);
    let sb = SerializedBytes::try_from(collection_properties(name)).unwrap();
    SweetDnaFile::from_bundle_with_overrides(
        &path,
        DnaModifiersOpt::default()
            .with_network_seed(seed.to_string())
            .with_properties(sb),
    )
    .await
    .unwrap()
}

async fn social_dna() -> DnaFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SOCIAL_DNA_PATH);
    SweetDnaFile::from_bundle(&path).await.unwrap()
}

/// `SweetConductor::exchange_peer_info` (the library helper) asserts that
/// EVERY space any conductor is in has at least `conductor_count` agent
/// infos — true for a homogeneous batch (every conductor installs the same
/// app), but never true here: Alice's and Bob's OWN `nft` collections are
/// two different, unrelated spaces, each with only ONE agent in it forever.
/// This reimplements just the injection this test actually needs — sharing
/// agent info for exactly ONE named space (whichever `dna_hash` the caller
/// asks about, e.g. `social`, or later the specific `nft` collection Bob
/// joins) — without the library helper's homogeneous-batch assumption.
async fn exchange_peer_info_for_space(conductors: &[SweetConductor], dna_hash: &DnaHash) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let mut all = std::collections::HashSet::new();
            for c in conductors {
                let infos = c
                    .holochain_p2p()
                    .peer_store(dna_hash.clone())
                    .await
                    .unwrap()
                    .get_all()
                    .await
                    .unwrap();
                all.extend(infos);
            }
            for c in conductors {
                c.holochain_p2p()
                    .peer_store(dna_hash.clone())
                    .await
                    .unwrap()
                    .insert(all.iter().cloned().collect())
                    .await
                    .unwrap();
            }
            if all.len() >= conductors.len() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("timeout while exchanging social-DHT peer info");
}

/// The full chain: Alice mints in her own collection, publishes discovery
/// for it (J.3), and — with NO shared localStorage, NO manually copied NFT
/// id, and NO prior relationship — Bob discovers the collection over the
/// network, independently reconstructs its exact clone from the published
/// ingredients, joins it, and resolves the real mint record from it.
#[tokio::test(flavor = "multi_thread")]
async fn peer_b_discovers_a_collection_and_resolves_its_nft_with_no_shared_state() {
    let mut conductors: Vec<SweetConductor> = SweetConductorBatch::standard(2).await.into_inner();

    let social = social_dna().await;

    // --- Alice: her own collection (`nft`) + the shared discovery DHT (`social`) ---
    let alice_nft_dna = nft_dna_for_collection("alices-collection-seed", "Alice's Collection").await;
    let alice_app = conductors[0]
        .setup_app(
            "p2p_art_marketplace",
            &[
                ("nft".to_string(), alice_nft_dna.clone()),
                ("social".to_string(), social.clone()),
            ],
        )
        .await
        .unwrap();
    let alice_nft_cell = alice_app.cells()[0].clone();
    let alice_social_cell = alice_app.cells()[1].clone();
    let (za, zsa) = (alice_nft_cell.zome(NFT_ZOME), alice_social_cell.zome(SOCIAL_ZOME));
    let ca = &conductors[0];

    let minted: MintNftOutput = ca
        .call(&za, "mint_nft", MintNftInput { metadata_hash: [7u8; 32] })
        .await;

    // Publish discovery for it — exactly the ingredients a real client reads
    // live from its own `appInfo()` (dna_hash, network_seed, properties),
    // never anything remembered separately.
    let alice_dna_hash = alice_nft_cell.dna_hash().clone();
    let properties_json = serde_json::to_string(&collection_properties("Alice's Collection")).unwrap();
    let _reg: Record = ca
        .call(
            &zsa,
            "register_nft_collection",
            RegisterNftCollectionInput {
                dna_hash: alice_dna_hash.clone(),
                network_seed: "alices-collection-seed".into(),
                properties_json,
            },
        )
        .await;

    // --- Bob: an UNRELATED collection of his own (never heard of Alice's) +
    //     the same shared `social` DHT. ---
    let bob_own_nft_dna = nft_dna_for_collection("bobs-unrelated-seed", "Bob's Own Collection").await;
    let bob_app = conductors[1]
        .setup_app(
            "p2p_art_marketplace",
            &[
                ("nft".to_string(), bob_own_nft_dna),
                ("social".to_string(), social.clone()),
            ],
        )
        .await
        .unwrap();
    let bob_social_cell = bob_app.cells()[1].clone();
    let zsb = bob_social_cell.zome(SOCIAL_ZOME);

    exchange_peer_info_for_space(&conductors, alice_social_cell.dna_hash()).await;
    await_consistency(&[alice_social_cell.clone(), bob_social_cell.clone()])
        .await
        .unwrap();

    // Bob discovers Alice's collection purely over the network (J.3).
    let discovered: Vec<NftCollectionDiscoveryView> =
        conductors[1].call(&zsb, "discover_nft_collections", ()).await;
    let found = discovered
        .iter()
        .find(|d| d.dna_hash == alice_dna_hash)
        .expect("Alice's collection must be discoverable by Bob");
    assert_eq!(found.network_seed, "alices-collection-seed");

    // Bob independently reconstructs the IDENTICAL clone from the published
    // ingredients only — exactly what `openDiscoveredCollection` does
    // client-side via `createCloneCell`. No NFT id was ever transmitted.
    let reconstructed_props: CollectionPropertiesMirror =
        serde_json::from_str(&found.properties_json).unwrap();
    let sb = SerializedBytes::try_from(reconstructed_props).unwrap();
    let reconstructed_dna = SweetDnaFile::from_bundle_with_overrides(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(NFT_DNA_PATH),
        DnaModifiersOpt::default()
            .with_network_seed(found.network_seed.clone())
            .with_properties(sb),
    )
    .await
    .unwrap();
    assert_eq!(
        reconstructed_dna.dna_hash(),
        &alice_dna_hash,
        "the reconstructed clone must have exactly the published dna_hash"
    );

    // Bob joins that reconstructed clone as a brand-new cell on his own
    // conductor, then resolves the NFT AUTHORITATIVELY from it — proving
    // discovery is only a reference; the real NFT state comes from the
    // collection cell itself, never from the discovery record.
    let bob_joined_app = conductors[1]
        .setup_app("bob-joins-alices-collection", &[reconstructed_dna])
        .await
        .unwrap();
    let bob_joined_nft_cell = bob_joined_app.cells()[0].clone();
    let zjb = bob_joined_nft_cell.zome(NFT_ZOME);

    exchange_peer_info_for_space(&conductors, &alice_dna_hash).await;
    await_consistency(&[alice_nft_cell.clone(), bob_joined_nft_cell.clone()])
        .await
        .unwrap();

    let records: Vec<Record> = conductors[1].call(&zjb, "get_all_nfts", ()).await;
    assert_eq!(records.len(), 1, "Bob must see exactly the one NFT Alice minted");
    assert_eq!(
        records[0].action_address(),
        &minted.nft,
        "the resolved record must be Alice's actual mint, not a fabricated one"
    );
}

/// A registration can claim a `dna_hash` that does NOT match the
/// `network_seed`/`properties` it also carries (nothing in `social`'s
/// `validate()` can catch this — see the integrity module's
/// authorization-boundary note). Proves that reconstructing the clone from
/// the ingredients and comparing the resulting hash to the CLAIMED hash
/// (exactly what `openDiscoveredCollection` does) reliably catches the
/// mismatch — a forged reference cannot silently resolve to the wrong
/// collection's NFTs.
#[tokio::test(flavor = "multi_thread")]
async fn a_forged_dna_hash_claim_is_caught_by_reconstruction_not_by_validation() {
    let mut conductor = SweetConductor::standard().await;
    let real_nft_dna = nft_dna_for_collection("real-seed", "Real Collection").await;
    let social = social_dna().await;
    let app = conductor
        .setup_app(
            "p2p_art_marketplace",
            &[
                ("nft".to_string(), real_nft_dna.clone()),
                ("social".to_string(), social),
            ],
        )
        .await
        .unwrap();
    let cells = app.cells().clone();
    let (nft_cell, social_cell) = (cells[0].clone(), cells[1].clone());
    let zs = social_cell.zome(SOCIAL_ZOME);
    let c = &conductor;

    let real_dna_hash = nft_cell.dna_hash().clone();

    // A bogus, unrelated dna_hash — NOT what "real-seed" + these properties
    // actually hash to.
    let mut fake_bytes = [0xEE; 32];
    fake_bytes[0] = 0x01;
    let fake_dna_hash = DnaHash::from_raw_32(fake_bytes.to_vec());
    assert_ne!(fake_dna_hash, real_dna_hash);

    let properties_json = serde_json::to_string(&collection_properties("Real Collection")).unwrap();
    let _reg: Record = c
        .call(
            &zs,
            "register_nft_collection",
            RegisterNftCollectionInput {
                dna_hash: fake_dna_hash.clone(),
                network_seed: "real-seed".into(),
                properties_json: properties_json.clone(),
            },
        )
        .await;

    let discovered: Vec<NftCollectionDiscoveryView> = c.call(&zs, "discover_nft_collections", ()).await;
    let found = discovered
        .iter()
        .find(|d| d.dna_hash == fake_dna_hash)
        .expect("the (forged) registration is still discoverable — validation cannot catch this");

    // Reconstructing from the record's own network_seed/properties yields
    // the REAL collection's hash, not the claimed one — the mismatch is what
    // a resolver must check before trusting the reference.
    let reconstructed_props: CollectionPropertiesMirror =
        serde_json::from_str(&found.properties_json).unwrap();
    let sb = SerializedBytes::try_from(reconstructed_props).unwrap();
    let reconstructed_dna = SweetDnaFile::from_bundle_with_overrides(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(NFT_DNA_PATH),
        DnaModifiersOpt::default()
            .with_network_seed(found.network_seed.clone())
            .with_properties(sb),
    )
    .await
    .unwrap();

    assert_eq!(reconstructed_dna.dna_hash(), &real_dna_hash);
    assert_ne!(
        reconstructed_dna.dna_hash(),
        &found.dna_hash,
        "a forged dna_hash claim must be detectable: the reconstructed hash never matches it"
    );
}
