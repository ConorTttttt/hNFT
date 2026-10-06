//! Phase J.3 — network-wide NFT-collection discovery.
//!
//! Lives in the `social` DNA deliberately: `social` is this hApp's ONE
//! global, non-cloned cell (`workdir/happ.yaml`: `network_seed: null`, no
//! `clone_limit`) — the only DHT every agent shares in common BEFORE they
//! know about any particular `nft` collection clone. A registration authored
//! *inside* an `nft` clone's own DHT would only ever be visible to peers who
//! have *already* joined that exact clone — which is exactly the problem
//! discovery needs to solve, so it cannot live there. See the J.3 report for
//! the full audit.
//!
//! What this proves and what it doesn't:
//!   - `dna_hash` is the collection's authoritative network identity: the
//!     `nft` clone's own `DnaHash`, taken directly from the registering
//!     agent's local conductor state (`ClonedCell.cell_id[0]` via
//!     `appInfo()`) — never independently re-derived by THIS validator,
//!     because `social`'s validators generally do not have that `nft` clone
//!     installed and cannot compute or check its hash.
//!   - `network_seed` + `properties_json` are copied verbatim from that same
//!     clone's live `dna_modifiers`, because Holochain has no "join an
//!     existing DnaHash directly" mechanism — a discovering peer must supply
//!     byte-identical modifiers to `createCloneCell` to reconstruct the SAME
//!     clone. This is not a second mutable source of truth: `CollectionProperties`
//!     is already immutable for a DNA's lifetime (see the J.1/J.2 reports), so
//!     a frozen copy of it can never drift from the live original.
//!   - Authorization is `action.author()` ONLY — "the agent who published
//!     this announcement", not cryptographic proof that agent is the `nft`
//!     clone's `minter_pubkey`/true creator. `social`'s validators cannot
//!     verify a cross-DNA fact about a clone they don't have (see the J.3
//!     report §"authorization boundary"). This must not be presented in the
//!     UI as a verified-creator guarantee.
//!   - Two different agents CAN both register the same `dna_hash`; nothing
//!     here can stop that (no shared counter/lock across independent
//!     authors' entries — the same limitation this codebase has documented
//!     for every other cross-agent aggregate case). Resolving that into one
//!     canonical listing per `dna_hash` is the discovery QUERY's job
//!     (deterministic tie-break, not a validation rule) — see
//!     `discover_nft_collections` in the coordinator zome.

use hdi::prelude::*;

/// Generous but bounded — `properties_json` carries a full `CollectionProperties`
/// blob (including, for a generative collection, its whole trait catalogue).
const MAX_PROPERTIES_JSON_BYTES: usize = 16 * 1024;
const MAX_NETWORK_SEED_BYTES: usize = 512;

/// An immutable announcement: "the signing agent publishes this `nft` clone
/// as a discoverable collection". See this module's doc comment for exactly
/// what is and isn't proven by its existence.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct NftCollectionRegistration {
    /// The `nft` clone's own `DnaHash` — see the module doc.
    pub dna_hash: DnaHash,
    /// Verbatim `dna_modifiers.network_seed` from that clone.
    pub network_seed: String,
    /// Verbatim JSON of that clone's `CollectionProperties` (as read live
    /// from the registering agent's own conductor, decoded from
    /// `dna_modifiers.properties`). Opaque to this zome — `nft`-shaped
    /// validation of its *contents* is `nft_integrity`'s job, not this one's
    /// (this DHT never installs or validates the `nft` DNA itself).
    pub properties_json: String,
}

fn check_fields(entry: &NftCollectionRegistration) -> ExternResult<ValidateCallbackResult> {
    if entry.network_seed.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "network_seed must not be empty".into(),
        ));
    }
    if entry.network_seed.len() > MAX_NETWORK_SEED_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "network_seed exceeds {MAX_NETWORK_SEED_BYTES} bytes"
        )));
    }
    if entry.properties_json.is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "properties_json must not be empty".into(),
        ));
    }
    if entry.properties_json.len() > MAX_PROPERTIES_JSON_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "properties_json exceeds {MAX_PROPERTIES_JSON_BYTES} bytes"
        )));
    }
    // A minimal, cheap sanity check only — this is NOT `nft`-schema
    // validation (this zome has no business knowing `CollectionProperties`'
    // internal shape; see the module doc). Just reject obvious garbage.
    let trimmed = entry.properties_json.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Ok(ValidateCallbackResult::Invalid(
            "properties_json does not look like a JSON object".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_nft_collection_registration(
    _action: TypedAction<EntryCreationData>,
    entry: NftCollectionRegistration,
) -> ExternResult<ValidateCallbackResult> {
    check_fields(&entry)
}

/// Immutable once created — there is nothing meaningful to "edit" about an
/// existing registration (its `dna_hash` IS the collection; different
/// properties would mean a different DnaHash, i.e. a different collection,
/// i.e. a NEW registration, not an edit of this one). Only delete
/// (unregister) is supported; see `validate_delete_nft_collection_registration`.
pub fn validate_update_nft_collection_registration(
    _action: TypedAction<UpdateData>,
    _entry: NftCollectionRegistration,
    _original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "collection registrations are immutable — unregister and publish a new one instead".into(),
    ))
}

/// Only the original registrant may unregister (withdraw) their own
/// announcement. This does not and cannot prove they were the collection's
/// true creator — see the module doc's authorization-boundary note.
pub fn validate_delete_nft_collection_registration(
    action: TypedAction<DeleteData>,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the registrant may unregister their own collection announcement".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn target_registration_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .target_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link target is not an ActionHash".into()
        )))
}

fn decode_registration(record: &Record) -> ExternResult<NftCollectionRegistration> {
    record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not an NftCollectionRegistration".into()
        )))
}

/// `AgentToNftCollectionRegistration` ("my registrations") and
/// `PathToNftCollectionRegistration` (the global discovery directory) both
/// must point at a real `NftCollectionRegistration`. Permanent — same
/// convention as `social`'s existing `PathToCollection`.
pub fn validate_create_link_to_nft_collection_registration(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_registration_hash(&action)?;
    let record = must_get_valid_record(target)?;
    let _reg = decode_registration(&record)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_to_nft_collection_registration(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "collection discovery links are permanent".into(),
    ))
}
