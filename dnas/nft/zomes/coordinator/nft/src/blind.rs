//! Coordinator externs for G3a blind fixed-inventory batches. See
//! `integrity/nft/src/blind_collection.rs` for the protocol and what
//! validation does and does not prove.
//!
//! Lifecycle: `create_blind_batch` -> `mint_batch_nfts` (chunked) ->
//! `finalize_inventory` -> `reserve_blind_purchase`* -> `close_batch` ->
//! `record_randomness` -> `get_assignments` -> `release_batch_transfer`.
//!
//! All assignment math lives in `nft_integrity` (`assign_slots` etc.) and is
//! shared with validation; nothing is reimplemented here. Discovery uses links,
//! which validators never read. The randomness verifier is DEV-ONLY.
//!
//! No payment coupling: nothing here touches Unyt. Refunds/escrow are deferred.

use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

/// Coordinator-side cap on one `mint_batch_nfts` call (a whole batch is minted
/// over several calls; the DRAFT state tolerates a partial mint).
pub const MAX_MINT_CHUNK: usize = 128;

fn guest(msg: impl Into<String>) -> WasmError {
    wasm_error!(WasmErrorInner::Guest(msg.into()))
}

fn get_record(hash: &ActionHash, what: &str) -> ExternResult<Record> {
    get(hash.clone(), GetOptions::default())?.ok_or(guest(format!("{what} not found")))
}

fn decode<T>(record: &Record, what: &str) -> ExternResult<T>
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
{
    record
        .entry()
        .to_app_option()
        .map_err(|e| guest(format!("{e:?}")))?
        .ok_or(guest(format!("not a {what}")))
}

/// Oldest link target of `link_type` from `base` (discovery only).
fn first_link_target(base: &ActionHash, link_type: LinkTypes) -> ExternResult<Option<ActionHash>> {
    let mut links = get_links(
        LinkQuery::try_new(base.clone(), link_type)?,
        GetStrategy::Network,
    )?;
    links.sort_by_key(|l| l.timestamp);
    Ok(links.into_iter().find_map(|l| l.target.into_action_hash()))
}

fn batch_and_params(batch: &ActionHash) -> ExternResult<(Record, BlindBatchCommitment)> {
    let rec = get_record(batch, "batch")?;
    let params = decode(&rec, "BlindBatchCommitment")?;
    Ok((rec, params))
}

// ---------------------------------------------------------------------------
// create_blind_batch / mint_batch_nfts / finalize_inventory (minter)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateBlindBatchInput {
    pub batch_size: u32,
    /// Advisory only; never used by consensus validation.
    #[serde(default)]
    pub sale_window_secs: u64,
}

