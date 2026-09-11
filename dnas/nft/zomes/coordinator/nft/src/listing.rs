use crate::query::current_owner;
use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

const LISTINGS_PATH: &str = "collection.listings";

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateListingInput {
    pub nft: ActionHash,
    pub price: u64,
    #[serde(default)]
    pub currency: String,
}

#[hdk_extern]
pub fn create_listing(input: CreateListingInput) -> ExternResult<Record> {
    let me = agent_info()?.agent_initial_pubkey;

    // Authoritative ownership check — same technique `initiate_transfer`
    // already uses (validate() can't walk the provenance chain).
    if current_owner(input.nft.clone())? != me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "only the current owner may list this NFT".into()
        )));
    }

    let listing = Listing { nft: input.nft.clone(), price: input.price, currency: input.currency };
    let hash = create_entry(&EntryTypes::Listing(listing))?;

    create_link(input.nft, hash.clone(), LinkTypes::NftToListing, ())?;
    create_link(me, hash.clone(), LinkTypes::AgentToListing, ())?;
    let path = Path::from(LISTINGS_PATH);
    create_link(path.path_entry_hash()?, hash.clone(), LinkTypes::PathToListing, ())?;

    get(hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "newly created Listing not found".into()
    )))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateListingInput {
    /// Head of this lister's update chain for the listing — the create
    /// action for a first edit, or the previous update's action thereafter.
    pub previous_action_hash: ActionHash,
    pub price: u64,
    #[serde(default)]
    pub currency: String,
}

#[hdk_extern]
pub fn update_listing_price(input: UpdateListingInput) -> ExternResult<Record> {
    // nft carries over unchanged; fetch it from the current head.
    let current = get(input.previous_action_hash.clone(), GetOptions::default())?.ok_or(
        wasm_error!(WasmErrorInner::Guest("listing not found".into())),
    )?;
    let existing: Listing = current
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not a Listing".into()
        )))?;

    let listing = Listing { nft: existing.nft, price: input.price, currency: input.currency };
    let updated_hash = update_entry(input.previous_action_hash.clone(), &listing)?;
    create_link(
        input.previous_action_hash,
        updated_hash.clone(),
        LinkTypes::ListingUpdates,
        (),
    )?;
    get(updated_hash, GetOptions::default())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "updated Listing not found".into()
    )))
}

/// Cancel a listing (author-only, enforced in `validate()`). Deletes the
/// *current head* of the update chain, not necessarily `listing` itself
/// (its original create) — a price-edited listing's original action has a
/// live Update pointing past it, which `resolve_latest`/`active_listing_view`
/// would still happily resolve to if only the original were deleted.
#[hdk_extern]
pub fn cancel_listing(listing: ActionHash) -> ExternResult<()> {
    let head = resolve_latest(listing.clone())?.ok_or(wasm_error!(WasmErrorInner::Guest(
        "listing not found".into()
    )))?;
    delete_entry(head.action_address().clone())?;
    Ok(())
}

/// Latest version of a listing (walks `ListingUpdates`), mirrors `get_nft`.
#[hdk_extern]
pub fn get_listing(listing: ActionHash) -> ExternResult<Option<Record>> {
    resolve_latest(listing)
}

