use crate::canonical::parse_hex32;
use crate::generation::GenerationConfig;
use crate::randomness::RandomnessSpec;
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
    /// Seconds after a `BlindPurchaseCommitment` before anyone may author a
    /// `BlindPurchaseAbandoned` marker for it. Same shape as
    /// `transfer_window_secs`; see `blind_collection.rs`'s
    /// `validate_create_blind_purchase_abandoned` and DESIGN.md "Resolution
    /// liveness" for what this marker does and doesn't do (UI-only signal
    /// for this demo -- no on-chain dispute/refund path).
    #[serde(default)]
    pub blind_resolution_window_secs: u64,
    /// Explicit stable, opaque collection namespace: exactly 64 lowercase hex
    /// characters (32 bytes). NOT the DNA hash (which changes on migration).
    /// Required for blind batches and for canonical G3 randomness derivation.
    #[serde(default)]
    pub collection_id: Option<String>,
    /// Provider-independent randomness configuration (G3a).
    #[serde(default)]
    pub randomness: Option<RandomnessSpec>,
    /// G1: trait catalogue + generation version for a generative collection.
    /// `None` = not a generative collection. Validated on read via
    /// `generation_config()`; see `generation.rs`.
    #[serde(default)]
    pub generation: Option<GenerationConfig>,
    /// Per-agent mint whitelist (Phase J.1). `None` or `[]` = whitelist mode is
    /// off; `minter_pubkey`/`max_supply` behave exactly as before. Non-empty =
    /// WHITELIST mint mode: `minter_pubkey` is no longer consulted for mint
    /// authorization (see [`Self::mint_mode`]) — only an agent listed here may
    /// mint, each capped at their own `allowance`. Immutable for the DNA's
    /// lifetime, same as every other field here: there is no add/remove-agent
    /// operation after collection creation (see the Phase 5 audit, §F).
    #[serde(default)]
    pub whitelist: Option<Vec<WhitelistEntry>>,
}

/// One whitelist entry: an agent's lifetime mint allowance under WHITELIST
/// mode. `agent` is base64, same convention as `minter_pubkey`, because
/// `CollectionProperties` round-trips through DNA `modifiers.properties`
/// (JSON), which cannot carry raw `AgentPubKey` bytes as first-class values.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WhitelistEntry {
    pub agent: String,
    /// Must be `>= 1`. A zero allowance is rejected at parse time — omit the
    /// entry instead of writing an allowance of zero.
    pub allowance: u32,
}

/// This collection's resolved mint-authorization mode. Never constructed by
/// hand outside [`CollectionProperties::mint_mode`] — always the result of
/// parsing/validating `CollectionProperties`.
#[derive(Debug, Clone)]
pub enum MintMode {
    /// No author restriction (today's `minter_pubkey: None`).
    Public,
    /// Exactly one agent may mint (today's `minter_pubkey: Some(_)`).
    CreatorOnly(AgentPubKey),
    /// Only a listed agent may mint, each capped at their own allowance.
    /// `(agent, allowance)` pairs, duplicate-free, each allowance `>= 1`.
    Whitelist(Vec<(AgentPubKey, u32)>),
}

impl CollectionProperties {
    pub fn get() -> ExternResult<Self> {
        dna_info()?.modifiers.properties.try_into().map_err(|e| {
            wasm_error!(WasmErrorInner::Guest(format!(
                "Failed to deserialize CollectionProperties: {e:?}"
            )))
        })
    }

    /// The validated generative config, if this collection has one.
    pub fn generation_config() -> ExternResult<Option<GenerationConfig>> {
        match Self::get()?.generation {
            None => Ok(None),
            Some(cfg) => {
                cfg.validate().map_err(|e| {
                    wasm_error!(WasmErrorInner::Guest(format!(
                        "invalid generation config: {e}"
                    )))
                })?;
                Ok(Some(cfg))
            }
        }
    }

    /// The validated 32-byte collection id.
    pub fn collection_id_bytes() -> ExternResult<[u8; 32]> {
        Self::get()?.collection_id_bytes_checked()
    }

