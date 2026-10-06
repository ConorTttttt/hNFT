// Shared by several test crates; each uses only a subset of these helpers.
#![allow(dead_code)]

use holochain::prelude::*;
use holochain::sweettest::*;
use nft_integrity::*;
use std::path::Path;

pub const DNA_PATH: &str = "../../../workdir/nft.dna";
pub const ZOME: &str = "nft";

pub fn dev_props(
    minter: Option<AgentPubKey>,
    max_supply: u32,
    transfer_window_secs: u64,
) -> CollectionProperties {
    CollectionProperties {
        name: "Test Collection".into(),
        symbol: "TEST".into(),
        max_supply,
        minter_pubkey: minter.map(|a| a.to_string()),
        royalty_bps: 0,
        transfer_window_secs,
        blind_resolution_window_secs: 0,
        collection_id: None,
        randomness: None,
        generation: None,
        whitelist: None,
    }
}

/// A collection with WHITELIST mint mode: only `entries`' agents may mint,
/// each capped at their own allowance. `minter_pubkey` is left `None` — it is
/// ignored once `whitelist` is non-empty (see `CollectionProperties::mint_mode`).
pub fn whitelist_props(
    entries: Vec<(AgentPubKey, u32)>,
    max_supply: u32,
    transfer_window_secs: u64,
) -> CollectionProperties {
    CollectionProperties {
        whitelist: Some(
            entries
                .into_iter()
                .map(|(agent, allowance)| WhitelistEntry {
                    agent: agent.to_string(),
                    allowance,
                })
                .collect(),
        ),
        ..dev_props(None, max_supply, transfer_window_secs)
    }
}

/// A collection configured for G3a blind batches: an explicit `collection_id`
/// and the INSECURE dev randomness verifier. Open minting (any agent may mint);
/// batch ownership is enforced by the batch-author checks, not the minter key.
pub fn blind_props(transfer_window_secs: u64) -> CollectionProperties {
    CollectionProperties {
        collection_id: Some("c0".repeat(32)),
        randomness: Some(RandomnessSpec {
            provider_id: INSECURE_DEV_PROVIDER_ID.into(),
            verifier_params: String::new(),
        }),
        ..dev_props(None, 0, transfer_window_secs)
    }
}

/// The decoded `collection_id` used by `blind_props` (32 bytes of 0xC0).
pub const BLIND_COLLECTION_ID: [u8; 32] = [0xC0; 32];

/// Load the packed DNA, optionally overriding `modifiers.properties`.
pub async fn dna(props: Option<CollectionProperties>) -> DnaFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(DNA_PATH);
    match props {
        None => SweetDnaFile::from_bundle(&path).await.unwrap(),
        Some(p) => {
            let sb = SerializedBytes::try_from(p).unwrap();
            SweetDnaFile::from_bundle_with_overrides(
                &path,
                DnaModifiersOpt::default().with_properties(sb),
            )
            .await
            .unwrap()
        }
    }
}

pub async fn setup(
    n: usize,
    props: Option<CollectionProperties>,
) -> (SweetConductorBatch, Vec<SweetCell>) {
    let mut conductors = SweetConductorBatch::standard(n).await;
    let dna_file = dna(props).await;
    let apps = conductors.setup_app("nft", &[dna_file]).await.unwrap();
    let cells = apps.cells_flattened();
    (conductors, cells)
}

/// A distinct fake commitment hash per `seed`.
pub fn metadata_hash(seed: u8) -> [u8; 32] {
    let mut h = [seed; 32];
    h[0] = seed.wrapping_add(1);
    h
}
