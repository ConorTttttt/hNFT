pub mod agreement;
pub mod dna_properties;
pub mod links;
pub mod listing;
pub mod nft_manifest;
pub mod transfer;

pub use agreement::*;
pub use dna_properties::*;
pub use links::*;
pub use listing::*;
pub use nft_manifest::*;
pub use transfer::*;

use hdi::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    NftManifest(NftManifest),
    TransferCommitment(TransferCommitment),
    TransferReceipt(TransferReceipt),
    TransferCancellation(TransferCancellation),
    MintAgreement(MintAgreement),
    RevealDefault(RevealDefault),
    #[entry_type(visibility = "private")]
    RevealSecret(RevealSecret),
    Listing(Listing),
}

#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    /// `Path("collection.all")` -> NftManifest original action. Global discovery.
    PathToNft,
    /// NftManifest original action -> reveal Update action.
    NftUpdates,
    /// NftManifest original action -> TransferReceipt action. Append-only provenance.
    NftToReceipt,
    /// buyer AgentPubKey -> TransferCommitment action. The buyer's offer inbox.
    AgentToInboundCommitment,
    /// owner AgentPubKey -> NftManifest original action. DERIVED ownership index;
    /// LinkTag = authorizing TransferReceipt action hash (or the mint action, seq 0).
    AgentToNft,
    /// NftManifest original action -> MintAgreement action. (0 or 1.)
    NftToAgreement,
    /// NftManifest original action -> RevealDefault action. (0 or 1.)
    NftToRevealDefault,
    /// delegate AgentPubKey -> MintAgreement action. Delegate discovery.
    AgentToDelegatedReveal,
    /// `Path("collection.listings")` -> Listing action hash. Global discovery.
    PathToListing,
    /// NftManifest original action -> Listing action hash. "Is this listed."
    NftToListing,
    /// lister AgentPubKey -> Listing action hash. "My listings".
    AgentToListing,
    /// Listing original Create action -> Update action. Resolves "latest".
    ListingUpdates,
    /// Listing action hash -> interested buyer AgentPubKey. Seller's queue.
    ListingToInterested,
    /// buyer AgentPubKey -> Listing action hash. "Listings I'm interested in".
    AgentToInterest,
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_agent_joining(
    _agent_pub_key: AgentPubKey,
    _membrane_proof: &Option<MembraneProof>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

const IMMUTABLE: &str = "only NftManifest may be updated (reveal); this entry type is immutable";
const NOT_DELETABLE: &str = "entries in this zome are not deletable";

fn validate_create_entry(
    app_entry: EntryTypes,
    action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::NftManifest(e) => validate_create_nft_manifest(action, e),
        EntryTypes::TransferCommitment(e) => validate_create_transfer_commitment(action, e),
        EntryTypes::TransferReceipt(e) => validate_create_transfer_receipt(action, e),
        EntryTypes::TransferCancellation(e) => validate_create_transfer_cancellation(action, e),
        EntryTypes::MintAgreement(e) => validate_create_mint_agreement(action, e),
        EntryTypes::RevealDefault(e) => validate_create_reveal_default(action, e),
        EntryTypes::RevealSecret(e) => validate_create_reveal_secret(action, e),
        EntryTypes::Listing(e) => validate_create_listing(action, e),
    }
}

/// The "new entry stored by an Update action" op. `NftManifest` (reveal) and
/// `Listing` (price edit) may be updated; everything else is immutable.
fn validate_stored_update_entry(app_entry: &EntryTypes) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::NftManifest(_) | EntryTypes::Listing(_) => Ok(ValidateCallbackResult::Valid),
        _ => Ok(ValidateCallbackResult::Invalid(IMMUTABLE.into())),
    }
}

fn validate_register_update(
    app_entry: EntryTypes,
    action: TypedAction<UpdateData>,
) -> ExternResult<ValidateCallbackResult> {
    let original_record = must_get_valid_record(action.data.original_action_address.clone())?;
    let original_action =
        TypedAction::<EntryCreationData>::try_from_action(original_record.action().clone())?;
    match app_entry {
        EntryTypes::NftManifest(nft) => {
            let original_nft = match NftManifest::try_from(original_record) {
                Ok(e) => e,
                Err(e) => {
                    return Ok(ValidateCallbackResult::Invalid(format!(
                        "expected NftManifest as update original: {e:?}"
                    )))
                }
            };
            validate_update_nft_manifest(action, nft, original_action, original_nft)
        }
        EntryTypes::Listing(listing) => validate_update_listing(action, listing, original_action),
        _ => Ok(ValidateCallbackResult::Invalid(IMMUTABLE.into())),
    }
}

/// `EntryTypes` doesn't implement `TryFrom<SerializedBytes>` itself — only the
/// individual structs do (`#[hdk_entry_helper]`) — so a fetched record of
/// unknown-which-variant type (the delete path only has the deleted action's
/// hash) is decoded by trying the deletable variants in turn. `to_app_option`
/// returns `Err` (not `Ok(None)`) on a shape mismatch, so each attempt's
/// error is swallowed, not propagated — the next attempt is the fallback.
fn decode_deletable_entry(record: &Record) -> ExternResult<EntryTypes> {
    if let Ok(Some(listing)) = record.entry().to_app_option::<Listing>() {
        return Ok(EntryTypes::Listing(listing));
    }
    Err(wasm_error!(WasmErrorInner::Guest(
        "deleted record is not a deletable entry type in this zome".into()
    )))
}

