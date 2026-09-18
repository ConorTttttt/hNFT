use crate::agreement::MintAgreement;
use crate::dna_properties::CollectionProperties;
use crate::UnitEntryTypes;
use hdi::prelude::*;

/// The NFT. Its identity is the hash of its original Create action — never a field
/// in here. Minter and mint time are on the Create action header. Ownership is
/// derived from the transfer provenance chain, never stored here.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct NftManifest {
    /// Commitment: `blake2b_256(salt || canonical_bytes(attributes, image_uri))`.
    /// Immutable across the Sealed -> Revealed update.
    pub metadata_hash: [u8; 32],
    /// `Sealed` at mint; flipped to `Revealed` exactly once via `update_entry`.
    pub payload: NftPayload,
    /// The `BlindBatchCommitment` this mint belongs to, if minted as part of
    /// a blind-box batch. `None` for an ordinary, non-blind mint. Immutable
    /// across the Sealed -> Revealed update -- see `validate_update_nft_manifest`.
    #[serde(default)]
    pub batch: Option<ActionHash>,
    /// This mint's fixed position within its batch (0..batch_size), assigned
    /// sequentially by the minter at mint time. `None` iff `batch` is `None`.
    /// Immutable across the Sealed -> Revealed update.
    #[serde(default)]
    pub batch_slot: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NftPayload {
    Sealed,
    Revealed {
        salt: [u8; 32],
        attributes: Vec<NftAttribute>,
        image_uri: String,
        /// The `MintAgreement` this reveal was authorised under, if any. Present
        /// when the NFT was minted with an agreement — lets `validate()` check a
        /// delegate-authored reveal. NOT part of `reveal_preimage` (the
        /// commitment binds only salt/attributes/image_uri).
        #[serde(default)]
        agreement: Option<ActionHash>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct NftAttribute {
    pub trait_type: String,
    pub value: String,
}

/// Canonical preimage bytes for the commit-reveal check.
///
/// Layout (every length is a little-endian `u32`):
/// ```text
///   salt[32]
///   attributes.len()
///   for each attribute:  trait_type.len() ‖ trait_type (utf-8)
///                        value.len()      ‖ value (utf-8)
///   image_uri.len() ‖ image_uri (utf-8)
/// ```
/// Deliberately NOT a serde/msgpack encoding — a hand-rolled, length-prefixed
/// format so a TypeScript client can reproduce it byte-for-byte without matching
/// any library's serialization quirks. See `client/src/commitment.ts`.
pub fn reveal_preimage(salt: &[u8; 32], attributes: &[NftAttribute], image_uri: &str) -> Vec<u8> {
    fn put(buf: &mut Vec<u8>, bytes: &[u8]) {
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }
    let mut buf = Vec::with_capacity(32 + 4 + image_uri.len() + 8);
    buf.extend_from_slice(salt);
    buf.extend_from_slice(&(attributes.len() as u32).to_le_bytes());
    for a in attributes {
        put(&mut buf, a.trait_type.as_bytes());
        put(&mut buf, a.value.as_bytes());
    }
    put(&mut buf, image_uri.as_bytes());
    buf
}

/// blake2b-256 of `bytes`, as a fixed array. Uses `holo_hash`'s pure-Rust
/// implementation, which is available and deterministic inside `validate()`.
pub fn commit_hash(bytes: &[u8]) -> [u8; 32] {
    let v = holo_hash::blake2b_256(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&v);
    out
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_create_nft_manifest(
    action: TypedAction<EntryCreationData>,
    nft: NftManifest,
) -> ExternResult<ValidateCallbackResult> {
    let props = CollectionProperties::get()?;

    // 1. Minter authority (progenitor pattern). `None` => dev mode, no restriction.
    if let Some(minter_str) = props.minter_pubkey.as_ref() {
        let minter = AgentPubKey::try_from(minter_str.clone())
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("bad minter_pubkey: {e:?}"))))?;
        if action.author() != &minter {
            return Ok(ValidateCallbackResult::Invalid(
                "only the collection minter may mint".into(),
            ));
        }
    }

    // 2. Must mint Sealed, with a real commitment.
    if !matches!(nft.payload, NftPayload::Sealed) {
        return Ok(ValidateCallbackResult::Invalid("NFT must be minted Sealed".into()));
    }
    if nft.metadata_hash == [0u8; 32] {
        return Ok(ValidateCallbackResult::Invalid("metadata_hash must not be zero".into()));
    }

    // 3. max_supply. Enforceable only because a single minter's own source chain is
    //    strictly ordered by construction — no network race is possible against
    //    yourself. Walk the minter's chain from this mint's predecessor and count
    //    prior NftManifest creates. `must_get_agent_activity` DEFERS on an
    //    incomplete chain (UnresolvedDependencies) rather than failing, so
    //    validators converge; a forked minter chain produces a warrant.
    if props.minter_pubkey.is_some() && props.max_supply > 0 {
        let prev = action
            .prev_action()
            .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("mint has no prev_action".into())))?
            .clone();
        let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
        let minted_so_far = activity
            .iter()
            .filter(|a| is_nft_manifest_create(a.action.action()))
            .count() as u32;
        if minted_so_far >= props.max_supply {
            return Ok(ValidateCallbackResult::Invalid(
                "collection max_supply reached".into(),
            ));
        }
    }

    Ok(ValidateCallbackResult::Valid)
}

