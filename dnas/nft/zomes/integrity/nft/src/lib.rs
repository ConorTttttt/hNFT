pub mod dna_properties;
pub mod links;
pub mod nft_manifest;
pub mod transfer;

pub use dna_properties::*;
pub use links::*;
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
    }
}

/// The "new entry stored by an Update action" op. Only NftManifest may be updated.
fn validate_stored_update_entry(app_entry: &EntryTypes) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::NftManifest(_) => Ok(ValidateCallbackResult::Valid),
        _ => Ok(ValidateCallbackResult::Invalid(IMMUTABLE.into())),
    }
}

fn validate_register_update(
    app_entry: EntryTypes,
    action: TypedAction<UpdateData>,
) -> ExternResult<ValidateCallbackResult> {
    let EntryTypes::NftManifest(nft) = app_entry else {
        return Ok(ValidateCallbackResult::Invalid(IMMUTABLE.into()));
    };
    let original_record = must_get_valid_record(action.data.original_action_address.clone())?;
    let original_action =
        TypedAction::<EntryCreationData>::try_from_action(original_record.action().clone())?;
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
        FlatOp::Delete(OpDelete { .. }) => {
            Ok(ValidateCallbackResult::Invalid(NOT_DELETABLE.into()))
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
            OpRecord::DeleteEntry { .. } => {
                Ok(ValidateCallbackResult::Invalid(NOT_DELETABLE.into()))
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
