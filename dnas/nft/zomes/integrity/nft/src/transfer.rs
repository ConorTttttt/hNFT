use crate::blind_collection::{validate_batch_release, BatchRelease};
use crate::dna_properties::CollectionProperties;
use crate::nft_manifest::{is_create_of, NftManifest};
use crate::UnitEntryTypes;
use hdi::prelude::*;

/// Authored by the CURRENT OWNER. An open, binding offer to hand `nft` to
/// `to_agent`. One commitment per ownership state per seller (enforced below).
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct TransferCommitment {
    /// `NftManifest` original Create action — the NFT's identity.
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
    /// The ownership state this offer supersedes:
    ///   - `nft` itself for the first transfer (minter -> first owner), or
    ///   - the `ActionHash` of the previously accepted `TransferReceipt`.
    pub prev_state: ActionHash,
    /// `1` for the first transfer, `+1` per hop thereafter.
    pub sequence: u32,
    /// Mandatory for the FIRST transfer of a blind-batch NFT, forbidden
    /// otherwise. Validators reconstruct the assignment from it; see
    /// `blind_collection::validate_batch_release`. `None` for ordinary NFTs.
    #[serde(default)]
    pub batch_release: Option<BatchRelease>,
}

/// Authored by the BUYER named in the commitment. Accepting the offer.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct TransferReceipt {
    pub commitment: ActionHash,
}

/// Authored by the SELLER. Voids an unaccepted commitment so the state can be
/// re-offered. Valid only *after* the transfer window has elapsed.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct TransferCancellation {
    pub commitment: ActionHash,
}

// ---------------------------------------------------------------------------
// Shared loaders
// ---------------------------------------------------------------------------

pub fn load_commitment(hash: &ActionHash) -> ExternResult<(Record, TransferCommitment)> {
    let record = must_get_valid_record(hash.clone())?;
    let commitment: TransferCommitment = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a TransferCommitment".into()
        )))?;
    Ok((record, commitment))
}

pub fn load_receipt(hash: &ActionHash) -> ExternResult<(Record, TransferReceipt)> {
    let record = must_get_valid_record(hash.clone())?;
    let receipt: TransferReceipt = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a TransferReceipt".into()
        )))?;
    Ok((record, receipt))
}