    pub fn collection_id_bytes_checked(&self) -> ExternResult<[u8; 32]> {
        let s = self
            .collection_id
            .as_ref()
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "this collection has no collection_id".into()
            )))?;
        parse_hex32(s)
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("invalid collection_id: {e}"))))
    }

    /// `Ok(None)` = open minting (dev). `Ok(Some(_))` = the minting authority.
    /// NOTE: this reflects only `minter_pubkey`, i.e. the CREATOR_ONLY case.
    /// It is unaware of WHITELIST mode — callers that need the full picture
    /// (mint authorization) must use [`Self::mint_mode`] instead. Kept as-is
    /// for existing callers (`is_minter`) that only ever meant "am I the sole
    /// fixed minter", never "may I mint at all".
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

    /// Resolves this collection's mint-authorization mode (Phase J.1).
    ///
    /// WHITELIST takes priority whenever `whitelist` is present and non-empty
    /// — `minter_pubkey` is then ignored for authorization purposes (it may
    /// still be set for other reasons, but ambiguity between the two modes is
    /// not possible: WHITELIST always wins so there is exactly one active
    /// mode). Otherwise this is exactly today's behavior: `minter_pubkey`
    /// present => CREATOR_ONLY, absent => PUBLIC.
    ///
    /// A malformed whitelist (bad key encoding, a duplicate agent, a zero
    /// allowance, or — when `max_supply > 0` — allowances summing above
    /// `max_supply`) is an error here, never a silent fallback to a weaker
    /// mode. Per the Phase 5 audit §E, there is no race-free way to enforce
    /// an aggregate `max_supply` across multiple independent agents' chains
    /// at mint time; the sum-check here is what makes the aggregate cap safe
    /// in WHITELIST mode instead — a load-time invariant on the fixed
    /// allowance table, not a runtime race. Because this runs inside
    /// `validate()`, a bad whitelist causes every mint in the collection to
    /// be rejected (fails closed) rather than exceeding the cap.
    pub fn mint_mode(&self) -> ExternResult<MintMode> {
        if let Some(entries) = self.whitelist.as_ref() {
            if !entries.is_empty() {
                return Ok(MintMode::Whitelist(Self::parse_whitelist(
                    entries,
                    self.max_supply,
                )?));
            }
        }
        match self.minter_pubkey.as_ref() {
            Some(s) => AgentPubKey::try_from(s.clone())
                .map(MintMode::CreatorOnly)
                .map_err(|e| {
                    wasm_error!(WasmErrorInner::Guest(format!(
                        "Invalid minter_pubkey in CollectionProperties: {e:?}"
                    )))
                }),
            None => Ok(MintMode::Public),
        }
    }

    fn parse_whitelist(
        entries: &[WhitelistEntry],
        max_supply: u32,
    ) -> ExternResult<Vec<(AgentPubKey, u32)>> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::with_capacity(entries.len());
        let mut total: u64 = 0;
        for e in entries {
            let agent = AgentPubKey::try_from(e.agent.clone()).map_err(|err| {
                wasm_error!(WasmErrorInner::Guest(format!(
                    "whitelist: invalid agent key {:?}: {err:?}",
                    e.agent
                )))
            })?;
            if e.allowance == 0 {
                return Err(wasm_error!(WasmErrorInner::Guest(format!(
                    "whitelist: agent {:?} has a zero allowance (omit the entry instead)",
                    e.agent
                ))));
            }
            if !seen.insert(agent.clone()) {
                return Err(wasm_error!(WasmErrorInner::Guest(format!(
                    "whitelist: duplicate agent {:?}",
                    e.agent
                ))));
            }
            total += e.allowance as u64;
            out.push((agent, e.allowance));
        }
        if max_supply > 0 && total > max_supply as u64 {
            return Err(wasm_error!(WasmErrorInner::Guest(format!(
                "whitelist: allowances sum to {total}, exceeding max_supply {max_supply}"
            ))));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(byte: u8) -> AgentPubKey {
        // `from_raw_32` computes a correct DHT-location checksum for us — needed
        // here because these round-trip through `to_string()`/`try_from`, which
        // validates it (unlike `from_raw_36`, which trusts its last 4 bytes).
        AgentPubKey::from_raw_32(vec![byte; 32])
    }

    fn base(whitelist: Option<Vec<WhitelistEntry>>, max_supply: u32) -> CollectionProperties {
        CollectionProperties {
            name: "Test".into(),
            symbol: "T".into(),
            max_supply,
            minter_pubkey: None,
            royalty_bps: 0,
            transfer_window_secs: 300,
            blind_resolution_window_secs: 0,
            collection_id: None,
            randomness: None,
            generation: None,
            whitelist,
        }
    }

    #[test]
    fn no_whitelist_falls_back_to_public() {
        let props = base(None, 0);
        assert!(matches!(props.mint_mode().unwrap(), MintMode::Public));
    }

    #[test]
    fn empty_whitelist_falls_back_to_creator_only() {
        let alice = agent(1);
        let mut props = base(Some(vec![]), 0);
        props.minter_pubkey = Some(alice.to_string());
        match props.mint_mode().unwrap() {
            MintMode::CreatorOnly(m) => assert_eq!(m, alice),
            other => panic!("expected CreatorOnly, got {other:?}"),
        }
    }

    #[test]
    fn whitelist_present_ignores_minter_pubkey() {
        let alice = agent(1);
        let bob = agent(2);
        let mut props = base(
            Some(vec![WhitelistEntry {
                agent: alice.to_string(),
                allowance: 5,
            }]),
            0,
        );
        // Even with a `minter_pubkey` set to a DIFFERENT agent, whitelist mode
        // wins outright — there is exactly one active mode, never both.
        props.minter_pubkey = Some(bob.to_string());
        match props.mint_mode().unwrap() {
            MintMode::Whitelist(entries) => assert_eq!(entries, vec![(alice, 5)]),
            other => panic!("expected Whitelist, got {other:?}"),
        }
    }

    #[test]
    fn whitelist_resolves_every_agent_and_allowance() {
        let alice = agent(1);
        let bob = agent(2);
        let props = base(
            Some(vec![
                WhitelistEntry {
                    agent: alice.to_string(),
                    allowance: 5,
                },
                WhitelistEntry {
                    agent: bob.to_string(),
                    allowance: 10,
                },
            ]),
            100,
        );
        match props.mint_mode().unwrap() {
            MintMode::Whitelist(entries) => {
                assert_eq!(entries, vec![(alice, 5), (bob, 10)]);
            }
            other => panic!("expected Whitelist, got {other:?}"),
        }
    }

    #[test]
    fn zero_allowance_is_rejected() {
        let alice = agent(1);
        let props = base(
            Some(vec![WhitelistEntry {
                agent: alice.to_string(),
                allowance: 0,
            }]),
            0,
        );
        assert!(props.mint_mode().is_err());
    }

    #[test]
    fn duplicate_agent_is_rejected() {
        let alice = agent(1);
        let props = base(
            Some(vec![
                WhitelistEntry {
                    agent: alice.to_string(),
                    allowance: 5,
                },
                WhitelistEntry {
                    agent: alice.to_string(),
                    allowance: 1,
                },
            ]),
            0,
        );
        assert!(props.mint_mode().is_err());
    }

    #[test]
    fn invalid_agent_encoding_is_rejected() {
        let props = base(
            Some(vec![WhitelistEntry {
                agent: "not-a-valid-agent-key".into(),
                allowance: 5,
            }]),
            0,
        );
        assert!(props.mint_mode().is_err());
    }

    #[test]
    fn allowances_summing_within_max_supply_are_accepted() {
        let alice = agent(1);
        let bob = agent(2);
        let props = base(
            Some(vec![
                WhitelistEntry {
                    agent: alice.to_string(),
                    allowance: 5,
                },
                WhitelistEntry {
                    agent: bob.to_string(),
                    allowance: 10,
                },
            ]),
            15,
        );
        assert!(props.mint_mode().is_ok());
    }

    #[test]
    fn allowances_exceeding_max_supply_are_rejected() {
        let alice = agent(1);
        let bob = agent(2);
        let props = base(
            Some(vec![
                WhitelistEntry {
                    agent: alice.to_string(),
                    allowance: 5,
                },
                WhitelistEntry {
                    agent: bob.to_string(),
                    allowance: 10,
                },
            ]),
            14,
        );
        assert!(props.mint_mode().is_err());
    }

    #[test]
    fn unlimited_max_supply_skips_the_sum_check() {
        let alice = agent(1);
        let props = base(
            Some(vec![WhitelistEntry {
                agent: alice.to_string(),
                allowance: u32::MAX,
            }]),
            0, // 0 = unlimited
        );
        assert!(props.mint_mode().is_ok());
    }
}
