pub mod collection;
pub mod comment;
pub mod follow;
pub mod like;
pub mod nft_collection_registry;

pub use collection::*;
pub use comment::*;
pub use follow::*;
pub use like::*;
pub use nft_collection_registry::*;

use hdi::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    Comment(Comment),
    Collection(Collection),
    NftCollectionRegistration(NftCollectionRegistration),
}

#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    /// follower AgentPubKey -> followee AgentPubKey.
    AgentToFollowing,
    /// followee AgentPubKey -> follower AgentPubKey. Reverse index.
    AgentToFollowers,
    /// liker AgentPubKey -> liked AnyLinkableHash. Tag = target_role.
    AgentToLiked,
    /// liked AnyLinkableHash -> liker AgentPubKey. Reverse index.
    TargetToLikers,
    /// NFT or parent Comment AnyLinkableHash -> Comment action hash.
    TargetToComment,
    /// author AgentPubKey -> Comment action hash. "My comments".
    AgentToComment,
    /// curator AgentPubKey -> Collection action hash. "My collections".
    AgentToCollection,
    /// `Path("collections.all")` -> Collection action hash. Global directory.
    PathToCollection,
    /// Collection original Create action -> Update action. Resolves "latest".
    CollectionUpdates,
    /// Collection action hash -> item AnyLinkableHash. Tag = target_role.
    CollectionToItem,
    /// registrant AgentPubKey -> NftCollectionRegistration action hash. "My registrations".
    AgentToNftCollectionRegistration,
    /// `Path("nft-collections.all")` -> NftCollectionRegistration action hash.
    /// The network-wide discovery directory (Phase J.3).
    PathToNftCollectionRegistration,
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

fn validate_create_entry(
    app_entry: EntryTypes,
    action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::Comment(e) => validate_create_comment(action, e),
        EntryTypes::Collection(e) => validate_create_collection(action, e),
        EntryTypes::NftCollectionRegistration(e) => {
            validate_create_nft_collection_registration(action, e)
        }
    }
}

/// Only `Collection` may be updated (curator editing name/description);
/// `Comment` and `NftCollectionRegistration` are immutable.
fn validate_stored_update_entry(app_entry: &EntryTypes) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::Collection(_) => Ok(ValidateCallbackResult::Valid),
        EntryTypes::Comment(_) => Ok(ValidateCallbackResult::Invalid(
            "comments are immutable".into(),
        )),
        EntryTypes::NftCollectionRegistration(_) => Ok(ValidateCallbackResult::Invalid(
            "collection registrations are immutable".into(),
        )),
    }
}

fn validate_register_update(
    app_entry: EntryTypes,
    action: TypedAction<UpdateData>,
) -> ExternResult<ValidateCallbackResult> {
    let EntryTypes::Collection(collection) = app_entry else {
        return Ok(ValidateCallbackResult::Invalid(
            "only a Collection may be updated".into(),
        ));
    };
    let original_record = must_get_valid_record(action.data.original_action_address.clone())?;
    let original_action =
        TypedAction::<EntryCreationData>::try_from_action(original_record.action().clone())?;
    validate_update_collection(action, collection, original_action)
}

/// `EntryTypes` (the `#[hdk_entry_types]` enum) doesn't implement
/// `TryFrom<SerializedBytes>` itself — only the individual structs do, via
/// `#[hdk_entry_helper]` — so a fetched record of unknown-which-variant type
/// is decoded by trying each in turn. `Comment` and `Collection` have
/// distinct field counts, so a shape mismatch is expected and just means
/// "try the next type" — `to_app_option` returns `Err` (not `Ok(None)`) on a
/// shape mismatch, so each attempt's error is swallowed, not propagated.
fn decode_known_entry(record: &Record) -> ExternResult<EntryTypes> {
    if let Ok(Some(comment)) = record.entry().to_app_option::<Comment>() {
        return Ok(EntryTypes::Comment(comment));
    }
    if let Ok(Some(collection)) = record.entry().to_app_option::<Collection>() {
        return Ok(EntryTypes::Collection(collection));
    }
    if let Ok(Some(reg)) = record.entry().to_app_option::<NftCollectionRegistration>() {
        return Ok(EntryTypes::NftCollectionRegistration(reg));
    }
    Err(wasm_error!(WasmErrorInner::Guest(
        "deleted record is neither a Comment, a Collection, nor an NftCollectionRegistration"
            .into()
    )))
}