/// Create the batch commitment (DRAFT). Returns the batch id.
#[hdk_extern]
pub fn create_blind_batch(input: CreateBlindBatchInput) -> ExternResult<ActionHash> {
    check_batch_params(input.batch_size, ASSIGNMENT_ALGORITHM_V1).map_err(guest)?;
    let batch = create_entry(&EntryTypes::BlindBatchCommitment(BlindBatchCommitment {
        batch_size: input.batch_size,
        sale_window_secs: input.sale_window_secs,
        assignment_algorithm_version: ASSIGNMENT_ALGORITHM_V1,
    }))?;
    let path = Path::from(crate::COLLECTION_PATH);
    create_link(
        path.path_entry_hash()?,
        batch.clone(),
        LinkTypes::PathToBlindBatch,
        (),
    )?;
    Ok(batch)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MintBatchNftsInput {
    pub batch: ActionHash,
    /// Slot of `metadata_hashes[0]`; slots run consecutively from here.
    pub start_slot: u32,
    /// The sealed-manifest commitment for each slot in this chunk.
    pub metadata_hashes: Vec<[u8; 32]>,
}

/// Mint one chunk of sealed NFTs into the batch. Validation (not this function)
/// enforces that the caller authored the batch and the slot is in range.
#[hdk_extern]
pub fn mint_batch_nfts(input: MintBatchNftsInput) -> ExternResult<Vec<ActionHash>> {
    if input.metadata_hashes.is_empty() || input.metadata_hashes.len() > MAX_MINT_CHUNK {
        return Err(guest(format!("chunk must hold 1..={MAX_MINT_CHUNK} NFTs")));
    }
    let mut out = Vec::with_capacity(input.metadata_hashes.len());
    for (i, metadata_hash) in input.metadata_hashes.into_iter().enumerate() {
        let (nft, _) = crate::mint::mint_manifest_in_batch(
            metadata_hash,
            Some(input.batch.clone()),
            Some(input.start_slot + i as u32),
        )?;
        out.push(nft);
    }
    Ok(out)
}

/// Bind slot -> NFT. Collects this agent's own original sealed mints for the
/// batch (first per slot), requires every slot to be covered, and writes the
/// immutable `InventoryFinalized`.
#[hdk_extern]
pub fn finalize_inventory(batch: ActionHash) -> ExternResult<ActionHash> {
    let (_, params) = batch_and_params(&batch)?;
    let collection_id = CollectionProperties::collection_id_bytes()?;
    let n = params.batch_size as usize;

    let mut slots: Vec<Option<(ActionHash, [u8; 32])>> = vec![None; n];
    for rec in query(ChainQueryFilter::new().include_entries(true))? {
        if !matches!(rec.action().action_type(), ActionType::Create) {
            continue;
        }
        let Ok(Some(m)) = rec.entry().to_app_option::<NftManifest>() else {
            continue;
        };
        let (Some(b), Some(slot)) = (m.batch.as_ref(), m.batch_slot) else {
            continue;
        };
        if b != &batch || slot as usize >= n {
            continue;
        }
        if slots[slot as usize].is_none() {
            slots[slot as usize] = Some((rec.action_address().clone(), m.metadata_hash));
        }
    }
    let missing = slots.iter().filter(|s| s.is_none()).count();
    if missing > 0 {
        return Err(guest(format!(
            "{missing} of {n} slots have no sealed NFT yet -- mint them before finalizing"
        )));
    }
    let listed: Vec<(ActionHash, [u8; 32])> = slots.into_iter().flatten().collect();
    let leaves: Vec<[u8; 32]> = listed
        .iter()
        .enumerate()
        .map(|(i, (nft, md))| inventory_leaf(&collection_id, &batch, i as u32, nft, md))
        .collect();
    let digest = inventory_digest(&collection_id, &batch, &leaves);

    let inventory = create_entry(&EntryTypes::InventoryFinalized(InventoryFinalized {
        batch: batch.clone(),
        nfts: listed.into_iter().map(|(nft, _)| nft).collect(),
        inventory_digest: digest,
    }))?;
    create_link(batch, inventory.clone(), LinkTypes::BatchToInventory, ())?;
    Ok(inventory)
}

// ---------------------------------------------------------------------------
// reserve_blind_purchase (buyer)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct ReserveBlindPurchaseInput {
    pub batch: ActionHash,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReserveBlindPurchaseOutput {
    pub purchase: ActionHash,
}

/// Reserve a purchase against the batch's finalized inventory. Payment is out
/// of scope here (deferred to the payment layer).
#[hdk_extern]
pub fn reserve_blind_purchase(
    input: ReserveBlindPurchaseInput,
) -> ExternResult<ReserveBlindPurchaseOutput> {
    let batch_rec = get_record(&input.batch, "batch")?;
    let minter = batch_rec.action().author().clone();
    let inventory = first_link_target(&input.batch, LinkTypes::BatchToInventory)?
        .ok_or(guest("this batch has no finalized inventory yet"))?;

    let purchase = create_entry(&EntryTypes::BlindPurchaseCommitment(
        BlindPurchaseCommitment {
            batch: input.batch.clone(),
            inventory,
        },
    ))?;
    create_link(
        input.batch.clone(),
        purchase.clone(),
        LinkTypes::BatchToPurchase,
        (),
    )?;
    // Best-effort nudge; BatchToPurchase is the durable discovery path.
    let _ = send_remote_signal(
        RemoteSignal::BlindPurchaseReserved {
            batch: input.batch,
            purchase: purchase.clone(),
        },
        vec![minter],
    );
    Ok(ReserveBlindPurchaseOutput { purchase })
}

// ---------------------------------------------------------------------------
// close_batch (minter)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct CloseBatchInput {
    pub batch: ActionHash,
    /// Explicit purchase set (required if more than N purchases exist).
    /// `None` = every purchase currently discoverable via `BatchToPurchase`.
    /// Note the minter chooses the set: omission cannot be prevented.
    #[serde(default)]
    pub purchases: Option<Vec<ActionHash>>,
}

