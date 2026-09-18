//! Blind-box collection batches: see DESIGN.md for the full mechanism,
//! attack model, and -- important -- exactly what security claim this
//! design does and does NOT make. This file is the integrity-side
//! (validate()) half only.
//!
//! Demo decisions locked in for this pass (see HANDOFF.md "Decisions that
//! need the user"): minter-controlled resolution order is an accepted
//! trust assumption (not the closed-batch redesign); `BlindPurchaseAbandoned`
//! is a UI-only signal for now (no on-chain dispute/refund path); the
//! minter's X25519 public key lives on `CollectionProperties`.

use crate::dna_properties::CollectionProperties;
use crate::nft_manifest::{commit_hash, NftManifest, NftPayload};
use crate::UnitEntryTypes;
use hdi::prelude::*;

// ---------------------------------------------------------------------------
// Domain separation. Bump this if the transcript shape below ever changes,
// so an old and new client can never be tricked into agreeing on a hash for
// different reasons. Length-prefixed even though currently fixed, so a
// future V2 tag of a different length can't create ambiguity with V1's.
// ---------------------------------------------------------------------------

const ASSIGNMENT_DOMAIN_TAG: &[u8] = b"UNYT-BLIND-ASSIGNMENT-V1";

fn put_length_prefixed(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(bytes);
}

/// The exact, versioned transcript both `validate()` and the coordinator
/// MUST build byte-for-byte identically. Every component here is
/// fixed-width (a `HoloHash`'s `get_raw_39()` is always 39 bytes; the two
/// secrets are always 32) except the domain tag, which is length-prefixed
/// on principle for forward compatibility -- see comment above. Layout:
/// ```text
///   domain_tag.len() [4, LE]  ‖  domain_tag
///   batch.get_raw_39()        [39]
///   purchase.get_raw_39()     [39]
///   buyer_nonce               [32]
///   mint_seed                 [32]
/// ```
pub fn assignment_transcript(
    batch: &ActionHash,
    purchase: &ActionHash,
    buyer_nonce: &[u8; 32],
    mint_seed: &[u8; 32],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + ASSIGNMENT_DOMAIN_TAG.len() + 39 + 39 + 32 + 32);
    put_length_prefixed(&mut buf, ASSIGNMENT_DOMAIN_TAG);
    buf.extend_from_slice(batch.get_raw_39());
    buf.extend_from_slice(purchase.get_raw_39());
    buf.extend_from_slice(buyer_nonce);
    buf.extend_from_slice(mint_seed);
    buf
}

/// `blake2b_256(assignment_transcript(...))[0..4] as u32 % batch_size`,
/// linear-probed forward (wrapping) past any slot in `taken`. See DESIGN.md
/// "Resolution ordering" for the property this deliberately does NOT claim:
/// the final slot depends on which purchases have already been resolved
/// (i.e. on `taken`), which the minter controls the order of -- accepted
/// as a stated trust assumption for this demo.
///
/// Returns `batch_size` (an out-of-range sentinel, never a valid slot) if
/// every slot is taken -- callers must treat that as "batch exhausted."
/// `validate_create_blind_assignment_reveal`'s completeness check is
/// expected to have already ruled that case out before calling this.
pub fn resolve_slot(
    batch: &ActionHash,
    purchase: &ActionHash,
    buyer_nonce: &[u8; 32],
    mint_seed: &[u8; 32],
    batch_size: u32,
    taken: &[u32],
) -> u32 {
    let transcript = assignment_transcript(batch, purchase, buyer_nonce, mint_seed);
    let h = commit_hash(&transcript);
    let start = u32::from_le_bytes([h[0], h[1], h[2], h[3]]) % batch_size;

    let mut slot = start;
    for _ in 0..batch_size {
        if !taken.contains(&slot) {
            return slot;
        }
        slot = (slot + 1) % batch_size;
    }
    batch_size // sentinel: batch exhausted, caller must reject
}

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// Minted once per blind-box batch, before any buyer exists. `seed_hash`
/// commits the minter to `mint_seed` -- restated in full on every
/// `BlindAssignmentReveal` for this batch (not derived incrementally), so
/// each reveal is independently checkable without needing the others.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindBatchCommitment {
    pub batch_size: u32,
    /// commit_hash(mint_seed)
    pub seed_hash: [u8; 32],
}

