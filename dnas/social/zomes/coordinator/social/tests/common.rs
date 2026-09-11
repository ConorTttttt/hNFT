use holochain::prelude::*;
use holochain::sweettest::*;
use std::path::Path;

pub const DNA_PATH: &str = "../../../workdir/social.dna";
pub const ZOME: &str = "social";

pub async fn dna() -> DnaFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(DNA_PATH);
    SweetDnaFile::from_bundle(&path).await.unwrap()
}

pub async fn setup(n: usize) -> (SweetConductorBatch, Vec<SweetCell>) {
    let mut conductors = SweetConductorBatch::standard(n).await;
    let dna_file = dna().await;
    let apps = conductors.setup_app("social", &[dna_file]).await.unwrap();
    let cells = apps.cells_flattened();
    (conductors, cells)
}

/// A distinct fake target hash per `seed` — social targets are opaque,
/// cross-DNA references that `validate()` never resolves, so a fabricated
/// `AnyLinkableHash` (wrapping a fabricated `ActionHash`) is exactly as valid
/// a test fixture as a real NFT's hash would be.
pub fn fake_target(seed: u8) -> AnyLinkableHash {
    let mut bytes = [seed; 32];
    bytes[0] = seed.wrapping_add(1);
    ActionHash::from_raw_32(bytes.to_vec()).into()
}