fn validate_create_link(
    link_type: LinkTypes,
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::PathToNft => validate_create_link_path_to_nft(action),
        LinkTypes::NftUpdates => validate_create_link_nft_updates(action),
        LinkTypes::NftToReceipt => validate_create_link_nft_to_receipt(action),
        LinkTypes::AgentToInboundCommitment => {
            validate_create_link_agent_to_inbound_commitment(action)
        }
        LinkTypes::AgentToNft => validate_create_link_agent_to_nft(action),
        LinkTypes::NftToAgreement => validate_create_link_nft_to_agreement(action),
        LinkTypes::NftToRevealDefault => validate_create_link_nft_to_reveal_default(action),
        LinkTypes::AgentToDelegatedReveal => {
            validate_create_link_agent_to_delegated_reveal(action)
        }
        LinkTypes::PathToListing | LinkTypes::AgentToListing => {
            validate_create_link_to_listing(action)
        }
        LinkTypes::NftToListing => validate_create_link_nft_to_listing(action),
        LinkTypes::ListingUpdates => validate_create_link_listing_updates(action),
        LinkTypes::AgentToInterest => validate_create_link_agent_to_interest(action),
        LinkTypes::ListingToInterested => validate_create_link_listing_to_interested(action),
    }
}

fn validate_delete_link(
    link_type: LinkTypes,
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::PathToNft => validate_delete_link_path_to_nft(action, original_action),
        LinkTypes::NftUpdates => validate_delete_link_nft_updates(action, original_action),
        LinkTypes::NftToReceipt => validate_delete_link_nft_to_receipt(action, original_action),
        LinkTypes::AgentToInboundCommitment => {
            validate_delete_link_agent_to_inbound_commitment(action, original_action)
        }
        LinkTypes::AgentToNft => validate_delete_link_agent_to_nft(action, original_action),
        LinkTypes::NftToAgreement
        | LinkTypes::NftToRevealDefault
        | LinkTypes::AgentToDelegatedReveal => Ok(ValidateCallbackResult::Invalid(
            "agreement links are permanent".into(),
        )),
        LinkTypes::PathToListing
        | LinkTypes::AgentToListing
        | LinkTypes::NftToListing
        | LinkTypes::ListingUpdates
        | LinkTypes::AgentToInterest
        | LinkTypes::ListingToInterested => validate_delete_link_permanent(action, original_action),
    }
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::CreateEntry(create_entry) => match create_entry {
            OpEntry::CreateEntry { app_entry, action } => {
                validate_create_entry(app_entry, action.into())
            }
            OpEntry::UpdateEntry { app_entry, .. } => validate_stored_update_entry(&app_entry),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::Update(update_entry) => match update_entry {
            OpUpdate::Entry { app_entry, action } => validate_register_update(app_entry, action),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::Delete(OpDelete { action }) => {
            let original_record =
                must_get_valid_record(action.data.deletes_address.clone())?;
            let original_action = TypedAction::<EntryCreationData>::try_from_action(
                original_record.action().clone(),
            )?;
            match decode_deletable_entry(&original_record) {
                Ok(EntryTypes::Listing(_)) => validate_delete_listing(action, original_action),
                Ok(_) => Ok(ValidateCallbackResult::Invalid(NOT_DELETABLE.into())),
                Err(_) => Ok(ValidateCallbackResult::Invalid(NOT_DELETABLE.into())),
            }
        }
        FlatOp::Link(OpLink::CreateLink { link_type, action }) => {
            validate_create_link(link_type, action)
        }
        FlatOp::Link(OpLink::DeleteLink {
            link_type,
            original_action,
            action,
        }) => validate_delete_link(link_type, action, original_action),
        FlatOp::CreateRecord(store_record) => match store_record {
            OpRecord::CreateEntry { app_entry, action } => {
                validate_create_entry(app_entry, action.into())
            }
            OpRecord::UpdateEntry {
                app_entry, action, ..
            } => validate_register_update(app_entry, action),
            OpRecord::DeleteEntry { action, .. } => {
                let original_record =
                    must_get_valid_record(action.data.deletes_address.clone())?;
                let original_action = TypedAction::<EntryCreationData>::try_from_action(
                    original_record.action().clone(),
                )?;
                match decode_deletable_entry(&original_record) {
                    Ok(EntryTypes::Listing(_)) => validate_delete_listing(action, original_action),
                    _ => Ok(ValidateCallbackResult::Invalid(NOT_DELETABLE.into())),
                }
            }
            OpRecord::CreateLink { link_type, action } => validate_create_link(link_type, action),
            OpRecord::DeleteLink { action } => {
                let record = must_get_valid_record(action.link_add_address.clone())?;
                let create_link =
                    TypedAction::<CreateLinkData>::try_from_action(record.action().clone())?;
                let link_type = match LinkTypes::from_type(
                    create_link.data.zome_index,
                    create_link.data.link_type,
                )? {
                    Some(lt) => lt,
                    None => return Ok(ValidateCallbackResult::Valid),
                };
                validate_delete_link(link_type, action, create_link)
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::AgentActivity(agent_activity) => match agent_activity {
            OpActivity::CreateAgent { agent, action } => {
                let prev = action
                    .prev_action()
                    .ok_or_else(|| {
                        wasm_error!(WasmErrorInner::Guest("expected a prior action".into()))
                    })?
                    .clone();
                let previous_action = must_get_action(prev)?;
                match &previous_action.action().data {
                    ActionData::AgentValidationPkg(AgentValidationPkgData {
                        membrane_proof, ..
                    }) => validate_agent_joining(agent, membrane_proof),
                    _ => Ok(ValidateCallbackResult::Invalid(
                        "the previous action for a `CreateAgent` must be an `AgentValidationPkg`"
                            .into(),
                    )),
                }
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
    }
}