/// Authored by a BUYER, before they learn anything about assignment. This
/// entry's own author IS the authoritative buyer identity -- nothing
/// duplicates it elsewhere; `BlindAssignmentReveal.purchase` plus
/// `must_get_valid_record` is how the rest of the protocol recovers "who is
/// this purchase's buyer," including in `TransferCommitment`'s
/// `blind_reveal` check (see `transfer.rs`).
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindPurchaseCommitment {
    pub batch: ActionHash,
    /// commit_hash(buyer_nonce)
    pub nonce_hash: [u8; 32],
    /// buyer_nonce encrypted to the minter's X25519 key
    /// (`CollectionProperties::minter_x25519_pubkey`). Opaque transport
    /// data -- validate() never decrypts it, never trusts it, and only
    /// sanity-checks its length. The eventual plaintext in
    /// `BlindAssignmentReveal.buyer_nonce` is what's actually checked
    /// (against `nonce_hash`, above). See DESIGN.md "Nonce delivery."
    #[serde(default)]
    pub encrypted_nonce: Vec<u8>,
}

/// Authored by the MINTER, once per resolved purchase, on the minter's own
/// chain -- see DESIGN.md "Why validate() can check this at all" for why
/// that authorship constraint is what makes this checkable without link
/// enumeration. Deliberately has NO `buyer` field -- the buyer is
/// `purchase`'s own author; storing it again here would be redundant state
/// that could (if ever handled carelessly) disagree with the truth.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindAssignmentReveal {
    pub batch: ActionHash,
    pub purchase: ActionHash,
    pub mint_seed: [u8; 32],
    pub buyer_nonce: [u8; 32],
    /// The Sealed NftManifest mint action assigned to this purchase.
    pub assigned_nft: ActionHash,
}

/// Anyone may author this once a purchase's resolution window has passed
/// with no `BlindAssignmentReveal` -- a liveness SIGNAL, not a refund.
/// validate() can confirm the deadline passed; it deliberately does NOT
/// (and, cheaply, cannot -- see DESIGN.md "Resolution liveness") confirm
/// the purchase is *still* unresolved at the moment this is authored. For
/// this demo, nothing on-chain consumes this beyond that check -- it's a
/// UI-only signal ("unresolved, contact seller"); see HANDOFF.md.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindPurchaseAbandoned {
    pub purchase: ActionHash,
}

// ---------------------------------------------------------------------------
// Shared loaders
// ---------------------------------------------------------------------------

fn load_batch_commitment(hash: &ActionHash) -> ExternResult<(Record, BlindBatchCommitment)> {
    let record = must_get_valid_record(hash.clone())?;
    let batch: BlindBatchCommitment = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a BlindBatchCommitment".into()
        )))?;
    Ok((record, batch))
}

pub fn load_purchase_commitment(
    hash: &ActionHash,
) -> ExternResult<(Record, BlindPurchaseCommitment)> {
    let record = must_get_valid_record(hash.clone())?;
    let purchase: BlindPurchaseCommitment = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a BlindPurchaseCommitment".into()
        )))?;
    Ok((record, purchase))
}

pub fn load_assignment_reveal(hash: &ActionHash) -> ExternResult<(Record, BlindAssignmentReveal)> {
    let record = must_get_valid_record(hash.clone())?;
    let reveal: BlindAssignmentReveal = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a BlindAssignmentReveal".into()
        )))?;
    Ok((record, reveal))
}

/// Is `action` a Create of `BlindAssignmentReveal` in this zome? Same
/// technique -- and same unconfirmed-accessor caveat -- as
/// `nft_manifest::is_create_of`.
fn is_blind_assignment_reveal_create(action: &Action) -> bool {
    if !matches!(action.action_type(), ActionType::Create) {
        return false;
    }
    let Some(EntryType::App(app_entry_def)) = action.entry_type() else {
        return false;
    };
    let Ok(scoped) = ScopedEntryDefIndex::try_from(UnitEntryTypes::BlindAssignmentReveal) else {
        return false;
    };
    app_entry_def.zome_index == scoped.zome_index && app_entry_def.entry_index == scoped.zome_type
}

// ---------------------------------------------------------------------------
// Validation: entries
// ---------------------------------------------------------------------------

