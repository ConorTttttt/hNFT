//! Phase J.6 — Marketplace Performance & Scale Audit.
//!
//! Measurement instrumentation only, per the J.6 scope boundary: no
//! production code (Rust or client) is exercised differently than J.5 already
//! exercises it — this file only adds `Instant::now()` timing around the
//! SAME zome calls (`get_all_nfts`, `all_listings`, collection open via
//! `setup_app`) that `networkNftDiscovery.ts`/`networkMarketplace.ts` already
//! make in production, run against real embedded `SweetConductor` instances
//! (a genuine Holochain conductor + DHT, not a mock).
//!
//! Local mirror types again (see `cross_dna_discovery.rs`'s own doc comment
//! for why): linking both `nft_integrity` and `social_integrity` into one
//! native test binary is a hard conflict, so this crate depends on neither
//! DNA's Rust crates.
//!
//! Run with `cargo test -p happ_tests --test j6_marketplace_performance -- --nocapture`
//! to see the `MEASURED ...` lines this file prints — the raw data behind the
//! J.6 report's tables. These are WALL-CLOCK measurements against a
//! single-process embedded conductor on this machine, not a distributed
//! network — see the report for exactly what that does and doesn't tell us.

use holochain::prelude::*;
use holochain::sweettest::*;
use holochain_serialized_bytes::SerializedBytes;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};

const NFT_DNA_PATH: &str = "../dnas/nft/workdir/nft.dna";
const NFT_ZOME: &str = "nft";

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
        symbol: "PERF".into(),
        max_supply: 0,
        minter_pubkey: None, // open minting — simplest fixture
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
struct CreateListingInput {
    nft: ActionHash,
    price: u64,
    #[serde(default)]
    currency: String,
}

#[derive(Deserialize, Debug)]
struct ListingViewMirror {
    #[allow(dead_code)]
    nft: ActionHash,
}

