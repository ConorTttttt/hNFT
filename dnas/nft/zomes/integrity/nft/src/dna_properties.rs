use hdi::prelude::*;

/// Per-collection configuration, baked into the DNA hash via `modifiers.properties`
/// at clone-create time. Safe to read in `validate()` (`dna_info()`), unlike
/// `init_properties`, because every validator on this DHT sees identical bytes by
/// construction. See references/progenitor.md.
#[derive(Serialize, Deserialize, SerializedBytes, Clone, Debug)]
pub struct CollectionProperties {
    pub name: String,
    pub symbol: String,
    /// Hard cap on mints. `0` = unlimited (cap not enforced).
    pub max_supply: u32,
    /// Base64 `AgentPubKey`; the only agent permitted to mint in this collection.
    /// `None` = open minting (dev/bootstrap only — the cap cannot be enforced
    /// without a single minting authority, see the architecture doc §1.5).
    pub minter_pubkey: Option<String>,
    /// Present but unenforced in v1. v2 seam for royalty payout preflight.
    #[serde(default)]
    pub royalty_bps: u16,
    /// Seconds a `TransferCommitment` stays open for the buyer to accept. After
    /// this window the seller may `TransferCancellation` it; a `TransferReceipt`
    /// is only valid *within* the window. The two verdicts are therefore disjoint
    /// by action timestamp. Baked into the DNA so every validator agrees.
    pub transfer_window_secs: u64,
}

impl CollectionProperties {
    pub fn get() -> ExternResult<Self> {
        dna_info()?
            .modifiers
            .properties
            .try_into()
            .map_err(|e| {
                wasm_error!(WasmErrorInner::Guest(format!(
                    "Failed to deserialize CollectionProperties: {e:?}"
                )))
            })
    }

    /// `Ok(None)` = open minting (dev). `Ok(Some(_))` = the minting authority.
    pub fn minter() -> ExternResult<Option<AgentPubKey>> {
        match Self::get()?.minter_pubkey {
            None => Ok(None),
            Some(s) => AgentPubKey::try_from(s).map(Some).map_err(|e| {
                wasm_error!(WasmErrorInner::Guest(format!(
                    "Invalid minter_pubkey in CollectionProperties: {e:?}"
                )))
            }),
        }
    }
}