pub fn validate_create_blind_batch_commitment(
    _action: TypedAction<EntryCreationData>,
    entry: BlindBatchCommitment,
) -> ExternResult<ValidateCallbackResult> {
    if entry.batch_size == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "batch_size must be greater than zero".into(),
        ));
    }
    if entry.seed_hash == [0u8; 32] {
        return Ok(ValidateCallbackResult::Invalid(
            "seed_hash must not be zero".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_blind_purchase_commitment(
    _action: TypedAction<EntryCreationData>,
    entry: BlindPurchaseCommitment,
) -> ExternResult<ValidateCallbackResult> {
    must_get_valid_record(entry.batch.clone())?; // the batch is real
    if entry.nonce_hash == [0u8; 32] {
        return Ok(ValidateCallbackResult::Invalid(
            "nonce_hash must not be zero".into(),
        ));
    }
    // Sanity bound only -- see struct doc; validate() never decrypts this.
    if entry.encrypted_nonce.is_empty() || entry.encrypted_nonce.len() > 4096 {
        return Ok(ValidateCallbackResult::Invalid(
            "encrypted_nonce is missing or implausibly large".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_blind_assignment_reveal(
    action: TypedAction<EntryCreationData>,
    entry: BlindAssignmentReveal,
) -> ExternResult<ValidateCallbackResult> {
    // 1. Author must be the batch's own minter.
    let (batch_rec, batch) = load_batch_commitment(&entry.batch)?;
    let minter = batch_rec.action().author().clone();
    if action.author() != &minter {
        return Ok(ValidateCallbackResult::Invalid(
            "only the batch's own minter may author its BlindAssignmentReveal".into(),
        ));
    }

    // 2. mint_seed matches the batch's commitment.
    if commit_hash(&entry.mint_seed) != batch.seed_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "mint_seed does not match the batch's seed_hash".into(),
        ));
    }

    // 3. The purchase belongs to this batch, and buyer_nonce matches its
    //    commitment. The purchase's own author is the authoritative buyer
    //    -- see BlindAssignmentReveal's struct doc on why there's no
    //    separate `buyer` field to check for agreement with.
    let (_, purchase) = load_purchase_commitment(&entry.purchase)?;
    if purchase.batch != entry.batch {
        return Ok(ValidateCallbackResult::Invalid(
            "purchase does not belong to this batch".into(),
        ));
    }
    if commit_hash(&entry.buyer_nonce) != purchase.nonce_hash {
        return Ok(ValidateCallbackResult::Invalid(
            "buyer_nonce does not match the purchase's nonce_hash".into(),
        ));
    }

    // 4. assigned_nft is real, belongs to this batch, and is still Sealed.
    let assigned_rec = must_get_valid_record(entry.assigned_nft.clone())?;
    let assigned: NftManifest = assigned_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "assigned_nft is not an NftManifest".into()
        )))?;
    if assigned.batch != Some(entry.batch.clone()) {
        return Ok(ValidateCallbackResult::Invalid(
            "assigned_nft does not belong to this batch".into(),
        ));
    }
    if !matches!(assigned.payload, NftPayload::Sealed) {
        return Ok(ValidateCallbackResult::Invalid(
            "assigned_nft is not Sealed -- already revealed or invalid state".into(),
        ));
    }
    let Some(assigned_slot) = assigned.batch_slot else {
        return Ok(ValidateCallbackResult::Invalid(
            "assigned_nft has no batch_slot -- not minted as part of a batch".into(),
        ));
    };

    // 5. Walk the minter's OWN chain (deterministic -- see DESIGN.md "Why
    //    validate() can check this at all") to collect every EARLIER
    //    BlindAssignmentReveal for this same batch: which slots and which
    //    purchases are already spoken for.
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("reveal has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(minter, ChainFilter::new(prev))?;

    let mut taken_slots: Vec<u32> = Vec::new();
    for a in activity.iter() {
        let act = a.action.action();
        if !is_blind_assignment_reveal_create(act) {
            continue;
        }
        let this_hash = a.action.action_address().clone();
        let (_, earlier) = load_assignment_reveal(&this_hash)?;
        if earlier.batch != entry.batch {
            continue; // this minter may run multiple batches
        }
        if earlier.purchase == entry.purchase {
            return Ok(ValidateCallbackResult::Invalid(
                "this purchase has already been assigned".into(),
            ));
        }
        // Re-derive the earlier reveal's own assigned_nft's batch_slot
        // rather than trusting a cached number, so a maliciously-crafted
        // earlier reveal can't lie about which slot it consumed.
        let earlier_nft_rec = must_get_valid_record(earlier.assigned_nft.clone())?;
        let earlier_nft: NftManifest = earlier_nft_rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "earlier reveal's assigned_nft is not an NftManifest".into()
            )))?;
        if let Some(slot) = earlier_nft.batch_slot {
            taken_slots.push(slot);
        }
    }

    // 6. Completeness: the batch isn't already fully assigned.
    if taken_slots.len() as u32 >= batch.batch_size {
        return Ok(ValidateCallbackResult::Invalid(
            "batch is already fully assigned".into(),
        ));
    }

    // 7. Recompute the expected slot and confirm it matches. NOTE what this
    //    does and does not prove -- see DESIGN.md "Resolution ordering":
    //    this confirms the slot is CONSISTENT with `taken_slots` as
    //    observed on the minter's chain up to this point, not that the
    //    minter couldn't have chosen a more favorable resolution ORDER
    //    before writing this reveal -- accepted as a trust assumption for
    //    this demo.
    let expected_slot = resolve_slot(
        &entry.batch,
        &entry.purchase,
        &entry.buyer_nonce,
        &entry.mint_seed,
        batch.batch_size,
        &taken_slots,
    );
    if expected_slot != assigned_slot {
        return Ok(ValidateCallbackResult::Invalid(format!(
            "assigned_nft's batch_slot ({assigned_slot}) does not match the deterministic \
             assignment ({expected_slot}) for this purchase given prior resolutions"
        )));
    }

    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_blind_purchase_abandoned(
    action: TypedAction<EntryCreationData>,
    entry: BlindPurchaseAbandoned,
) -> ExternResult<ValidateCallbackResult> {
    let (purchase_rec, _purchase) = load_purchase_commitment(&entry.purchase)?;
    let props = CollectionProperties::get()?;
    let deadline = purchase_rec.action().timestamp().as_micros()
        + (props.blind_resolution_window_secs as i64) * 1_000_000;
    if action.timestamp().as_micros() <= deadline {
        return Ok(ValidateCallbackResult::Invalid(
            "cannot mark a purchase abandoned before the resolution window closes".into(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

// ---------------------------------------------------------------------------
// Property tests for resolve_slot / assignment_transcript. These are NOT a
// substitute for the fixed, cross-implementation test vectors DESIGN.md
// calls for (pin real output bytes once both validate() and the coordinator
// exist, from an actual compiled run) -- they only check that the shape of
// the function's behavior holds, so a refactor doesn't silently break
// determinism or domain separation.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(byte: u8) -> ActionHash {
        ActionHash::from_raw_36(vec![byte; 36])
    }

    #[test]
    fn deterministic_for_identical_inputs() {
        let batch = hash_of(1);
        let purchase = hash_of(2);
        let nonce = [3u8; 32];
        let seed = [4u8; 32];
        let a = resolve_slot(&batch, &purchase, &nonce, &seed, 100, &[]);
        let b = resolve_slot(&batch, &purchase, &nonce, &seed, 100, &[]);
        assert_eq!(a, b);
    }

    #[test]
    fn linear_probe_skips_taken_slots() {
        let batch = hash_of(1);
        let purchase = hash_of(2);
        let nonce = [3u8; 32];
        let seed = [4u8; 32];
        let raw = resolve_slot(&batch, &purchase, &nonce, &seed, 100, &[]);
        let taken: Vec<u32> = (0..100).filter(|&s| s != raw).collect();
        // Only `raw` itself is free -- must resolve to exactly that slot.
        let resolved = resolve_slot(&batch, &purchase, &nonce, &seed, 100, &taken);
        assert_eq!(resolved, raw);
    }

    #[test]
    fn fully_taken_batch_returns_sentinel() {
        let batch = hash_of(1);
        let purchase = hash_of(2);
        let nonce = [3u8; 32];
        let seed = [4u8; 32];
        let taken: Vec<u32> = (0..10).collect();
        let resolved = resolve_slot(&batch, &purchase, &nonce, &seed, 10, &taken);
        assert_eq!(resolved, 10); // sentinel == batch_size
    }

    #[test]
    fn different_purchase_usually_different_slot() {
        // Not a guarantee (collisions are possible and fine -- that's what
        // linear probing is for), but a same-slot result for every distinct
        // input across a reasonable sample would indicate a construction
        // bug, so this is a smoke test, not a security property.
        let batch = hash_of(1);
        let nonce = [3u8; 32];
        let seed = [4u8; 32];
        let mut distinct = std::collections::HashSet::new();
        for i in 0..20u8 {
            let purchase = hash_of(i);
            distinct.insert(resolve_slot(&batch, &purchase, &nonce, &seed, 1000, &[]));
        }
        assert!(distinct.len() > 1, "20 distinct purchases all mapped to the same slot");
    }
}