fn resolve_latest(listing: ActionHash) -> ExternResult<Option<Record>> {
    let links = get_links(
        LinkQuery::try_new(listing.clone(), LinkTypes::ListingUpdates)?,
        GetStrategy::Network,
    )?;
    let latest = links
        .into_iter()
        .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
        .and_then(|l| l.target.into_action_hash())
        .unwrap_or(listing);
    get(latest, GetOptions::default())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ListingView {
    pub listing: ActionHash,
    pub nft: ActionHash,
    pub price: u64,
    pub currency: String,
    pub seller: AgentPubKey,
}

/// Resolves `listing` to its latest content and confirms it's still active:
/// not cancelled, and its author still owns the NFT (a listing goes stale if
/// the owner sells another way — this is where that gets filtered out,
/// same "don't trust a possibly-stale index" posture as `initiate_transfer`'s
/// own ownership check).
fn active_listing_view(listing: ActionHash) -> ExternResult<Option<ListingView>> {
    let Some(record) = resolve_latest(listing.clone())? else {
        return Ok(None); // cancelled (deleted) or unresolvable
    };
    let Some(entry): Option<Listing> = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
    else {
        return Ok(None);
    };
    // The listing's identity (`listing`, the original create) is authored by
    // the lister, regardless of how many price-edit Updates followed. Once
    // there's an Update, plain `get()` on the superseded original can come
    // back empty (the same "latest wins" masking that makes a deleted
    // entry's `get()` resolve to nothing) — `must_get_action` sidesteps that,
    // returning the original action unconditionally.
    let original = must_get_action(listing.clone())?;
    let seller = original.action().author().clone();
    if current_owner(entry.nft.clone())? != seller {
        return Ok(None); // stale — sold another way since listing
    }
    Ok(Some(ListingView {
        listing,
        nft: entry.nft,
        price: entry.price,
        currency: entry.currency,
        seller,
    }))
}

/// The active listing for an NFT, if any.
#[hdk_extern]
pub fn listing_for_nft(nft: ActionHash) -> ExternResult<Option<ListingView>> {
    let links = get_links(
        LinkQuery::try_new(nft, LinkTypes::NftToListing)?,
        GetStrategy::Network,
    )?;
    let Some(most_recent) = links
        .into_iter()
        .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
        .and_then(|l| l.target.into_action_hash())
    else {
        return Ok(None);
    };
    active_listing_view(most_recent)
}

/// Every active listing in the collection, regardless of lister — same shape
/// as `get_all_nfts`.
#[hdk_extern]
pub fn all_listings(_: ()) -> ExternResult<Vec<ListingView>> {
    let path = Path::from(LISTINGS_PATH);
    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::PathToListing)?,
        GetStrategy::Network,
    )?;
    let mut out = Vec::new();
    for link in links {
        let Some(hash) = link.target.into_action_hash() else { continue };
        if let Some(view) = active_listing_view(hash)? {
            out.push(view);
        }
    }
    Ok(out)
}

#[hdk_extern]
pub fn my_listings(_: ()) -> ExternResult<Vec<ListingView>> {
    let me = agent_info()?.agent_initial_pubkey;
    let links = get_links(
        LinkQuery::try_new(me, LinkTypes::AgentToListing)?,
        GetStrategy::Local,
    )?;
    let mut out = Vec::new();
    for link in links {
        let Some(hash) = link.target.into_action_hash() else { continue };
        if let Some(view) = active_listing_view(hash)? {
            out.push(view);
        }
    }
    Ok(out)
}

/// Signal interest in a listing: records both index directions and nudges
/// the seller. Doesn't move the NFT — the seller completes a sale by calling
/// the existing `initiate_transfer` at whichever interested buyer they pick.
#[hdk_extern]
pub fn express_interest(listing: ActionHash) -> ExternResult<()> {
    let me = agent_info()?.agent_initial_pubkey;
    let record = get(listing.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("listing not found".into())
    ))?;
    let seller = record.action().author().clone();
    let entry: Listing = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not a Listing".into()
        )))?;

    create_link(me.clone(), listing.clone(), LinkTypes::AgentToInterest, ())?;
    create_link(listing.clone(), me.clone(), LinkTypes::ListingToInterested, ())?;
    let _ = send_remote_signal(
        RemoteSignal::ListingInterest { listing, nft: entry.nft, buyer: me },
        vec![seller],
    );
    Ok(())
}

/// Seller-facing: who has expressed interest in this listing.
#[hdk_extern]
pub fn interested_buyers(listing: ActionHash) -> ExternResult<Vec<AgentPubKey>> {
    let links = get_links(
        LinkQuery::try_new(listing, LinkTypes::ListingToInterested)?,
        GetStrategy::Network,
    )?;
    Ok(links
        .into_iter()
        .filter_map(|l| l.target.into_agent_pub_key())
        .collect())
}

