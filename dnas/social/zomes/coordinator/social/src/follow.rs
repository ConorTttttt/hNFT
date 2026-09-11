use hdk::prelude::*;
use social_integrity::*;

/// Follow an agent: creates both the forward (`AgentToFollowing`) and reverse
/// (`AgentToFollowers`) index links in one call, both authored by the caller.
#[hdk_extern]
pub fn follow(agent: AgentPubKey) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;
    create_link(me.clone(), agent.clone(), LinkTypes::AgentToFollowing, ())?;
    create_link(agent.clone(), me, LinkTypes::AgentToFollowers, ())?;
    let _ = send_remote_signal(crate::RemoteSignal::Followed, vec![agent]);
    Ok(())
}

/// Unfollow: delete both link directions this agent authored for `agent`.
#[hdk_extern]
pub fn unfollow(agent: AgentPubKey) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;

    for link in get_links(
        LinkQuery::try_new(me.clone(), LinkTypes::AgentToFollowing)?,
        GetStrategy::Local,
    )? {
        if link.target.into_agent_pub_key().as_ref() == Some(&agent) {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    for link in get_links(
        LinkQuery::try_new(agent, LinkTypes::AgentToFollowers)?,
        GetStrategy::Network,
    )? {
        if link.target.into_agent_pub_key().as_ref() == Some(&me) {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    Ok(())
}

/// Agents `agent` follows.
#[hdk_extern]
pub fn following(agent: AgentPubKey) -> ExternResult<Vec<AgentPubKey>> {
    let me = agent_info()?.agent_initial_pubkey;
    let strategy = if agent == me {
        GetStrategy::Local
    } else {
        GetStrategy::Network
    };
    let links = get_links(
        LinkQuery::try_new(agent, LinkTypes::AgentToFollowing)?,
        strategy,
    )?;
    Ok(links
        .into_iter()
        .filter_map(|l| l.target.into_agent_pub_key())
        .collect())
}

/// Agents who follow `agent`.
#[hdk_extern]
pub fn followers(agent: AgentPubKey) -> ExternResult<Vec<AgentPubKey>> {
    let me = agent_info()?.agent_initial_pubkey;
    let strategy = if agent == me {
        GetStrategy::Local
    } else {
        GetStrategy::Network
    };
    let links = get_links(
        LinkQuery::try_new(agent, LinkTypes::AgentToFollowers)?,
        strategy,
    )?;
    Ok(links
        .into_iter()
        .filter_map(|l| l.target.into_agent_pub_key())
        .collect())
}

/// Convenience for UI button state: does the caller already follow `agent`?
#[hdk_extern]
pub fn am_i_following(agent: AgentPubKey) -> ExternResult<bool> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToFollowing)?,
        GetStrategy::Local,
    )?;
    Ok(links
        .iter()
        .any(|l| l.target.clone().into_agent_pub_key().as_ref() == Some(&agent)))
}
