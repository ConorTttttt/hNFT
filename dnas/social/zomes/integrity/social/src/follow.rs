use hdi::prelude::*;

/// `AgentToFollowing` / `AgentToFollowers` — pure relationships, no backing entry.
/// A follow is just the link existing; unfollow is deleting it. Both link types
/// are created together by the follower's own `follow()` call (see the
/// coordinator zome), so both are validated identically: only the follower may
/// author (create or delete) either side.

pub fn validate_create_link_agent_to_following(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let follower = base_agent(&action)?;
    let followee = target_agent(&action)?;
    if follower != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the follower may author their own following edge".into(),
        ));
    }
    if follower == followee {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot follow yourself".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_link_agent_to_followers(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    // Base is the *followee* here, not the author — the follower authors this
    // reverse-index entry on the followee's behalf, same as `nft`'s
    // `AgentToNft` index isn't authored by the agent it's keyed on.
    let followee = base_agent(&action)?;
    let follower = target_agent(&action)?;
    if follower != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the follower may author their own follower-index entry".into(),
        ));
    }
    if follower == followee {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot follow yourself".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Unfollow: only the agent who authored the original `CreateLink` — the
/// follower, on both link types — may delete it. Same "original author only"
/// technique as `nft`'s `AgentToNft` / `AgentToInboundCommitment` deletes.
pub fn validate_delete_link_follow(
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the original follower may unfollow".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn base_agent(action: &TypedAction<CreateLinkData>) -> ExternResult<AgentPubKey> {
    AgentPubKey::try_from(action.data.base_address.clone()).map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link base is not an AgentPubKey: {e:?}"
        )))
    })
}

fn target_agent(action: &TypedAction<CreateLinkData>) -> ExternResult<AgentPubKey> {
    AgentPubKey::try_from(action.data.target_address.clone()).map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "link target is not an AgentPubKey: {e:?}"
        )))
    })
}
