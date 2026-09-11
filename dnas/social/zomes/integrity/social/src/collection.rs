use hdi::prelude::*;

const MAX_NAME_BYTES: usize = 100;
const MAX_DESCRIPTION_BYTES: usize = 1000;
const MAX_ROLE_BYTES: usize = 100;

/// A curated list of items, owned by whoever created it. Membership is link
/// churn (`CollectionToItem`), not part of this entry — adding/removing items
/// never touches the entry itself.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct Collection {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

fn check_fields(entry: &Collection) -> ExternResult<ValidateCallbackResult> {
    if entry.name.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "collection name must not be empty".into(),
        ));
    }
    if entry.name.len() > MAX_NAME_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "collection name exceeds {MAX_NAME_BYTES} bytes"
        )));
    }
    if entry.description.len() > MAX_DESCRIPTION_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "collection description exceeds {MAX_DESCRIPTION_BYTES} bytes"
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_collection(
    _action: TypedAction<EntryCreationData>,
    entry: Collection,
) -> ExternResult<ValidateCallbackResult> {
    check_fields(&entry)
}

/// Only the collection's original curator may edit it.
pub fn validate_update_collection(
    action: TypedAction<UpdateData>,
    entry: Collection,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the curator may edit their collection".into(),
        ));
    }
    check_fields(&entry)
}

/// Only the collection's original curator may delete it.
pub fn validate_delete_collection(
    action: TypedAction<DeleteData>,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the curator may delete their collection".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `CollectionUpdates` — base = the Collection's original Create action,
/// target = an Update action. `get()` on the original hash returns only the
/// original content, so (as with `nft`'s `NftUpdates`) readers need this
/// chain to resolve "latest". Permanent, curator-authored only.
pub fn validate_create_link_collection_updates(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let base = base_collection_hash(&action)?;
    let base_record = load_collection_record(base)?;
    let _collection = decode_collection(&base_record)?;
    if base_record.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the curator may extend their collection's update chain".into(),
        ));
    }
    let target = target_collection_hash(&action)?;
    must_get_valid_record(target)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_collection_updates(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "CollectionUpdates links are permanent".into(),
    ))
}

fn base_collection_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .base_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link base is not an ActionHash".into()
        )))
}

fn target_collection_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .target_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link target is not an ActionHash".into()
        )))
}

fn load_collection_record(hash: ActionHash) -> ExternResult<Record> {
    must_get_valid_record(hash)
}

fn decode_collection(record: &Record) -> ExternResult<Collection> {
    record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not a Collection".into()
        )))
}

/// `AgentToCollection` (discovery: "my collections") and `PathToCollection`
/// (the global directory) both must point at a real `Collection`. Permanent.
pub fn validate_create_link_to_collection(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_collection_hash(&action)?;
    let record = load_collection_record(target)?;
    let _collection = decode_collection(&record)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_to_collection(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "collection discovery links are permanent".into(),
    ))
}

/// `CollectionToItem` — base is the `Collection`'s own create-action hash
/// (its identity), so `must_get_valid_record` on it fetches that exact
/// original create and its author directly: only the curator may add items.
/// The target (an NFT, usually in a different DNA) is opaque and unchecked.
pub fn validate_create_link_collection_to_item(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.data.tag.0.len() > MAX_ROLE_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "target_role tag exceeds {MAX_ROLE_BYTES} bytes"
        )));
    }
    let base = base_collection_hash(&action)?;
    let record = load_collection_record(base)?;
    let _collection = decode_collection(&record)?;
    if record.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the curator may add items to their collection".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Remove-from-collection: only the curator who added the item (the original
/// `CreateLink` author) may remove it.
pub fn validate_delete_link_collection_to_item(
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the curator who added an item may remove it".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