/// Commit the purchase set (1 <= K <= N, sorted) and close the batch.
#[hdk_extern]
pub fn close_batch(input: CloseBatchInput) -> ExternResult<ActionHash> {
    let (_, params) = batch_and_params(&input.batch)?;
    let inventory = first_link_target(&input.batch, LinkTypes::BatchToInventory)?
        .ok_or(guest("this batch has no finalized inventory"))?;

    let mut purchases = match input.purchases {
        Some(p) => p,
        None => get_links(
            LinkQuery::try_new(input.batch.clone(), LinkTypes::BatchToPurchase)?,
            GetStrategy::Network,
        )?
        .into_iter()
        .filter_map(|l| l.target.into_action_hash())
        .collect(),
    };
    purchases = canonical_purchase_order(purchases);
    purchases.dedup();
    if purchases.len() as u64 > params.batch_size as u64 {
        return Err(guest(format!(
            "{} purchases exist but the batch holds {}; pass an explicit list of at most {}",
            purchases.len(),
            params.batch_size,
            params.batch_size
        )));
    }
    check_close_shape(&purchases, params.batch_size).map_err(guest)?;

    let close = create_entry(&EntryTypes::BatchClose(BatchClose {
        batch: input.batch.clone(),
        inventory,
        purchases,
    }))?;
    create_link(input.batch, close.clone(), LinkTypes::BatchToClose, ())?;
    Ok(close)
}

// ---------------------------------------------------------------------------
// Randomness (anyone). DEV-ONLY verifier in G3a.
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct DevProofInput {
    pub close: ActionHash,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
}