/// `Comment`, `Collection` and `NftCollectionRegistration` are all
/// deletable, author-only; nothing else in this zome supports delete.
fn validate_register_delete(
    action: TypedAction<DeleteData>,
    original_action: TypedAction<EntryCreationData>,
    original_entry: EntryTypes,
) -> ExternResult<ValidateCallbackResult> {
    match original_entry {
        EntryTypes::Comment(_) => validate_delete_comment(action, original_action),
        EntryTypes::Collection(_) => validate_delete_collection(action, original_action),
        EntryTypes::NftCollectionRegistration(_) => {
            validate_delete_nft_collection_registration(action, original_action)
        }
    }
}

fn validate_create_link(
    link_type: LinkTypes,
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::AgentToFollowing => validate_create_link_agent_to_following(action),
        LinkTypes::AgentToFollowers => validate_create_link_agent_to_followers(action),
        LinkTypes::AgentToLiked => validate_create_link_agent_to_liked(action),
        LinkTypes::TargetToLikers => validate_create_link_target_to_likers(action),
        LinkTypes::TargetToComment | LinkTypes::AgentToComment => {
            validate_create_link_to_comment(action)
        }
        LinkTypes::AgentToCollection | LinkTypes::PathToCollection => {
            validate_create_link_to_collection(action)
        }
        LinkTypes::CollectionUpdates => validate_create_link_collection_updates(action),
        LinkTypes::CollectionToItem => validate_create_link_collection_to_item(action),
        LinkTypes::AgentToNftCollectionRegistration
        | LinkTypes::PathToNftCollectionRegistration => {
            validate_create_link_to_nft_collection_registration(action)
        }
    }
}

fn validate_delete_link(
    link_type: LinkTypes,
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    match link_type {
        LinkTypes::AgentToFollowing | LinkTypes::AgentToFollowers => {
            validate_delete_link_follow(action, original_action)
        }
        LinkTypes::AgentToLiked | LinkTypes::TargetToLikers => {
            validate_delete_link_like(action, original_action)
        }
        LinkTypes::TargetToComment | LinkTypes::AgentToComment => {
            validate_delete_link_to_comment(action, original_action)
        }
        LinkTypes::AgentToCollection | LinkTypes::PathToCollection => {
            validate_delete_link_to_collection(action, original_action)
        }
        LinkTypes::CollectionUpdates => {
            validate_delete_link_collection_updates(action, original_action)
        }
        LinkTypes::CollectionToItem => {
            validate_delete_link_collection_to_item(action, original_action)
        }
        LinkTypes::AgentToNftCollectionRegistration
        | LinkTypes::PathToNftCollectionRegistration => {
            validate_delete_link_to_nft_collection_registration(action, original_action)
        }
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
            let original_record = must_get_valid_record(action.data.deletes_address.clone())?;
            let original_action = TypedAction::<EntryCreationData>::try_from_action(
                original_record.action().clone(),
            )?;
            let original_entry = decode_known_entry(&original_record)?;
            validate_register_delete(action, original_action, original_entry)
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
                let original_record = must_get_valid_record(action.data.deletes_address.clone())?;
                let original_action = TypedAction::<EntryCreationData>::try_from_action(
                    original_record.action().clone(),
                )?;
                let original_entry = decode_known_entry(&original_record)?;
                validate_register_delete(action, original_action, original_entry)
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
                        membrane_proof,
                        ..
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
