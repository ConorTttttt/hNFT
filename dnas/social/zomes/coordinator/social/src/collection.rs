use hdk::prelude::*;
use social_integrity::*;

const PATH: &str = "collections.all";

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateCollectionInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[hdk_extern]
pub fn create_collection(input: CreateCollectionInput) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;
    let collection = Collection {
        name: input.name,
        description: input.description,
    };
    let hash = create_entry(&EntryTypes::Collection(collection))?;

    create_link(me, hash.clone(), LinkTypes::AgentToCollection, ())?;
    let path = Path::from(PATH);
    create_link(path.path_entry_hash()?, hash.clone(), LinkTypes::PathToCollection, ())?;

    get(hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created Collection not found".into()
    )))
}

/// Latest version of a collection (walks `CollectionUpdates`), same shape as
/// `nft`'s `get_nft`.
#[hdk_extern]
pub fn get_collection(collection: ActionHash) -> ExternResult<Option<Record>> {
    let links = get_links(
        LinkQuery::try_new(collection.clone(), LinkTypes::CollectionUpdates)?,
        GetStrategy::Network,
    )?;
    let latest = links
        .into_iter()
        .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
        .and_then(|l| l.target.into_action_hash())
        .unwrap_or(collection);
    get(latest, GetOptions::default())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateCollectionInput {
    /// Head of this curator's update chain for the collection — the create
    /// action for a first edit, or the previous update's action thereafter.
    pub previous_action_hash: ActionHash,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[hdk_extern]
pub fn update_collection(input: UpdateCollectionInput) -> ExternResult<Record> {
    let collection = Collection {
        name: input.name,
        description: input.description,
    };
    let updated_hash = update_entry(input.previous_action_hash.clone(), &collection)?;
    create_link(
        input.previous_action_hash,
        updated_hash.clone(),
        LinkTypes::CollectionUpdates,
        (),
    )?;
    get(updated_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "updated Collection not found".into()
    )))
}

/// Deletes the collection entry (author-only, enforced in `validate()`). Item
/// links are left in place — permanent, same convention as elsewhere in this
/// zome; a collection that fails to resolve is filtered client-side.
#[hdk_extern]
pub fn delete_collection(collection: ActionHash) -> ExternResult<()> {
    delete_entry(collection)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddToCollectionInput {
    pub collection: ActionHash,
    pub target: AnyLinkableHash,
    #[serde(default)]
    pub target_role: String,
}

/// Only the curator may add items (enforced in `validate()`).
#[hdk_extern]
pub fn add_to_collection(input: AddToCollectionInput) -> ExternResult<()> {
    let tag = LinkTag::new(input.target_role.into_bytes());
    create_link(input.collection, input.target, LinkTypes::CollectionToItem, tag)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoveFromCollectionInput {
    pub collection: ActionHash,
    pub target: AnyLinkableHash,
}

/// Only the curator who added the item may remove it (enforced in
/// `validate()`).
#[hdk_extern]
pub fn remove_from_collection(input: RemoveFromCollectionInput) -> ExternResult<()> {
    let links = get_links(
        LinkQuery::try_new(input.collection, LinkTypes::CollectionToItem)?,
        GetStrategy::Local,
    )?;
    for link in links {
        if link.target == input.target {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CollectionItemView {
    pub target: AnyLinkableHash,
    pub target_role: String,
}

#[hdk_extern]
pub fn collection_items(collection: ActionHash) -> ExternResult<Vec<CollectionItemView>> {
    let links = get_links(
        LinkQuery::try_new(collection, LinkTypes::CollectionToItem)?,
        GetStrategy::Network,
    )?;
    Ok(links
        .into_iter()
        .map(|l| CollectionItemView {
            target: l.target,
            target_role: String::from_utf8_lossy(&l.tag.0).into_owned(),
        })
        .collect())
}

#[hdk_extern]
pub fn my_collections(_: ()) -> ExternResult<Vec<Record>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToCollection)?,
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

/// Every collection in the directory, regardless of curator — same shape as
/// `nft`'s `get_all_nfts`.
#[hdk_extern]
pub fn all_collections(_: ()) -> ExternResult<Vec<Record>> {
    let path = Path::from(PATH);
    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::PathToCollection)?,
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
