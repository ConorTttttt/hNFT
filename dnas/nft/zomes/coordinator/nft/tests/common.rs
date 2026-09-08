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
    }
}

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
