//! Bridges a Unyt `nft_sale_escrow` settlement into this app's existing
//! transfer flow.
//!
//! # DEMO-MODE TRUST MODEL
//!
//! The public extern currently accepts only the NFT, intended buyer, and RAVE
//! hash. The RAVE hash is still NOT independently fetched or verified here.
//! `verify_rave_settlement` is therefore an explicit demo-only seam which
//! reconstructs the settlement claims from the local call context.
//!
//! This is intentionally not the final production verifier. The production
//! implementation will replace `verify_rave_settlement` with a call into the
//! authoritative Unyt coordinator once its actual API is known.
//!
//! # Production upgrade boundary
//!
//! The externally visible input should remain:
//!
//!     nft + to_agent + rave
//!
//! The verifier should return an authoritative `VerifiedRaveSettlement`.
//! Everything after that point is ordinary NFT transfer logic and should not
//! need to know how Unyt stores or retrieves a RAVE.
//!
//! # Design choice: no integrity changes
//!
//! No `integrity/nft` edits are required. The RAVE hash continues to ride in
//! the `AgentToInboundCommitment` link's tag for audit/display purposes.
//!
//! # Still-unverified conventions
//!
//! `nft_id_for` assumes a Unyt listing's `nft_id` equals this NFT's mint-action
//! hash rendered via `ActionHash`'s own string form. The identity representation
//! used by Unyt for `previous_owner`/`new_owner` is also not yet verified
//! against the real Unyt coordinator API.

use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct SettleFromRaveInput {
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
    pub rave: ActionHash,
}

/// Settlement data after the Unyt verification boundary.
///
/// Nothing outside the verifier should treat caller-supplied settlement
/// fields as authoritative.
#[derive(Debug)]
struct VerifiedRaveSettlement {
    nft_id: String,
    previous_owner: String,
    new_owner: String,
    fulfilled: bool,
}

/// The convention this integration establishes for matching a Unyt listing's
/// opaque `nft_id` string back to a real NFT in this DNA.
fn nft_id_for(nft: &ActionHash) -> String {
    nft.to_string()
}

/// DEMO ONLY.
///
/// This is the temporary verification seam. It deliberately does not fetch
/// the RAVE because the real Unyt coordinator API has not been established.
///
/// Keeping this boundary private means the eventual production verifier can
/// replace this function without changing the transfer/provenance logic below.
///
/// `rave` is currently accepted only so the public API already has the final
/// shape. Its hash is recorded separately on the resulting inbound link.
fn verify_rave_settlement(
    _rave: &ActionHash,
    nft: &ActionHash,
    seller: &AgentPubKey,
    buyer: &AgentPubKey,
) -> ExternResult<VerifiedRaveSettlement> {
    Ok(VerifiedRaveSettlement {
        nft_id: nft_id_for(nft),
        previous_owner: seller.to_string(),
        new_owner: buyer.to_string(),
        fulfilled: true,
    })
}

/// Validate an already-verified Unyt settlement against the NFT transfer
/// requested by the seller.
///
/// These checks remain separate from `verify_rave_settlement`: the verifier
/// establishes where the settlement came from; this function establishes
/// whether that settlement authorizes this particular NFT transfer.
fn check_verified_settlement(
    settlement: &VerifiedRaveSettlement,
    nft: &ActionHash,
    seller: &AgentPubKey,
    buyer: &AgentPubKey,
) -> ExternResult<()> {
    if !settlement.fulfilled {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "verified settlement is not fulfilled".into()
        )));
    }

    if settlement.nft_id != nft_id_for(nft) {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "verified settlement's nft_id does not match this NFT".into()
        )));
    }

    if settlement.new_owner != buyer.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "verified settlement's new_owner does not match the intended buyer".into()
        )));
    }

    if settlement.previous_owner != seller.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "verified settlement's previous_owner does not match the seller".into()
        )));
    }

    Ok(())
}

/// Seller-initiated transfer gated on a verified Unyt settlement.
///
/// The public input contains no caller-supplied settlement claims. The
/// settlement verification boundary is isolated in `verify_rave_settlement`.
/// Once that boundary returns successfully, the existing TransferCommitment
/// and provenance flow is reused unchanged.
#[hdk_extern]
pub fn initiate_transfer_from_rave(
    input: SettleFromRaveInput,
) -> ExternResult<crate::transfer::InitiateTransferOutput> {
    let me = agent_info()?.agent_initial_pubkey;

    // Same authoritative ownership check `initiate_transfer` and
    // `create_listing` already use.
    if crate::query::current_owner(input.nft.clone())? != me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "caller is not the current owner per the provenance chain".into()
        )));
    }

    let settlement = verify_rave_settlement(&input.rave, &input.nft, &me, &input.to_agent)?;

    check_verified_settlement(&settlement, &input.nft, &me, &input.to_agent)?;

    let (prev_state, sequence) = crate::query::next_transfer_state(&input.nft)?;
    let commitment = TransferCommitment {
        nft: input.nft.clone(),
        to_agent: input.to_agent.clone(),
        prev_state,
        sequence,
        batch_release: None,
    };
    let commitment_hash = create_entry(&EntryTypes::TransferCommitment(commitment))?;

    // The RAVE hash identifies the settlement used to authorize this offer.
    // The link tag is informational; verification happened above.
    create_link(
        input.to_agent.clone(),
        commitment_hash.clone(),
        LinkTypes::AgentToInboundCommitment,
        LinkTag::new(input.rave.clone().into_inner()),
    )?;

    let _ = send_remote_signal(
        RemoteSignal::TransferOffered {
            commitment: commitment_hash.clone(),
        },
        vec![input.to_agent],
    );

    Ok(crate::transfer::InitiateTransferOutput {
        commitment: commitment_hash,
    })
}