/// The proof the INSECURE dev verifier accepts for this collection's spec.
/// Public and forgeable by design; only meaningful when `provider_id` is the
/// dev provider.
#[hdk_extern]
pub fn dev_randomness_proof(input: DevProofInput) -> ExternResult<Vec<u8>> {
    let spec = CollectionProperties::get()?
        .randomness
        .ok_or(guest("this collection has no RandomnessSpec"))?;
    let close: BatchClose = decode(&get_record(&input.close, "close")?, "BatchClose")?;
    let (_, batch) = batch_and_params(&close.batch)?;
    let collection_id = CollectionProperties::collection_id_bytes()?;
    let ctx = VerificationContext {
        collection_id: &collection_id,
        close_hash: &input.close,
        close: &close,
        batch_size: batch.batch_size,
    };
    dev_insecure_proof_for_spec(&spec, &ctx, &input.event_id, &input.randomness)
        .map_err(|e| guest(e.to_string()))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RecordRandomnessInput {
    pub close: ActionHash,
    pub event_id: Vec<u8>,
    pub randomness: [u8; 32],
    pub proof: Vec<u8>,
}

/// Record a randomness result for a close. `provider_id` comes from the
/// collection's `RandomnessSpec`; validation verifies the proof.
#[hdk_extern]
pub fn record_randomness(input: RecordRandomnessInput) -> ExternResult<ActionHash> {
    let spec = CollectionProperties::get()?
        .randomness
        .ok_or(guest("this collection has no RandomnessSpec"))?;
    let result = create_entry(&EntryTypes::RandomnessResult(RandomnessResult {
        close: input.close.clone(),
        provider_id: spec.provider_id,
        event_id: input.event_id,
        randomness: input.randomness,
        proof: input.proof,
    }))?;
    create_link(input.close, result.clone(), LinkTypes::CloseToResult, ())?;
    Ok(result)
}

// ---------------------------------------------------------------------------
// Read-only views
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct BatchStateView {
    /// DRAFT | SALES_OPEN | CLOSED | RANDOMNESS_FIXED. (ASSIGNED, TRANSFERRED
    /// and REVEALED are derived per NFT.)
    pub state: String,
    pub batch_size: u32,
    pub inventory: Option<ActionHash>,
    pub close: Option<ActionHash>,
    pub result: Option<ActionHash>,
}

#[hdk_extern]
pub fn get_batch_state(batch: ActionHash) -> ExternResult<BatchStateView> {
    let (_, params) = batch_and_params(&batch)?;
    let inventory = first_link_target(&batch, LinkTypes::BatchToInventory)?;
    let close = first_link_target(&batch, LinkTypes::BatchToClose)?;
    let result = match &close {
        Some(c) => first_link_target(c, LinkTypes::CloseToResult)?,
        None => None,
    };
    let state = match (&inventory, &close, &result) {
        (None, _, _) => "DRAFT",
        (Some(_), None, _) => "SALES_OPEN",
        (Some(_), Some(_), None) => "CLOSED",
        (Some(_), Some(_), Some(_)) => "RANDOMNESS_FIXED",
    };
    Ok(BatchStateView {
        state: state.into(),
        batch_size: params.batch_size,
        inventory,
        close,
        result,
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetAssignmentsInput {
    pub close: ActionHash,
    pub result: ActionHash,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AssignmentView {
    pub purchase: ActionHash,
    pub buyer: AgentPubKey,
    /// Sorted position of the purchase in the close.
    pub position: u32,
    pub slot: u32,
    /// `InventoryFinalized.nfts[slot]`.
    pub nft: ActionHash,
}

/// The full deterministic assignment for a closed batch with randomness. Uses
/// the same pure functions as validation.
#[hdk_extern]
pub fn get_assignments(input: GetAssignmentsInput) -> ExternResult<Vec<AssignmentView>> {
    let close_rec = get_record(&input.close, "close")?;
    let close: BatchClose = decode(&close_rec, "BatchClose")?;
    let result_rec = get_record(&input.result, "randomness result")?;
    let result: RandomnessResult = decode(&result_rec, "RandomnessResult")?;
    if result.close != input.close {
        return Err(guest("randomness result is bound to a different close"));
    }
    let (_, params) = batch_and_params(&close.batch)?;
    let inventory: InventoryFinalized = decode(
        &get_record(&close.inventory, "inventory")?,
        "InventoryFinalized",
    )?;
    let collection_id = CollectionProperties::collection_id_bytes()?;

    let seed = assignment_seed(
        &collection_id,
        &close.batch,
        &input.close,
        &result.provider_id,
        &result.event_id,
        &result.randomness,
    );
    let slots =
        assign_slots(&seed, params.batch_size, close.purchases.len() as u32).map_err(guest)?;

    let mut out = Vec::with_capacity(slots.len());
    for (i, (purchase, slot)) in close.purchases.iter().zip(slots.iter()).enumerate() {
        let buyer = get_record(purchase, "purchase")?.action().author().clone();
        out.push(AssignmentView {
            purchase: purchase.clone(),
            buyer,
            position: i as u32,
            slot: *slot,
            nft: inventory.nfts[*slot as usize].clone(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// release_batch_transfer (minter)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct ReleaseBatchTransferInput {
    pub nft: ActionHash,
    pub close: ActionHash,
    pub result: ActionHash,
    pub purchase: ActionHash,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReleaseBatchTransferOutput {
    pub commitment: ActionHash,
}

/// First transfer of a batch NFT to its assigned buyer. The recipient is the
/// purchase's author; validation recomputes the assignment and rejects any
/// mismatch, so a wrong NFT/buyer fails here without a UI-supplied claim.
#[hdk_extern]
pub fn release_batch_transfer(
    input: ReleaseBatchTransferInput,
) -> ExternResult<ReleaseBatchTransferOutput> {
    let buyer = get_record(&input.purchase, "purchase")?
        .action()
        .author()
        .clone();
    let commitment = create_entry(&EntryTypes::TransferCommitment(TransferCommitment {
        nft: input.nft.clone(),
        to_agent: buyer.clone(),
        prev_state: input.nft,
        sequence: 1,
        batch_release: Some(BatchRelease {
            close: input.close,
            result: input.result,
            purchase: input.purchase,
        }),
    }))?;
    create_link(
        buyer.clone(),
        commitment.clone(),
        LinkTypes::AgentToInboundCommitment,
        (),
    )?;
    let _ = send_remote_signal(
        RemoteSignal::TransferOffered {
            commitment: commitment.clone(),
        },
        vec![buyer],
    );
    Ok(ReleaseBatchTransferOutput { commitment })
}

// ---------------------------------------------------------------------------
// Discovery (unchanged shape)
// ---------------------------------------------------------------------------

/// Every blind batch ever created in this collection, any minter.
#[hdk_extern]
pub fn get_all_blind_batches(_: ()) -> ExternResult<Vec<Record>> {
    let path = Path::from(crate::COLLECTION_PATH);
    let links = get_links(
        LinkQuery::try_new(path.path_entry_hash()?, LinkTypes::PathToBlindBatch)?,
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

/// Purchases discoverable against `batch` (the minter's input to `close_batch`).
#[hdk_extern]
pub fn purchases_for_batch(batch: ActionHash) -> ExternResult<Vec<Record>> {
    let links = get_links(
        LinkQuery::try_new(batch, LinkTypes::BatchToPurchase)?,
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

/// Read back a `BlindPurchaseCommitment` (immutable; a plain `get`).
#[hdk_extern]
pub fn get_blind_purchase(purchase: ActionHash) -> ExternResult<Option<Record>> {
    get(purchase, GetOptions::default())
}
