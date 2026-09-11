use hdi::prelude::*;

const MAX_BODY_BYTES: usize = 4000;
const MAX_ROLE_BYTES: usize = 100;

/// A comment on an NFT (or, via `target` pointing at another `Comment`'s
/// action hash, a reply). `target` is polymorphic (`AnyLinkableHash`) because
/// the NFT it names usually lives in a *different* DNA — this zome is a single
/// global cell shared across every `nft` collection clone, so it can't
/// `must_get_*` an NFT target to confirm it exists. `target_role` is the
/// `nft` clone's role name (e.g. `"nft.3"`) when `target` is an NFT, so the
/// UI can route a follow-up lookup to the right cell; it's empty when
/// `target` is a `Comment` in this same DNA.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct Comment {
    pub target: AnyLinkableHash,
    #[serde(default)]
    pub target_role: String,
    pub body: String,
}

pub fn validate_create_comment(
    _action: TypedAction<EntryCreationData>,
    entry: Comment,
) -> ExternResult<ValidateCallbackResult> {
    if entry.body.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "comment body must not be empty".into(),
        ));
    }
    if entry.body.len() > MAX_BODY_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "comment body exceeds {MAX_BODY_BYTES} bytes"
        )));
    }
    if entry.target_role.len() > MAX_ROLE_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "target_role exceeds {MAX_ROLE_BYTES} bytes"
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Only the comment's author may delete it.
pub fn validate_delete_comment(
    action: TypedAction<DeleteData>,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the comment's author may delete it".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn target_comment_action_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .target_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link target is not an ActionHash".into()
        )))
}

fn load_comment(hash: ActionHash) -> ExternResult<Comment> {
    let record = must_get_valid_record(hash)?;
    record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not a Comment".into()
        )))
}

/// `TargetToComment` (base = the NFT or parent Comment) and `AgentToComment`
/// (base = author) both point at a real `Comment` — same-DNA, so this is a
/// real, checkable invariant unlike the cross-DNA `target` field above.
pub fn validate_create_link_to_comment(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_comment_action_hash(&action)?;
    let _comment = load_comment(target)?;
    Ok(ValidateCallbackResult::Valid)
}

/// Permanent — same "links are permanent" convention as `nft`'s agreement
/// links. Deleting the underlying `Comment` is enough; readers filter out
/// links whose target no longer resolves.
pub fn validate_delete_link_to_comment(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "comment discovery links are permanent".into(),
    ))
}