/// `opened_at + window` as microseconds since the epoch. Using raw micros avoids
/// depending on a specific `Timestamp + Duration` impl — verify `Timestamp::as_micros`
/// against `holochain_integrity_types 0.7` (architecture doc §7).
fn window_deadline_micros(opened_at: Timestamp, window_secs: u64) -> i64 {
    opened_at.as_micros() + (window_secs as i64) * 1_000_000
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_create_transfer_commitment(
    action: TypedAction<EntryCreationData>,
    entry: TransferCommitment,
) -> ExternResult<ValidateCallbackResult> {
    let seller = action.author().clone();

    // 1. Seller must currently hold the state `prev_state` names.
    if entry.prev_state == entry.nft {
        // First transfer: prev_state is the mint. Seller must be the minter.
        let mint_rec = must_get_valid_record(entry.nft.clone())?;
        if mint_rec.action().author() != &seller {
            return Ok(ValidateCallbackResult::Invalid(
                "first transfer must be authored by the NFT's minter".into(),
            ));
        }
        if entry.sequence != 1 {
            return Ok(ValidateCallbackResult::Invalid(
                "first transfer must have sequence 1".into(),
            ));
        }

        // -- blind-batch release (G3a) --
        // An NFT minted into a batch may only leave the minter through the
        // deterministic assignment. There is no timestamp-based release.
        let manifest: NftManifest = mint_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "entry.nft is not an NftManifest".into()
            )))?;
        match (&manifest.batch, &entry.batch_release) {
            (Some(_), None) => {
                return Ok(ValidateCallbackResult::Invalid(
                    "a blind-batch NFT cannot be transferred without a BatchRelease".into(),
                ));
            }
            (None, Some(_)) => {
                return Ok(ValidateCallbackResult::Invalid(
                    "BatchRelease cited for an NFT that is not in a batch".into(),
                ));
            }
            (Some(_), Some(release)) => {
                match validate_batch_release(&entry.nft, &entry.to_agent, &manifest, release)? {
                    ValidateCallbackResult::Valid => {}
                    other => return Ok(other),
                }
            }
            (None, None) => {}
        }
    } else {
        // Later transfer: prev_state is the previously accepted TransferReceipt.
        let (_, prev_receipt) = load_receipt(&entry.prev_state)?;
        let (_, prev_commitment) = load_commitment(&prev_receipt.commitment)?;

        if prev_commitment.nft != entry.nft {
            return Ok(ValidateCallbackResult::Invalid(
                "prev_state belongs to a different NFT".into(),
            ));
        }
        if prev_commitment.to_agent != seller {
            return Ok(ValidateCallbackResult::Invalid(
                "seller does not hold the prior accepted ownership state".into(),
            ));
        }
        if entry.sequence != prev_commitment.sequence + 1 {
            return Ok(ValidateCallbackResult::Invalid(
                "sequence must increment by exactly 1".into(),
            ));
        }
    }

    // 2. Uniqueness on the SELLER's own (linear) chain: no earlier commitment by
    //    this seller superseding the same `prev_state` unless it was cancelled.
    let prev = action
        .prev_action()
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "commitment has no prev_action".into()
            ))
        })?
        .clone();
    if seller_has_open_duplicate(&seller, prev, &entry.nft, &entry.prev_state)? {
        return Ok(ValidateCallbackResult::Invalid(
            "this ownership state has already been offered and not cancelled".into(),
        ));
    }

    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_transfer_receipt(
    action: TypedAction<EntryCreationData>,
    entry: TransferReceipt,
) -> ExternResult<ValidateCallbackResult> {
    let (commitment_rec, commitment) = load_commitment(&entry.commitment)?;

    // 1. Only the named buyer may accept.
    if action.author() != &commitment.to_agent {
        return Ok(ValidateCallbackResult::Invalid(
            "only the named buyer may accept this transfer".into(),
        ));
    }

    // 2. Accept-vs-cancel race resolved by the transfer window using ACTION
    //    timestamps (available in validate()). Acceptance is valid only *within*
    //    the window; cancellation only *after* it. The two are disjoint.
    let props = CollectionProperties::get()?;
    let deadline = window_deadline_micros(
        commitment_rec.action().timestamp(),
        props.transfer_window_secs,
    );
    if action.timestamp().as_micros() > deadline {
        return Ok(ValidateCallbackResult::Invalid(
            "acceptance is after the transfer window closed".into(),
        ));
    }

    // 3. Buyer must not have already accepted this commitment (buyer's own chain).
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("receipt has no prev_action".into())))?
        .clone();
    if agent_already_accepted(action.author(), prev, &entry.commitment)? {
        return Ok(ValidateCallbackResult::Invalid(
            "this commitment has already been accepted by this agent".into(),
        ));
    }

    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_transfer_cancellation(
    action: TypedAction<EntryCreationData>,
    entry: TransferCancellation,
) -> ExternResult<ValidateCallbackResult> {
    let (commitment_rec, _commitment) = load_commitment(&entry.commitment)?;

    // Only the original offerer may cancel...
    if action.author() != commitment_rec.action().author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the offerer may cancel a commitment".into(),
        ));
    }
    // ...and only once the window has closed, so a valid acceptance and a valid
    // cancellation can never coexist for one commitment.
    let props = CollectionProperties::get()?;
    let deadline = window_deadline_micros(
        commitment_rec.action().timestamp(),
        props.transfer_window_secs,
    );
    if action.timestamp().as_micros() <= deadline {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot cancel before the transfer window closes".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// ---------------------------------------------------------------------------
// Own-chain walks (deterministic: a single agent's chain is strictly ordered;
// must_get_agent_activity DEFERS on an incomplete chain rather than failing)
// ---------------------------------------------------------------------------

fn seller_has_open_duplicate(
    seller: &AgentPubKey,
    chain_top: ActionHash,
    nft: &ActionHash,
    prev_state: &ActionHash,
) -> ExternResult<bool> {
    let activity = must_get_agent_activity(seller.clone(), ChainFilter::new(chain_top))?;

    let mut matching_commitments: Vec<ActionHash> = Vec::new();
    let mut cancelled: Vec<ActionHash> = Vec::new();

    for a in activity.iter() {
        let action = a.action.action();
        let this_hash = a.action.action_address().clone();

        if is_create_of(action, UnitEntryTypes::TransferCommitment) {
            let (_, c) = load_commitment(&this_hash)?;
            if &c.nft == nft && &c.prev_state == prev_state {
                matching_commitments.push(this_hash);
            }
        } else if is_create_of(action, UnitEntryTypes::TransferCancellation) {
            let record = must_get_valid_record(this_hash)?;
            if let Some(x) = record
                .entry()
                .to_app_option::<TransferCancellation>()
                .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            {
                cancelled.push(x.commitment);
            }
        }
    }

    Ok(matching_commitments.iter().any(|h| !cancelled.contains(h)))
}

fn agent_already_accepted(
    agent: &AgentPubKey,
    chain_top: ActionHash,
    commitment: &ActionHash,
) -> ExternResult<bool> {
    let activity = must_get_agent_activity(agent.clone(), ChainFilter::new(chain_top))?;
    for a in activity.iter() {
        let action = a.action.action();
        if is_create_of(action, UnitEntryTypes::TransferReceipt) {
            let (_, r) = load_receipt(&a.action.action_address().clone())?;
            if &r.commitment == commitment {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