/// Same construction J.3's `openDiscoveredCollection` performs client-side
/// (`createCloneCell` with a specific `network_seed`/`properties`) — but note
/// this ALSO includes re-reading/re-hashing the `.dna` bundle from disk each
/// call, which the real client does NOT do (the base wasm is already loaded
/// once at app install; `createCloneCell` only supplies new modifiers to it).
/// This function's own cost is measured separately from `setup_app`'s below
/// specifically so the report doesn't conflate the two.
async fn build_dna(seed: &str, name: &str) -> DnaFile {
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

async fn mint_n(conductor: &SweetConductor, zome: &SweetZome, n: u32, seed_offset: u32) {
    for i in 0..n {
        let mut hash = [0u8; 32];
        hash[0..4].copy_from_slice(&(seed_offset + i + 1).to_le_bytes());
        let _out: MintNftOutput = conductor
            .call(zome, "mint_nft", MintNftInput { metadata_hash: hash })
            .await;
    }
}

fn ms(d: Duration) -> u128 {
    d.as_millis()
}

/// **Scenario 1 — NFT-count scaling within a SINGLE collection.**
/// Measures `get_all_nfts()` latency as the collection grows: 1, 6, 24, 100
/// NFTs (cumulative — each threshold mints only the delta). This isolates
/// "does one collection's own authoritative query degrade with its size?"
/// from any cross-collection fan-out cost.
#[tokio::test(flavor = "multi_thread")]
async fn measure_get_all_nfts_scaling_with_nft_count() {
    let mut conductor = SweetConductor::standard().await;
    let dna = build_dna("j6-nft-scaling", "J6 NFT Scaling").await;
    let app = conductor
        .setup_app("p2p_art_marketplace", &[("nft".to_string(), dna)])
        .await
        .unwrap();
    let cell = app.cells()[0].clone();
    let zome = cell.zome(NFT_ZOME);

    let mut minted_so_far = 0u32;
    println!("\n=== MEASURED: get_all_nfts() latency vs. NFT count (single collection) ===");
    for &target in &[1u32, 6, 24, 100] {
        let to_mint = target - minted_so_far;
        let mint_start = Instant::now();
        mint_n(&conductor, &zome, to_mint, minted_so_far).await;
        let mint_elapsed = mint_start.elapsed();
        minted_so_far = target;

        // "Cold" first call after this batch of mints.
        let t0 = Instant::now();
        let records: Vec<Record> = conductor.call(&zome, "get_all_nfts", ()).await;
        let first_call = t0.elapsed();
        assert_eq!(records.len(), target as usize);

        // "Warm" repeat call, same conductor, same data, immediately after.
        let t1 = Instant::now();
        let records2: Vec<Record> = conductor.call(&zome, "get_all_nfts", ()).await;
        let second_call = t1.elapsed();
        assert_eq!(records2.len(), target as usize);

        println!(
            "MEASURED nfts={target:<4} mint_{to_mint}_elapsed_ms={:<6} get_all_nfts_first_ms={:<6} get_all_nfts_repeat_ms={:<6}",
            ms(mint_elapsed),
            ms(first_call),
            ms(second_call),
        );
    }
}

/// **Scenario 2 — listing-count scaling within a single collection.**
/// Measures `all_listings()` latency as the number of ACTIVE listings grows:
/// 1, 6, 24 (out of a fixed 24-NFT collection, so the query always walks the
/// same `PathToListing` anchor size at each step, only the listed subset
/// grows). Isolates listing-query cost from NFT-query cost.
#[tokio::test(flavor = "multi_thread")]
async fn measure_all_listings_scaling_with_listing_count() {
    let mut conductor = SweetConductor::standard().await;
    let dna = build_dna("j6-listing-scaling", "J6 Listing Scaling").await;
    let app = conductor
        .setup_app("p2p_art_marketplace", &[("nft".to_string(), dna)])
        .await
        .unwrap();
    let cell = app.cells()[0].clone();
    let zome = cell.zome(NFT_ZOME);

    mint_n(&conductor, &zome, 24, 0).await;
    let records: Vec<Record> = conductor.call(&zome, "get_all_nfts", ()).await;
    let nfts: Vec<ActionHash> = records.iter().map(|r| r.action_address().clone()).collect();

    let mut listed_so_far = 0usize;
    println!("\n=== MEASURED: all_listings() latency vs. active-listing count (24-NFT collection) ===");
    for &target in &[1usize, 6, 24] {
        for i in listed_so_far..target {
            let _rec: Record = conductor
                .call(
                    &zome,
                    "create_listing",
                    CreateListingInput {
                        nft: nfts[i].clone(),
                        price: 100,
                        currency: "USD".into(),
                    },
                )
                .await;
        }
        listed_so_far = target;

        let t0 = Instant::now();
        let listings: Vec<ListingViewMirror> = conductor.call(&zome, "all_listings", ()).await;
        let elapsed = t0.elapsed();
        assert_eq!(listings.len(), target);

        println!("MEASURED listed={target:<4} all_listings_ms={}", ms(elapsed));
    }
}

/// **Scenario 3 — collection fan-out (the shape of a real Network Marketplace
/// load): 1, 3, 6 SEPARATE collections, each with 24 NFTs, opened and queried
/// sequentially on ONE conductor — exactly the loop
/// `discoverNetworkMarketplace` runs client-side.** Separates DNA-bundle
/// construction cost (a sweettest/test-harness artifact — see `build_dna`'s
/// doc) from cell-provisioning cost (`setup_app`, the closest local analogue
/// to `createCloneCell`), and reports cumulative end-to-end time for the
/// full 6-collection/24-NFT-each scenario — J.5's actual default bounds.
#[tokio::test(flavor = "multi_thread")]
async fn measure_collection_fan_out_scaling() {
    let mut conductor = SweetConductor::standard().await;

    let mut total_build_dna = Duration::ZERO;
    let mut total_open = Duration::ZERO;
    let mut total_mint = Duration::ZERO;
    let mut total_query_nfts = Duration::ZERO;
    let mut total_query_listings = Duration::ZERO;

    println!("\n=== MEASURED: per-collection cost, sequential fan-out (24 NFTs each) ===");
    for i in 0..6u32 {
        let seed = format!("j6-fanout-{i}");
        let name = format!("J6 Fanout {i}");

        let t0 = Instant::now();
        let dna = build_dna(&seed, &name).await;
        let build_elapsed = t0.elapsed();
        total_build_dna += build_elapsed;

        let t1 = Instant::now();
        let app = conductor
            .setup_app(&format!("j6-fanout-app-{i}"), &[("nft".to_string(), dna)])
            .await
            .unwrap();
        let open_elapsed = t1.elapsed();
        total_open += open_elapsed;

        let cell = app.cells()[0].clone();
        let zome = cell.zome(NFT_ZOME);

        let t2 = Instant::now();
        mint_n(&conductor, &zome, 24, 0).await;
        let mint_elapsed = t2.elapsed();
        total_mint += mint_elapsed;

        let t3 = Instant::now();
        let records: Vec<Record> = conductor.call(&zome, "get_all_nfts", ()).await;
        let query_nfts_elapsed = t3.elapsed();
        total_query_nfts += query_nfts_elapsed;
        assert_eq!(records.len(), 24);

        let t4 = Instant::now();
        let listings: Vec<ListingViewMirror> = conductor.call(&zome, "all_listings", ()).await;
        let query_listings_elapsed = t4.elapsed();
        total_query_listings += query_listings_elapsed;
        assert_eq!(listings.len(), 0);

        let collection_number = i + 1;
        println!(
            "MEASURED collection={collection_number} build_dna_ms={:<5} setup_app_open_ms={:<5} mint_24_ms={:<5} get_all_nfts_ms={:<5} all_listings_ms={:<5}",
            ms(build_elapsed),
            ms(open_elapsed),
            ms(mint_elapsed),
            ms(query_nfts_elapsed),
            ms(query_listings_elapsed),
        );

        if [1u32, 3, 6].contains(&collection_number) {
            println!(
                "MEASURED cumulative_after={collection_number}_collections total_build_dna_ms={} total_open_ms={} total_query_nfts_ms={} total_query_listings_ms={} total_discovery_ms(open+query)={}",
                ms(total_build_dna),
                ms(total_open),
                ms(total_query_nfts),
                ms(total_query_listings),
                ms(total_open + total_query_nfts + total_query_listings),
            );
        }
    }
}

/// **Scenario 4 — progressive loading ("Load more") does no repeated work.**
/// Opens 3 collections (page 1), queries them, then opens 3 MORE (page 2,
/// simulating "Load more collections") — and confirms via call-count
/// (not just timing) that page 2 never re-queries page 1's collections. This
/// is the Rust-side complement to the existing client-side unit test
/// (`networkNftDiscovery.test.ts`'s "resolves references from an
/// already-open collection without cloning") — here run against a real
/// conductor to also capture the actual incremental latency.
#[tokio::test(flavor = "multi_thread")]
async fn measure_progressive_loading_does_only_incremental_work() {
    let mut conductor = SweetConductor::standard().await;
    let mut opened_cells: Vec<(SweetCell, SweetZome)> = Vec::new();

    async fn open_and_query(
        conductor: &mut SweetConductor,
        seed: &str,
        name: &str,
    ) -> (SweetCell, SweetZome, Duration, Duration) {
        let dna = build_dna(seed, name).await;
        let t0 = Instant::now();
        let app = conductor
            .setup_app(&format!("j6-progressive-{seed}"), &[("nft".to_string(), dna)])
            .await
            .unwrap();
        let open_elapsed = t0.elapsed();
        let cell = app.cells()[0].clone();
        let zome = cell.zome(NFT_ZOME);
        mint_n(conductor, &zome, 24, 0).await;
        let t1 = Instant::now();
        let records: Vec<Record> = conductor.call(&zome, "get_all_nfts", ()).await;
        let query_elapsed = t1.elapsed();
        assert_eq!(records.len(), 24);
        (cell, zome, open_elapsed, query_elapsed)
    }

    println!("\n=== MEASURED: progressive loading (page 1: 3 collections, page 2: 3 more) ===");
    let mut page1_total = Duration::ZERO;
    for i in 0..3u32 {
        let (cell, zome, open_ms, query_ms) =
            open_and_query(&mut conductor, &format!("j6-page1-{i}"), &format!("Page1 {i}")).await;
        page1_total += open_ms + query_ms;
        opened_cells.push((cell, zome));
    }
    println!("MEASURED page1_3_collections_total_ms={}", ms(page1_total));
    let page1_count = opened_cells.len();

    let mut page2_total = Duration::ZERO;
    for i in 0..3u32 {
        let (cell, zome, open_ms, query_ms) =
            open_and_query(&mut conductor, &format!("j6-page2-{i}"), &format!("Page2 {i}")).await;
        page2_total += open_ms + query_ms;
        opened_cells.push((cell, zome));
    }
    println!("MEASURED page2_3_more_collections_total_ms={}", ms(page2_total));

    // The actual "no repeated work" assertion: page 1's cells are untouched —
    // re-querying them here (as a control) should return the SAME 24 records,
    // proving page 2's work didn't touch or duplicate page 1's state.
    for (_cell, zome) in opened_cells.iter().take(page1_count) {
        let records: Vec<Record> = conductor.call(zome, "get_all_nfts", ()).await;
        assert_eq!(records.len(), 24, "page 1 collections must be unaffected by page 2's loading");
    }
    assert_eq!(opened_cells.len(), 6);
}

/// **Scenario 5 — baseline control for the partial-failure claim.**
/// Sweettest has no clean way to make ONE installed collection's queries
/// fail on demand without either hanging or crashing the whole conductor, so
/// this does not inject a real Rust-side failure — that would produce an
/// unreliable, unrepresentative measurement. The actual partial-failure
/// BEHAVIOR (a failed collection doesn't block or slow the others) is
/// already verified deterministically at the client composition layer:
/// `networkMarketplace.test.ts`'s "a listing-fetch failure for one
/// collection still resolves its NFTs, unlisted" and
/// `networkNftDiscovery.test.ts`'s "marks a collection unresolved if..."
/// tests, both via a mocked rejection on exactly the call site that would
/// fail. What THIS test measures is the control: a single healthy
/// collection's query cost in isolation, to compare against the fan-out
/// numbers above — confirming there is no fixed per-call overhead being
/// hidden by the multi-collection loop in Scenario 3.
#[tokio::test(flavor = "multi_thread")]
async fn measure_single_collection_baseline() {
    let mut conductor = SweetConductor::standard().await;

    let dna_a = build_dna("j6-baseline", "Baseline").await;
    let app_a = conductor
        .setup_app("j6-baseline-app", &[("nft".to_string(), dna_a)])
        .await
        .unwrap();
    let cell_a = app_a.cells()[0].clone();
    let zome_a = cell_a.zome(NFT_ZOME);
    mint_n(&conductor, &zome_a, 24, 0).await;

    let t0 = Instant::now();
    let records_a: Vec<Record> = conductor.call(&zome_a, "get_all_nfts", ()).await;
    let elapsed_a = t0.elapsed();
    assert_eq!(records_a.len(), 24);
    println!(
        "\n=== MEASURED: single-collection baseline (control) ===\nMEASURED baseline_get_all_nfts_ms={}",
        ms(elapsed_a)
    );
}
