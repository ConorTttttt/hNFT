use crate::agreement::{MintAgreement, RevealDefault};
use crate::blind_collection::{load_purchase_commitment, BlindBatchCommitment};
use crate::nft_manifest::{is_create_of, NftManifest};
use crate::transfer::{load_commitment, load_receipt, TransferReceipt};
use crate::UnitEntryTypes;
use hdi::prelude::*;

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

// --- PathToNft : discovery, permanent ---------------------------------------

pub fn validate_create_link_path_to_nft(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_action_hash(&action)?;
    let record = must_get_valid_record(target)?;
    let _nft: NftManifest = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "PathToNft target must be an NftManifest".into()
        )))?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_path_to_nft(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "PathToNft links are permanent".into(),
    ))
}

// --- NftUpdates : reveal chain, permanent ----------------------------------

pub fn validate_create_link_nft_updates(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    // base = original NftManifest Create; target = the reveal Update action.
    let base = base_action_hash(&action)?;
    let base_record = must_get_valid_record(base)?;
    let _nft: NftManifest = base_record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "NftUpdates base must be an NftManifest".into()
        )))?;
    let target = target_action_hash(&action)?;
    must_get_valid_record(target)?;
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_nft_updates(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "NftUpdates links are permanent".into(),
    ))
}

// --- NftToReceipt : provenance, append-only ------------------------------

