use hdk::prelude::*;
use social_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct LikeTargetInput {
    pub target: AnyLinkableHash,
    /// The `nft` clone's role name when `target` is an NFT (e.g. `"nft.3"`);
    /// empty when it's some other kind of target.
    #[serde(default)]
    pub target_role: String,
}

fn liked_by_me(target: &AnyLinkableHash, me: &AgentPubKey) -> ExternResult<bool> {
    Ok(get_links(
        LinkQuery::try_new(me.clone(), LinkTypes::AgentToLiked)?,
        GetStrategy::Local,
    )?
    .iter()
    .any(|l| &l.target == target))
}

/// Like a target. Idempotent — a no-op if already liked (soft check;
/// `validate()` can't enforce uniqueness, same as `nft`'s reveal-default
/// gating).
#[hdk_extern]
pub fn like(input: LikeTargetInput) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;
    if liked_by_me(&input.target, &me)? {
        return Ok(());
    }
    let tag = LinkTag::new(input.target_role.into_bytes());
    create_link(me.clone(), input.target.clone(), LinkTypes::AgentToLiked, tag)?;
    create_link(input.target, me, LinkTypes::TargetToLikers, ())?;
    Ok(())
}

/// Unlike: delete both link directions this agent authored for `target`.
#[hdk_extern]
pub fn unlike(target: AnyLinkableHash) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;

    for link in get_links(
        LinkQuery::try_new(me.clone(), LinkTypes::AgentToLiked)?,
        GetStrategy::Local,
    )? {
        if link.target == target {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    for link in get_links(
        LinkQuery::try_new(target, LinkTypes::TargetToLikers)?,
        GetStrategy::Network,
    )? {
        if link.target.into_agent_pub_key().as_ref() == Some(&me) {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LikesView {
    pub count: u32,
    pub liked_by_me: bool,
}

#[hdk_extern]
pub fn likes_for(target: AnyLinkableHash) -> ExternResult<LikesView> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(target, LinkTypes::TargetToLikers)?,
        GetStrategy::Network,
    )?;
    let liked_by_me = links
        .iter()
        .any(|l| l.target.clone().into_agent_pub_key().as_ref() == Some(&me));
    Ok(LikesView {
        count: links.len() as u32,
        liked_by_me,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MyLikedItem {
    pub target: AnyLinkableHash,
    pub target_role: String,
}

/// Every target this agent has liked, across every `nft` collection.
#[hdk_extern]
pub fn my_likes(_: ()) -> ExternResult<Vec<MyLikedItem>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToLiked)?,
        GetStrategy::Local,
    )?;
    Ok(links
        .into_iter()
        .map(|l| MyLikedItem {
            target: l.target,
            target_role: String::from_utf8_lossy(&l.tag.0).into_owned(),
        })
        .collect())
}
