use hdi::prelude::*;

const MAX_ROLE_BYTES: usize = 100;

/// `AgentToLiked` / `TargetToLikers` — pure relationships, no backing entry.
/// Both are created together by the liker's own `like()` call (see the
/// coordinator zome): `AgentToLiked` (base = liker) is the "my likes" index,
/// `TargetToLikers` (base = the liked target) is the "who liked this" index.
/// `AgentToLiked`'s tag carries `target_role` — which `nft` clone the liked
/// target lives in — so "my likes" can be resolved across collections.

pub fn validate_create_link_agent_to_liked(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.data.tag.0.len() > MAX_ROLE_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "target_role tag exceeds {MAX_ROLE_BYTES} bytes"
        )));
    }
    let liker = AgentPubKey::try_from(action.data.base_address.clone()).map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link base is not an AgentPubKey: {e:?}"
        )))
    })?;
    if liker != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the liker may author their own liked-index entry".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_link_target_to_likers(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    // Base is the liked target (opaque — may be cross-DNA, not checked here).
    let liker = AgentPubKey::try_from(action.data.target_address.clone()).map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link target is not an AgentPubKey: {e:?}"
        )))
    })?;
    if liker != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the liker may author their own entry in a target's likers index".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Unlike: only the original liker (the author of the `CreateLink`) may
/// delete either side.
pub fn validate_delete_link_like(
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the original liker may unlike".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