pub fn validate_create_link_nft_to_receipt(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let nft = base_action_hash(&action)?;
    let receipt_hash = target_action_hash(&action)?;
    let (_, receipt): (_, TransferReceipt) = load_receipt(&receipt_hash)?;
    let (commitment_rec, commitment) = load_commitment(&receipt.commitment)?;

    if commitment.nft != nft {
        return Ok(ValidateCallbackResult::Invalid(
            "NftToReceipt base does not match the receipt's commitment.nft".into(),
        ));
    }
    // Either party to the transfer may publish the provenance link.
    if action.author() != commitment_rec.action().author()
        && action.author() != &commitment.to_agent
    {
        return Ok(ValidateCallbackResult::Invalid(
            "link author is not a party to this transfer".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_nft_to_receipt(
    _action: TypedAction<DeleteLinkData>,
    _original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(
        "NftToReceipt provenance links are append-only".into(),
    ))
}

// --- AgentToInboundCommitment : buyer's offer inbox --------------------------

pub fn validate_create_link_agent_to_inbound_commitment(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let recipient = base_agent(&action)?;
    let commitment_hash = target_action_hash(&action)?;
    let (_, commitment) = load_commitment(&commitment_hash)?;
    if commitment.to_agent != recipient {
        return Ok(ValidateCallbackResult::Invalid(
            "inbox link base must be the commitment's to_agent".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_agent_to_inbound_commitment(
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the link author may remove an inbox link".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// --- AgentToNft : DERIVED ownership index --------------------------------

pub fn validate_create_link_agent_to_nft(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let owner = base_agent(&action)?;
    let nft = target_action_hash(&action)?;

    // LinkTag carries the authorizing evidence's ActionHash as raw 39 bytes
    // (written by the coordinator as `LinkTag::new(hash.into_inner())`).
    let evidence = ActionHash::try_from_raw_39(action.data.tag.clone().0).map_err(|e| {
        wasm_error!(WasmErrorInner::Guest(format!(
            "AgentToNft LinkTag is not an ActionHash: {e:?}"
        )))
    })?;
    let record = must_get_valid_record(evidence)?;

    // Discriminate by entry TYPE, not by a failed deserialize — `to_app_option`
    // errors (not `None`) when the stored entry is a different app type.
    if is_create_of(record.action(), UnitEntryTypes::TransferReceipt) {
        // Transfer case: evidence is a TransferReceipt authorizing `owner`.
        let receipt: TransferReceipt = record
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "AgentToNft evidence receipt has no entry".into()
            )))?;
        let (_, commitment) = load_commitment(&receipt.commitment)?;
        if commitment.nft == nft && commitment.to_agent == owner {
            return Ok(ValidateCallbackResult::Valid);
        }
        return Ok(ValidateCallbackResult::Invalid(
            "AgentToNft evidence does not authorize this owner for this NFT".into(),
        ));
    }

    // Mint case: evidence must be this NFT's own Create, authored by `owner`.
    if !is_create_of(record.action(), UnitEntryTypes::NftManifest) {
        return Ok(ValidateCallbackResult::Invalid(
            "AgentToNft evidence must be a TransferReceipt or the NFT's mint".into(),
        ));
    }
    if record.action_address() != &nft {
        return Ok(ValidateCallbackResult::Invalid(
            "mint-case AgentToNft evidence must be the NFT's mint action".into(),
        ));
    }
    if record.action().author() != &owner {
        return Ok(ValidateCallbackResult::Invalid(
            "mint-case AgentToNft owner must be the minter".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_delete_link_agent_to_nft(
    action: TypedAction<DeleteLinkData>,
    original_action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    // The ownership *ledger* is the provenance chain; this index link being
    // removed is harmless. Require the deleter to be the original link author.
    if action.author() != original_action.author() {
        return Ok(ValidateCallbackResult::Invalid(
            "only the link author may remove their ownership-index link".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// --- PathToBlindBatch : discovery, permanent --------------------------------

pub fn validate_create_link_path_to_blind_batch(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let target = target_action_hash(&action)?;
    let record = must_get_valid_record(target)?;
    let _batch: BlindBatchCommitment = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "PathToBlindBatch target must be a BlindBatchCommitment".into()
        )))?;
    Ok(ValidateCallbackResult::Valid)
}

// --- BatchToPurchase : minter's discovery of purchases against their batch,
// permanent -------------------------------------------------------------

pub fn validate_create_link_batch_to_purchase(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let batch = base_action_hash(&action)?;
    let purchase_hash = target_action_hash(&action)?;
    let (_, purchase) = load_purchase_commitment(&purchase_hash)?;
    if purchase.batch != batch {
        return Ok(ValidateCallbackResult::Invalid(
            "BatchToPurchase base does not match the purchase's batch".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// --- MintAgreement links -----------------------------------------------------

/// NftManifest mint action -> MintAgreement action. Author is the minter (the
/// MintAgreement's own validation already checks its author == the minter).
pub fn validate_create_link_nft_to_agreement(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let nft = base_action_hash(&action)?;
    let target = target_action_hash(&action)?;
    let rec = must_get_valid_record(target)?;
    let agreement: MintAgreement = rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "NftToAgreement target is not a MintAgreement".into()
        )))?;
    if agreement.nft != nft {
        return Ok(ValidateCallbackResult::Invalid(
            "NftToAgreement base does not match the agreement's nft".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// NftManifest mint action -> RevealDefault action.
pub fn validate_create_link_nft_to_reveal_default(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let nft = base_action_hash(&action)?;
    let target = target_action_hash(&action)?;
    let rec = must_get_valid_record(target)?;
    let default: RevealDefault = rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "NftToRevealDefault target is not a RevealDefault".into()
        )))?;
    if default.nft != nft {
        return Ok(ValidateCallbackResult::Invalid(
            "NftToRevealDefault base does not match the default's nft".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// delegate AgentPubKey -> MintAgreement action. Base must be the agreement's
/// named delegate.
pub fn validate_create_link_agent_to_delegated_reveal(
    action: TypedAction<CreateLinkData>,
) -> ExternResult<ValidateCallbackResult> {
    let delegate = base_agent(&action)?;
    let target = target_action_hash(&action)?;
    let rec = must_get_valid_record(target)?;
    let agreement: MintAgreement = rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "AgentToDelegatedReveal target is not a MintAgreement".into()
        )))?;
    if agreement.delegate.as_ref() != Some(&delegate) {
        return Ok(ValidateCallbackResult::Invalid(
            "AgentToDelegatedReveal base is not the agreement's delegate".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