/// Reveal: the sole permitted update to an `NftManifest`.
pub fn validate_update_nft_manifest(
    action: TypedAction<UpdateData>,
    nft: NftManifest,
    original_action: TypedAction<EntryCreationData>,
    original_nft: NftManifest,
) -> ExternResult<ValidateCallbackResult> {
    let mint_hash = action.data.original_action_address.clone();

    if action.author() != original_action.author() {
        // Not the minter — allowed only if a MintAgreement names this author as
        // delegate and the minter's reveal_by has passed.
        let NftPayload::Revealed {
            agreement: Some(ag_hash),
            ..
        } = &nft.payload
        else {
            return Ok(ValidateCallbackResult::Invalid(
                "only the minter may reveal".into(),
            ));
        };
        let ag_rec = must_get_valid_record(ag_hash.clone())?;
        let ag: MintAgreement = ag_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "reveal references a non-MintAgreement".into()
            )))?;
        if ag.nft != mint_hash {
            return Ok(ValidateCallbackResult::Invalid(
                "the referenced agreement is for a different NFT".into(),
            ));
        }
        if ag_rec.action().author() != original_action.author() {
            return Ok(ValidateCallbackResult::Invalid(
                "the agreement was not authored by the minter".into(),
            ));
        }
        if ag.delegate.as_ref() != Some(action.author()) {
            return Ok(ValidateCallbackResult::Invalid(
                "reveal author is not the agreement's delegate".into(),
            ));
        }
        if action.timestamp().as_micros() < ag.reveal_by.as_micros() {
            return Ok(ValidateCallbackResult::Invalid(
                "the delegate may reveal only after reveal_by".into(),
            ));
        }
    }
    // Branching a reveal off an already-revealed manifest.
    if !matches!(original_nft.payload, NftPayload::Sealed) {
        return Ok(ValidateCallbackResult::Invalid("NFT already revealed".into()));
    }
    // Branching a second reveal directly off the sealed mint. The author's own
    // chain is strictly ordered, so a prior Update targeting this same mint
    // action means they already revealed it. (With a delegate, minter and
    // delegate could each reveal once on their own chains — harmless: both must
    // hash to the same commitment, and `get_nft` returns the latest.)
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("reveal has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    for a in activity.iter() {
        if let ActionData::Update(u) = &a.action.action().data {
            if u.original_action_address == mint_hash {
                return Ok(ValidateCallbackResult::Invalid("NFT already revealed".into()));
            }
        }
    }
    if nft.metadata_hash != original_nft.metadata_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "metadata_hash may not change on reveal".into(),
        ));
    }
    if nft.batch != original_nft.batch {
        return Ok(ValidateCallbackResult::Invalid(
            "batch may not change on reveal".into(),
        ));
    }
    if nft.batch_slot != original_nft.batch_slot {
        return Ok(ValidateCallbackResult::Invalid(
            "batch_slot may not change on reveal".into(),
        ));
    }
    let NftPayload::Revealed {
        salt,
        ref attributes,
        ref image_uri,
        ..
    } = nft.payload
    else {
        return Ok(ValidateCallbackResult::Invalid(
            "an update must reveal, not stay Sealed".into(),
        ));
    };
    let computed = commit_hash(&reveal_preimage(&salt, attributes, image_uri));
    if computed != nft.metadata_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "revealed metadata does not match the mint commitment".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// True iff `action` is a `Create` of the given unit entry type in this zome.
///
/// NOTE (architecture doc §7.1): the exact accessor for the expected
/// `(ZomeIndex, EntryDefIndex)` of a unit entry type is unconfirmed against
/// `hdi 0.8` — verify `ScopedEntryDefIndex::try_from(UnitEntryTypes::_)` and the
/// `entry_type()` shape with a Sweettest before relying on this.
pub fn is_create_of(action: &Action, unit: UnitEntryTypes) -> bool {
    if !matches!(action.action_type(), ActionType::Create) {
        return false;
    }
    let Some(EntryType::App(app_entry_def)) = action.entry_type() else {
        return false;
    };
    let Ok(scoped) = ScopedEntryDefIndex::try_from(unit) else {
        return false;
    };
    app_entry_def.zome_index == scoped.zome_index && app_entry_def.entry_index == scoped.zome_type
}

pub fn is_nft_manifest_create(action: &Action) -> bool {
    is_create_of(action, UnitEntryTypes::NftManifest)
}
