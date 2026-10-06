use hdk::prelude::*;
use social_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateCommentInput {
    pub target: AnyLinkableHash,
    #[serde(default)]
    pub target_role: String,
    pub body: String,
}

#[hdk_extern]
pub fn create_comment(input: CreateCommentInput) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;
    let comment = Comment {
        target: input.target.clone(),
        target_role: input.target_role,
        body: input.body,
    };
    let comment_hash = create_entry(&EntryTypes::Comment(comment))?;
    create_link(
        input.target,
        comment_hash.clone(),
        LinkTypes::TargetToComment,
        (),
    )?;
    create_link(me, comment_hash.clone(), LinkTypes::AgentToComment, ())?;
    get(comment_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created Comment not found".into()
    )))
}

/// Only the comment's author may delete it (enforced in `validate()`).
#[hdk_extern]
pub fn delete_comment(comment: ActionHash) -> ExternResult<()> {
    delete_entry(comment)?;
    Ok(())
}

/// Direct children of `target` — top-level comments on an NFT, or replies to
/// a specific comment when `target` is itself a comment's action hash.
#[hdk_extern]
pub fn comments_for(target: AnyLinkableHash) -> ExternResult<Vec<Record>> {
    let links = get_links(
        LinkQuery::try_new(target, LinkTypes::TargetToComment)?,
        GetStrategy::Network,
    )?;
    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;
    Ok(records.into_iter().flatten().collect())
}

#[hdk_extern]
pub fn my_comments(_: ()) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToComment)?,
        GetStrategy::Local,
    )?;
    let get_inputs: Vec<GetInput> = links
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .map(|h| GetInput::new(h.into(), GetOptions::default()))
        .collect();
    let records = HDK.with(|hdk| hdk.borrow().get(get_inputs))?;
    Ok(records.into_iter().flatten().collect())
}
