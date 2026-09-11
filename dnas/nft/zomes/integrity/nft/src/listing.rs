use hdi::prelude::*;

/// A public, editable ask: the current owner offering an NFT for sale at
/// `price` (a coordination signal only — no native currency on Holochain, no
/// escrow; same unenforced-until-later stance as `MintAgreement.payment_ref`
/// and `CollectionProperties.royalty_bps`). Buying doesn't happen through
/// this entry — a buyer `express_interest`s, and the seller completes the
/// sale with the existing `initiate_transfer` flow.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct Listing {
    /// The NFT this offers (its mint Create action).
    pub nft: ActionHash,
    pub price: u64,
    #[serde(default)]
    pub currency: String,
}

const MAX_CURRENCY_BYTES: usize = 20;

pub fn validate_create_listing(
    _action: TypedAction<EntryCreationData>,
    entry: Listing,
) -> ExternResult<ValidateCallbackResult> {
    must_get_action(entry.nft)?; // the NFT is real
    if entry.price == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "listing price must be greater than zero".into(),
        ));
    }
    if entry.currency.len() > MAX_CURRENCY_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "currency exceeds {MAX_CURRENCY_BYTES} bytes"
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Only the lister may edit their own listing (change price/currency).
/// "Only the current owner" isn't checked here — ownership requires walking
/// the provenance link chain, which `validate()` can't do; it's an
/// authoritative check in the coordinator instead (`create_listing`, same as
/// `initiate_transfer` already does for offers).
pub fn validate_update_listing(
    action: TypedAction<UpdateData>,
    entry: Listing,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the lister may edit their listing".into(),
        ));
    }
    if entry.price == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "listing price must be greater than zero".into(),
        ));
    }
    if entry.currency.len() > MAX_CURRENCY_BYTES {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "currency exceeds {MAX_CURRENCY_BYTES} bytes"
        )));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Only the lister may cancel their own listing.
pub fn validate_delete_listing(
    action: TypedAction<DeleteData>,
    original_action: TypedAction<EntryCreationData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the lister may cancel their listing".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// --- links -------------------------------------------------------------

fn target_action_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .target_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link target is not an ActionHash".into()
        )))
}

fn base_action_hash(action: &TypedAction<CreateLinkData>) -> ExternResult<ActionHash> {
    action
        .data
        .base_address
        .clone()
        .into_action_hash()
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "link base is not an ActionHash".into()
        )))
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

fn load_listing(hash: ActionHash) -> ExternResult<(Record, Listing)> {
    let record = must_get_valid_record(hash)?;
    let listing: Listing = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "target is not a Listing".into()
        )))?;
    Ok((record, listing))
}

/// `PathToListing` / `AgentToListing`: target must be a real `Listing`.
/// Permanent (discovery links, same convention as `PathToNft`).
pub fn validate_create_link_to_listing(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_action_hash(&action)?;
    let _ = load_listing(target)?;
    Ok(ValidateCallbackResult::Valid)
}

/// `NftToListing`: base is the NFT, target must be a real `Listing` for it.
pub fn validate_create_link_nft_to_listing(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let nft = base_action_hash(&action)?;
    let target = target_action_hash(&action)?;
    let (_, listing) = load_listing(target)?;
    if listing.nft != nft {
        return Ok(ValidateCallbackResult::Invalid(
            "NftToListing base does not match the Listing's nft".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// `ListingUpdates` — base = Listing's original Create, target = an Update
/// action. Same shape as `NftUpdates`.
pub fn validate_create_link_listing_updates(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let base = base_action_hash(&action)?;
    let (base_record, _) = load_listing(base)?;
    if base_record.action().author() != action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the lister may extend their listing's update chain".into(),
        ));
    }
    let target = target_action_hash(&action)?;
    must_get_valid_record(target)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_permanent(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "listing links are permanent".into(),
    ))
}

/// `AgentToInterest` (base = buyer) / `ListingToInterested` (target = buyer):
/// both created by the buyer expressing interest, same dual-link-by-one-
/// author pattern as `social`'s `AgentToLiked`/`TargetToLikers`.
pub fn validate_create_link_agent_to_interest(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let buyer = base_agent(&action)?;
    if buyer != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the interested agent may author their own interest entry".into(),
        ));
    }
    let target = target_action_hash(&action)?;
    let _ = load_listing(target)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_link_listing_to_interested(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let listing = base_action_hash(&action)?;
    let _ = load_listing(listing)?;
    let buyer = target_agent(&action)?;
    if buyer != *action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the interested agent may author their own entry in a listing's interested index"
                .into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
