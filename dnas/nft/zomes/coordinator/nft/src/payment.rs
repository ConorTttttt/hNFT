//! Bridges a Unyt `nft_sale_escrow` settlement into this app's existing
//! transfer flow.
//!
//! # DEMO-MODE TRUST MODEL -- read before reusing this past the demo
//!
//! This version does NOT fetch or verify the RAVE itself. The caller (UI)
//! already talked to the Unyt cell to build the sale, so it's trusted to
//! have read the RAVE's `computed_values` correctly and passes those fields
//! straight in. This zome only checks internal consistency of what it was
//! handed (do the fields match this NFT and this buyer/seller) -- it does
//! NOT confirm those fields came from a real, network-validated RAVE at all.
//!
//! Concretely, the trust shift from the coordinator-fetches-it design: there
//! a forged input could only point at a REAL RAVE (content comes from
//! Unyt's own DHT, so it can be mismatched but not fabricated). Here, a
//! compromised UI build, a malicious browser extension, or a plain bug can
//! hand this function `fulfilled: true` for a sale that never happened, and
//! nothing here can tell the difference.
//!
//! Why this is an acceptable demo trade-off, not a hidden hole: this extern
//! can only be called by an NFT's current owner (checked below, same as
//! `initiate_transfer`), so it only ever gates the SELLER's own decision to
//! give the NFT away. A seller whose own client lies to them mostly just
//! reproduces the outcome of the pre-existing, unconditional
//! `initiate_transfer` (owner gives NFT away for free) -- it does not open a
//! new way for a buyer or third party to take an NFT without the owner's
//! action. Nothing about `TransferCommitment`/`TransferReceipt` validation
//! in `integrity/nft` changes; ownership still only moves through that same
//! chain.
//!
//! # Upgrading past the demo
//!
//! To restore real verification: replace the four plain fields below with a
//! single `rave: ActionHash` + `unyt_cell: CallTargetCell`, and inside
//! `check_claimed_settlement` do a `call()` into the Unyt cell instead of
//! trusting the caller -- see the prior handoff draft for that version. That
//! version needs Unyt's actual coordinator zome API for reading a RAVE (not
//! publicly documented at the time this was written); this one doesn't need
//! it at all, which is the whole point of the demo shortcut.
//!
//! # Design choice carried over unchanged: no integrity changes
//!
//! Still true here: no `integrity/nft` edits, no DNA-hash impact. The RAVE
//! hash rides in the `AgentToInboundCommitment` link's tag (informational
//! only -- no validator reads it), exactly as before.
//!
//! # Still-unverified convention -- confirm before relying on this
//!
//! `nft_id_for` assumes a Unyt listing's `nft_id` equals this NFT's
//! mint-action hash rendered via `ActionHash`'s own string form, and that
//! whatever the UI treats as "the buyer's/seller's Unyt address" stringifies
//! the same way as `AgentPubKey::to_string()` here. Both are app-level
//! conventions the UI must follow consistently, not something this code can
//! check.

use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct SettleFromRaveInput {
    pub nft: ActionHash,
    pub to_agent: AgentPubKey,
    /// The Unyt RAVE's own action hash. Not re-fetched here -- carried
    /// through only so it can be recorded in the inbox link's tag for
    /// later display/audit.
    pub rave: ActionHash,
    /// The following four fields are `output.computed_values.*` from that
    /// RAVE, AS THE UI READ THEM. See module doc -- these are trusted, not
    /// re-verified against the Unyt network by this zome.
    pub fulfilled: bool,
    pub nft_id: String,
    pub new_owner: String,
    pub previous_owner: String,
}

/// The convention this integration establishes for matching a Unyt listing's
/// opaque `nft_id` string back to a real NFT in this DNA. See module doc.
fn nft_id_for(nft: &ActionHash) -> String {
    nft.to_string()
}

/// Checks internal consistency of the fields the UI supplied against this
/// specific sale. Does NOT confirm those fields came from a real RAVE --
/// see module doc.
fn check_claimed_settlement(
    claim: &SettleFromRaveInput,
    nft: &ActionHash,
    seller: &AgentPubKey,
    buyer: &AgentPubKey,
) -> ExternResult<()> {
    if !claim.fulfilled {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement is not marked fulfilled".into()
        )));
    }
    if claim.nft_id != nft_id_for(nft) {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's nft_id does not match this NFT".into()
        )));
    }
    if claim.new_owner != buyer.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's new_owner does not match the intended buyer".into()
        )));
    }
    if claim.previous_owner != seller.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's previous_owner does not match the seller's payout key".into()
        )));
    }
    Ok(())
}

/// Seller-initiated transfer, gated on a UI-supplied (not independently
/// re-verified -- see module doc) Unyt settlement claim. Reuses the same
/// `TransferCommitment` entry, the same provenance chain, and the same
/// integrity validation `initiate_transfer` already relies on.
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

    check_claimed_settlement(&input, &input.nft, &me, &input.to_agent)?;

    let (prev_state, sequence) = crate::query::next_transfer_state(&input.nft)?;
    let commitment = TransferCommitment {
        nft: input.nft.clone(),
        to_agent: input.to_agent.clone(),
        prev_state,
        sequence,
    };
    let commitment_hash = create_entry(&EntryTypes::TransferCommitment(commitment))?;

    // Only difference from `initiate_transfer`: the tag carries the RAVE
    // hash so the buyer (or any auditor walking this link) can see which
    // settlement authorized the offer, instead of an empty tag.
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
