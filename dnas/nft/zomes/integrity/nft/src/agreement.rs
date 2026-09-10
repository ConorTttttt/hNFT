use crate::nft_manifest::NftAttribute;
use hdi::prelude::*;

/// A minter's public, immutable commitment about *when* a sealed NFT's metadata
/// will be revealed, and *who else* may reveal it. Authored by the minter,
/// linked from the mint action via `NftToAgreement`.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct MintAgreement {
    /// The NFT this agreement covers (its mint Create action).
    pub nft: ActionHash,
    /// When the minter undertakes to have revealed by. After this, a named
    /// `delegate` may reveal on the minter's behalf.
    pub reveal_by: Timestamp,
    /// After this, anyone may author a `RevealDefault` marking the NFT as
    /// never-revealed. Must be strictly after `reveal_by`.
    pub default_after: Timestamp,
    /// Optional agent authorised to reveal once `reveal_by` has passed.
    pub delegate: Option<AgentPubKey>,
    /// Unused in phase 1 — seam for a future escrowed-payment leg.
    #[serde(default)]
    pub payment_ref: Option<ActionHash>,
}

/// Marks that a `MintAgreement`'s `default_after` passed with the NFT still
/// sealed. Anyone may author it (buyer, curator, a watchdog). Linked from the
/// mint action via `NftToRevealDefault`.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct RevealDefault {
    pub nft: ActionHash,
    pub agreement: ActionHash,
}

/// The reveal preimage, kept as a **private** entry on the minter's source chain
/// so the scheduled auto-reveal sweep can reveal without the browser's
/// IndexedDB copy. Never published to the DHT.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct RevealSecret {
    pub nft: ActionHash,
    pub salt: [u8; 32],
    pub attributes: Vec<NftAttribute>,
    pub image_uri: String,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_create_mint_agreement(
    action: TypedAction<EntryCreationData>,
    entry: MintAgreement,
) -> ExternResult<ValidateCallbackResult> {
    let mint = must_get_action(entry.nft.clone())?;
    let minter = mint.action().author().clone();

    if action.author() != &minter {
        return Ok(ValidateCallbackResult::Invalid(
            "only the NFT's minter may author its MintAgreement".into(),
        ));
    }
    if entry.reveal_by.as_micros() >= entry.default_after.as_micros() {
        return Ok(ValidateCallbackResult::Invalid(
            "reveal_by must be strictly before default_after".into(),
        ));
    }
    if entry.reveal_by.as_micros() <= mint.action().timestamp().as_micros() {
        return Ok(ValidateCallbackResult::Invalid(
            "reveal_by must be after the mint".into(),
        ));
    }
    if entry.delegate.as_ref() == Some(&minter) {
        return Ok(ValidateCallbackResult::Invalid(
            "delegate must not be the minter".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_reveal_default(
    action: TypedAction<EntryCreationData>,
    entry: RevealDefault,
) -> ExternResult<ValidateCallbackResult> {
    let agreement_rec = must_get_valid_record(entry.agreement.clone())?;
    let agreement: MintAgreement = agreement_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a MintAgreement".into()
        )))?;

    if agreement.nft != entry.nft {
        return Ok(ValidateCallbackResult::Invalid(
            "RevealDefault.nft does not match the agreement's nft".into(),
        ));
    }
    // Deadline gate — same technique as the transfer window
    // (validate_create_transfer_receipt). Action timestamps are available in
    // validate(); wall-clock sys_time is not.
    if action.timestamp().as_micros() <= agreement.default_after.as_micros() {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot mark a reveal default before default_after".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_reveal_secret(
    _action: TypedAction<EntryCreationData>,
    entry: RevealSecret,
) -> ExternResult<ValidateCallbackResult> {
    // Private entry — content is on the author's chain only. Just confirm the
    // NFT it references exists.
    must_get_action(entry.nft)?;
    Ok(ValidateCallbackResult::Valid)
}
