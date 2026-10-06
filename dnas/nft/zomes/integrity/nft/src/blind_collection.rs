//! G3a: blind fixed-inventory batches.
//!
//! ```text
//! BlindBatchCommitment ── batch_id = its Create ActionHash
//!   └─ N sealed NftManifest (batch, batch_slot = i)
//!        └─ InventoryFinalized   nfts[i] == the NFT for slot i
//!             └─ BlindPurchaseCommitment × any   (cite the finalization)
//!                  └─ BatchClose  (1 <= K <= N purchases, sorted, minter-authored)
//!                       └─ RandomnessResult   (bound to the close)
//!                            └─ deterministic partial Fisher–Yates
//!                                 └─ BatchRelease in the first TransferCommitment
//! ```
//!
//! What this establishes, and what it does not:
//!
//! * The finalized inventory is one immutable slot -> NFT list. Its uniqueness
//!   is enforced by walking the batch author's OWN chain, so it holds on an
//!   honest linear chain. A forked author chain can present two finalizations;
//!   Holochain can warrant a fork, but `validate()` of one branch cannot see the
//!   other. There is no global (cross-author) uniqueness.
//! * A `BatchClose` commits ONE immutable purchase set. It does NOT prove that
//!   every economically eligible purchase was included: purchases live on
//!   independent buyer chains and validators cannot enumerate the DHT. The
//!   minter can omit purchases; refunds are a payment-layer concern.
//! * Assignment is a pure function of (inventory, sorted purchases, randomness).
//!   Nothing depends on resolution order, and nothing uses timestamps.
//! * For `K < N`, the `N - K` unselected NFTs simply have no release path.
//!
//! The randomness verifier in G3a is DEV-ONLY and INSECURE (see `randomness.rs`).

use crate::canonical::{preimage, put_action_hash, put_lp, put_u32};
use crate::dna_properties::CollectionProperties;
use crate::nft_manifest::{
    commit_hash, is_create_of, is_nft_manifest_create, NftManifest, NftPayload,
};
use crate::randomness::RandomnessResult;
use crate::UnitEntryTypes;
use hdi::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

/// Hard protocol limit on batch size (full-list inventory commitment).
pub const MAX_BATCH_SIZE: u32 = 1024;
/// The only assignment algorithm version.
pub const ASSIGNMENT_ALGORITHM_V1: u32 = 1;

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// Batch parameters. Immutable. Its Create `ActionHash` is the batch identity.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindBatchCommitment {
    /// N, the committed inventory size. `1..=MAX_BATCH_SIZE`.
    pub batch_size: u32,
    /// ADVISORY ONLY (UI). Never used by validation to close or release.
    #[serde(default)]
    pub sale_window_secs: u64,
    /// Must equal `ASSIGNMENT_ALGORITHM_V1`.
    pub assignment_algorithm_version: u32,
}

/// The authoritative, immutable slot -> NFT binding. Created after the N sealed
/// NFTs exist (their hashes cannot be in the batch commitment: circular).
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct InventoryFinalized {
    pub batch: ActionHash,
    /// `nfts[i]` is the NFT for slot `i`.
    pub nfts: Vec<ActionHash>,
    /// Derived from `nfts` (see `inventory_digest`); validators recompute it.
    pub inventory_digest: [u8; 32],
}

/// Authored by a buyer (the author IS the buyer). Cites the finalized inventory,
/// so a purchase cannot exist before the inventory is fixed.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindPurchaseCommitment {
    pub batch: ActionHash,
    /// The `InventoryFinalized` this purchase is for.
    pub inventory: ActionHash,
}

/// Minter-authored, one per batch. Commits the (sorted) purchase set.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BatchClose {
    pub batch: ActionHash,
    pub inventory: ActionHash,
    /// Strictly ascending by raw 39-byte `ActionHash`; `1 <= K <= N`.
    pub purchases: Vec<ActionHash>,
}

/// Anyone may author this once a purchase's resolution window has passed. A
/// UI/payment SIGNAL only: nothing on-chain consumes it and no release depends
/// on it. Unchanged from the legacy protocol.
#[derive(Clone, PartialEq)]
#[hdk_entry_helper]
pub struct BlindPurchaseAbandoned {
    pub purchase: ActionHash,
}

/// Evidence carried by the first `TransferCommitment` of a batch NFT.
/// Validators reconstruct the assignment from it; nothing here is trusted.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct BatchRelease {
    pub close: ActionHash,
    pub result: ActionHash,
    pub purchase: ActionHash,
}

// ---------------------------------------------------------------------------
// Pure canonical hashing: inventory
// ---------------------------------------------------------------------------

/// ```text
/// put("hnft.inventory.leaf") ‖ u32_le(1) ‖ collection_id[32] ‖ batch[39]
/// ‖ u32_le(slot) ‖ nft[39] ‖ metadata_hash[32]
/// ```
pub fn inventory_leaf(
    collection_id: &[u8; 32],
    batch: &ActionHash,
    slot: u32,
    nft: &ActionHash,
    metadata_hash: &[u8; 32],
) -> [u8; 32] {
    let mut buf = preimage("hnft.inventory.leaf", 1);
    buf.extend_from_slice(collection_id);
    put_action_hash(&mut buf, batch);
    put_u32(&mut buf, slot);
    put_action_hash(&mut buf, nft);
    buf.extend_from_slice(metadata_hash);
    commit_hash(&buf)
}

/// ```text
/// put("hnft.inventory.digest") ‖ u32_le(1) ‖ collection_id[32] ‖ batch[39]
/// ‖ u32_le(N) ‖ leaf_0 ‖ … ‖ leaf_{N-1}
/// ```
pub fn inventory_digest(
    collection_id: &[u8; 32],
    batch: &ActionHash,
    leaves: &[[u8; 32]],
) -> [u8; 32] {
    let mut buf = preimage("hnft.inventory.digest", 1);
    buf.extend_from_slice(collection_id);
    put_action_hash(&mut buf, batch);
    put_u32(&mut buf, leaves.len() as u32);
    for leaf in leaves {
        buf.extend_from_slice(leaf);
    }
    commit_hash(&buf)
}

// ---------------------------------------------------------------------------
// Pure canonical hashing + algorithm: assignment
// ---------------------------------------------------------------------------

/// ```text
/// put("hnft.assignment.seed") ‖ u32_le(1) ‖ collection_id[32] ‖ batch[39]
/// ‖ close[39] ‖ put(provider_id) ‖ put(event_id) ‖ randomness[32]
/// ```
pub fn assignment_seed(
    collection_id: &[u8; 32],
    batch: &ActionHash,
    close: &ActionHash,
    provider_id: &str,
    event_id: &[u8],
    randomness: &[u8; 32],
) -> [u8; 32] {
    let mut buf = preimage("hnft.assignment.seed", 1);
    buf.extend_from_slice(collection_id);
    put_action_hash(&mut buf, batch);
    put_action_hash(&mut buf, close);
    put_lp(&mut buf, provider_id.as_bytes());
    put_lp(&mut buf, event_id);
    buf.extend_from_slice(randomness);
    commit_hash(&buf)
}

/// Deterministic word stream: `word_j = u64_le(blake2b_256(seed ‖ u32_le(j))[0..8])`.
/// `counter` advances on EVERY draw, including rejected ones.
pub struct WordStream<'a> {
    seed: &'a [u8; 32],
    pub counter: u32,
}

impl<'a> WordStream<'a> {
    pub fn new(seed: &'a [u8; 32]) -> Self {
        Self { seed, counter: 0 }
    }

    fn word(&mut self) -> u64 {
        let mut buf = Vec::with_capacity(36);
        buf.extend_from_slice(self.seed);
        buf.extend_from_slice(&self.counter.to_le_bytes());
        self.counter += 1;
        let h = commit_hash(&buf);
        u64::from_le_bytes(h[..8].try_into().expect("8 bytes"))
    }

    /// Unbiased integer in `[0, m)` by rejection. `m == 1` returns 0 WITHOUT a
    /// draw. `m` must be `>= 1`.
    pub fn uniform(&mut self, m: u64) -> u64 {
        if m == 1 {
            return 0;
        }
        // 2^64 mod m
        let excess = 0u64.wrapping_sub(m) % m;
        let limit = (1u128 << 64) - excess as u128;
        loop {
            let w = self.word();
            if (w as u128) < limit {
                return w % m;
            }
        }
    }
}

/// Seeded partial Fisher–Yates over `[0, n)`: the slots for positions `0..k`.
/// Position `j` receives `perm[j]` after `swap(perm[j], perm[j + uniform(n - j)])`.
/// The output is injective (a prefix of a permutation), and the value at
/// position `j` does not depend on `k`.
pub fn assign_slots(seed: &[u8; 32], n: u32, k: u32) -> Result<Vec<u32>, String> {
    if k > n {
        return Err("cannot select more slots than the inventory holds".into());
    }
    let mut rng = WordStream::new(seed);
    let mut perm: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(k as usize);
    for j in 0..k {
        let r = j + rng.uniform((n - j) as u64) as u32;
        let at_j = *perm.get(&j).unwrap_or(&j);
        let at_r = *perm.get(&r).unwrap_or(&r);
        perm.insert(j, at_r);
        perm.insert(r, at_j);
        out.push(at_r);
    }
    Ok(out)
}

/// The slot for the purchase at sorted position `position`.
pub fn slot_for_position(seed: &[u8; 32], n: u32, position: u32) -> Result<u32, String> {
    if position >= n {
        return Err("position is outside the inventory".into());
    }
    Ok(*assign_slots(seed, n, position + 1)?
        .last()
        .expect("k >= 1 yields at least one slot"))
}

/// Canonical purchase order: ascending raw 39-byte `ActionHash`.
pub fn canonical_purchase_order(mut purchases: Vec<ActionHash>) -> Vec<ActionHash> {
    purchases.sort_by(|a, b| a.get_raw_39().cmp(b.get_raw_39()));
    purchases
}

/// Strictly ascending by raw 39 bytes (so also duplicate-free).
pub fn purchases_strictly_ascending(purchases: &[ActionHash]) -> bool {
    purchases
        .windows(2)
        .all(|w| w[0].get_raw_39() < w[1].get_raw_39())
}

pub fn check_batch_params(batch_size: u32, algorithm_version: u32) -> Result<(), String> {
    if batch_size == 0 || batch_size > MAX_BATCH_SIZE {
        return Err(format!("batch_size must be 1..={MAX_BATCH_SIZE}"));
    }
    if algorithm_version != ASSIGNMENT_ALGORITHM_V1 {
        return Err("unsupported assignment_algorithm_version".into());
    }
    Ok(())
}

/// Shape rules for a close's purchase list (pure part of the validator).
pub fn check_close_shape(purchases: &[ActionHash], batch_size: u32) -> Result<(), String> {
    let k = purchases.len();
    if k == 0 || k as u64 > batch_size as u64 {
        return Err("a BatchClose needs 1 <= K <= batch_size purchases".into());
    }
    if !purchases_strictly_ascending(purchases) {
        return Err("close purchases must be strictly ascending by raw ActionHash".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Loaders (type-checked: msgpack decoding alone can confuse look-alike entries)
// ---------------------------------------------------------------------------

/// Load a record that must be the CREATE of the given entry type. Decoding alone
/// is not enough: e.g. a `BatchClose` decodes as a `BlindPurchaseCommitment`
/// (extra fields are ignored), so the action's entry type is checked first.
fn load_entry<T>(hash: &ActionHash, unit: UnitEntryTypes, what: &str) -> ExternResult<(Record, T)>
where
    T: TryFrom<SerializedBytes, Error = SerializedBytesError>,
{
    let record = must_get_valid_record(hash.clone())?;
    if !is_create_of(record.action(), unit) {
        return Err(wasm_error!(WasmErrorInner::Guest(format!(
            "referenced action is not the Create of a {what}"
        ))));
    }
    let entry: T = record
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(format!(
            "referenced action is not a {what}"
        ))))?;
    Ok((record, entry))
}

pub fn load_batch_commitment(hash: &ActionHash) -> ExternResult<(Record, BlindBatchCommitment)> {
    load_entry(
        hash,
        UnitEntryTypes::BlindBatchCommitment,
        "BlindBatchCommitment",
    )
}

pub fn load_inventory(hash: &ActionHash) -> ExternResult<(Record, InventoryFinalized)> {
    load_entry(
        hash,
        UnitEntryTypes::InventoryFinalized,
        "InventoryFinalized",
    )
}

pub fn load_purchase_commitment(
    hash: &ActionHash,
) -> ExternResult<(Record, BlindPurchaseCommitment)> {
    load_entry(
        hash,
        UnitEntryTypes::BlindPurchaseCommitment,
        "BlindPurchaseCommitment",
    )
}

pub fn load_batch_close(hash: &ActionHash) -> ExternResult<(Record, BatchClose)> {
    load_entry(hash, UnitEntryTypes::BatchClose, "BatchClose")
}

pub fn load_randomness_result(hash: &ActionHash) -> ExternResult<(Record, RandomnessResult)> {
    load_entry(hash, UnitEntryTypes::RandomnessResult, "RandomnessResult")
}

/// `Invalid` early-return helper.
macro_rules! invalid {
    ($($arg:tt)*) => {
        return Ok(ValidateCallbackResult::Invalid(format!($($arg)*)))
    };
}

fn collection_id_or_invalid() -> ExternResult<Result<[u8; 32], String>> {
    let props = CollectionProperties::get()?;
    Ok(match props.collection_id_bytes_checked() {
        Ok(id) => Ok(id),
        Err(_) => Err("this collection needs a valid collection_id (64 lowercase hex)".into()),
    })
}

// ---------------------------------------------------------------------------
// Validation: entries
// ---------------------------------------------------------------------------

pub fn validate_create_blind_batch_commitment(
    action: TypedAction<EntryCreationData>,
    entry: BlindBatchCommitment,
) -> ExternResult<ValidateCallbackResult> {
    if let Err(e) = check_batch_params(entry.batch_size, entry.assignment_algorithm_version) {
        invalid!("{e}");
    }
    let props = CollectionProperties::get()?;
    if let Some(minter) = CollectionProperties::minter()? {
        if action.author() != &minter {
            invalid!("only the collection minter may create a blind batch");
        }
    }
    if props.generation.is_some() {
        invalid!("blind batches cannot exist in a generative collection");
    }
    if let Err(e) = collection_id_or_invalid()? {
        invalid!("{e}");
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Rules for an `NftManifest` that claims batch membership (called from the
/// manifest's own create validator). Mint-time uniqueness is deliberately NOT
/// attempted: twins may be minted, but only NFTs listed in `InventoryFinalized`
/// are usable by the batch protocol.
pub fn validate_manifest_batch_fields(
    action_author: &AgentPubKey,
    batch: &Option<ActionHash>,
    batch_slot: &Option<u32>,
) -> ExternResult<ValidateCallbackResult> {
    match (batch, batch_slot) {
        (None, None) => Ok(ValidateCallbackResult::Valid),
        (Some(b), Some(slot)) => {
            if CollectionProperties::get()?.generation.is_some() {
                invalid!("batch NFTs cannot exist in a generative collection");
            }
            let (batch_rec, batch) = load_batch_commitment(b)?;
            if batch_rec.action().author() != action_author {
                invalid!("a batch NFT must be minted by its batch's author");
            }
            if *slot >= batch.batch_size {
                invalid!("batch_slot is outside the batch");
            }
            Ok(ValidateCallbackResult::Valid)
        }
        _ => Ok(ValidateCallbackResult::Invalid(
            "batch and batch_slot must be both set or both unset".into(),
        )),
    }
}

pub fn validate_create_inventory_finalized(
    action: TypedAction<EntryCreationData>,
    entry: InventoryFinalized,
) -> ExternResult<ValidateCallbackResult> {
    let (batch_rec, batch) = load_batch_commitment(&entry.batch)?;
    if batch_rec.action().author() != action.author() {
        invalid!("only the batch's author may finalize its inventory");
    }
    if entry.nfts.len() as u64 != batch.batch_size as u64 {
        invalid!("nfts.len() must equal the batch's batch_size");
    }
    let collection_id = match collection_id_or_invalid()? {
        Ok(id) => id,
        Err(e) => invalid!("{e}"),
    };

    let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut leaves: Vec<[u8; 32]> = Vec::with_capacity(entry.nfts.len());
    for (i, nft_hash) in entry.nfts.iter().enumerate() {
        if !seen.insert(nft_hash.get_raw_39().to_vec()) {
            invalid!("duplicate NFT in the inventory");
        }
        let rec = must_get_valid_record(nft_hash.clone())?;
        // The ORIGINAL Create, never an Update: a reveal Update carries the same
        // batch/slot, so entry content alone would not distinguish it.
        if !is_nft_manifest_create(rec.action()) {
            invalid!("slot {i}: listed action is not the original NftManifest Create");
        }
        if rec.action().author() != action.author() {
            invalid!("slot {i}: NFT was not minted by the batch author");
        }
        let manifest: NftManifest = rec
            .entry()
            .to_app_option()
            .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "listed action has no NftManifest".into()
            )))?;
        if !matches!(manifest.payload, NftPayload::Sealed) {
            invalid!("slot {i}: NFT is not sealed");
        }
        if manifest.batch.as_ref() != Some(&entry.batch) {
            invalid!("slot {i}: NFT belongs to a different batch");
        }
        if manifest.batch_slot != Some(i as u32) {
            invalid!("slot {i}: NFT's batch_slot does not match its position");
        }
        if manifest.metadata_hash == [0u8; 32] {
            invalid!("slot {i}: NFT metadata_hash is zero");
        }
        leaves.push(inventory_leaf(
            &collection_id,
            &entry.batch,
            i as u32,
            nft_hash,
            &manifest.metadata_hash,
        ));
    }
    if inventory_digest(&collection_id, &entry.batch, &leaves) != entry.inventory_digest {
        invalid!("inventory_digest does not match the listed NFTs");
    }

    // Uniqueness on the author's OWN chain (honest linear chain; see module doc).
    let prev = action
        .prev_action()
        .ok_or_else(|| {
            wasm_error!(WasmErrorInner::Guest(
                "finalization has no prev_action".into()
            ))
        })?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    for a in activity.iter() {
        if is_create_of(a.action.action(), UnitEntryTypes::InventoryFinalized) {
            let (_, other) = load_inventory(a.action.as_hash())?;
            if other.batch == entry.batch {
                invalid!("this batch already has a finalized inventory");
            }
        }
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_blind_purchase_commitment(
    _action: TypedAction<EntryCreationData>,
    entry: BlindPurchaseCommitment,
) -> ExternResult<ValidateCallbackResult> {
    load_batch_commitment(&entry.batch)?;
    let (_, inventory) = load_inventory(&entry.inventory)?;
    if inventory.batch != entry.batch {
        invalid!("purchase's inventory belongs to a different batch");
    }
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_create_batch_close(
    action: TypedAction<EntryCreationData>,
    entry: BatchClose,
) -> ExternResult<ValidateCallbackResult> {
    let (batch_rec, batch) = load_batch_commitment(&entry.batch)?;
    if batch_rec.action().author() != action.author() {
        invalid!("only the batch's author may close it");
    }
    let (_, inventory) = load_inventory(&entry.inventory)?;
    if inventory.batch != entry.batch {
        invalid!("close cites an inventory that belongs to a different batch");
    }
    // NOTE: 1 <= K <= N (an intentional relaxation of "K == N"). No timestamps.
    if let Err(e) = check_close_shape(&entry.purchases, batch.batch_size) {
        invalid!("{e}");
    }
    for p in entry.purchases.iter() {
        let (_, purchase) = load_purchase_commitment(p)?;
        if purchase.batch != entry.batch {
            invalid!("a listed purchase belongs to a different batch");
        }
        if purchase.inventory != entry.inventory {
            invalid!("a listed purchase cites a different inventory");
        }
    }

    // One close per batch on the author's own chain.
    let prev = action
        .prev_action()
        .ok_or_else(|| wasm_error!(WasmErrorInner::Guest("close has no prev_action".into())))?
        .clone();
    let activity = must_get_agent_activity(action.author().clone(), ChainFilter::new(prev))?;
    for a in activity.iter() {
        if is_create_of(a.action.action(), UnitEntryTypes::BatchClose) {
            let (_, other) = load_batch_close(a.action.as_hash())?;
            if other.batch == entry.batch {
                invalid!("this batch is already closed");
            }
        }
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
        invalid!("cannot mark a purchase abandoned before the resolution window closes");
    }
    Ok(ValidateCallbackResult::Valid)
}

// ---------------------------------------------------------------------------
// Validation: BatchRelease (called from the transfer-commitment validator)
// ---------------------------------------------------------------------------

/// Reconstruct `NFT -> slot -> inventory -> close -> result -> assignment ->
/// purchase` and require `to_agent` to be that purchase's author. Trusts nothing
/// UI-supplied: every referenced record is fetched and re-validated.
pub fn validate_batch_release(
    nft: &ActionHash,
    to_agent: &AgentPubKey,
    manifest: &NftManifest,
    release: &BatchRelease,
) -> ExternResult<ValidateCallbackResult> {
    let (Some(batch_hash), Some(slot)) = (manifest.batch.clone(), manifest.batch_slot) else {
        invalid!("release cited for an NFT that is not in a batch");
    };
    let (_, batch) = load_batch_commitment(&batch_hash)?;
    let (_, close) = load_batch_close(&release.close)?;
    if close.batch != batch_hash {
        invalid!("close belongs to a different batch");
    }
    let (_, inventory) = load_inventory(&close.inventory)?;
    if inventory.batch != batch_hash {
        invalid!("close's inventory belongs to a different batch");
    }
    if inventory.nfts.get(slot as usize) != Some(nft) {
        invalid!("this NFT is not the committed NFT for its slot");
    }
    let (_, result) = load_randomness_result(&release.result)?;
    if result.close != release.close {
        invalid!("randomness result is bound to a different close");
    }
    let Some(position) = close.purchases.iter().position(|p| p == &release.purchase) else {
        invalid!("purchase is not in the close");
    };
    let collection_id = match collection_id_or_invalid()? {
        Ok(id) => id,
        Err(e) => invalid!("{e}"),
    };
    let seed = assignment_seed(
        &collection_id,
        &batch_hash,
        &release.close,
        &result.provider_id,
        &result.event_id,
        &result.randomness,
    );
    let assigned = match slot_for_position(&seed, batch.batch_size, position as u32) {
        Ok(s) => s,
        Err(e) => invalid!("{e}"),
    };
    if assigned != slot {
        invalid!("purchase is assigned slot {assigned}, not this NFT's slot {slot}");
    }
    let (purchase_rec, _) = load_purchase_commitment(&release.purchase)?;
    if purchase_rec.action().author() != to_agent {
        invalid!("transfer recipient is not the assigned buyer");
    }
    Ok(ValidateCallbackResult::Valid)
}

// ---------------------------------------------------------------------------
// Tests: pure functions and golden vectors (expected values computed
// independently with Python hashlib.blake2b; see docs/blind-box-collections.md)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    fn ah(prefix_byte: u8, loc: u8) -> ActionHash {
        let mut raw = vec![0x84, 0x29, 0x24];
        raw.extend_from_slice(&[prefix_byte; 32]);
        raw.extend_from_slice(&[0, 0, 0, loc]);
        ActionHash::from_raw_39(raw)
    }
    fn batch() -> ActionHash {
        ah(0xB0, 1)
    }
    fn nft(i: u8) -> ActionHash {
        ah(0xA0 + i, i)
    }
    const CID: [u8; 32] = [0xC0; 32];

    fn leaf(i: u8) -> [u8; 32] {
        inventory_leaf(&CID, &batch(), i as u32, &nft(i), &[0x30 + i; 32])
    }

    #[test]
    fn inventory_leaf_golden_vectors() {
        assert_eq!(
            hex(&leaf(0)),
            "473ac37f6831f7bad8bc0f2dac4f409422a1c7744b81a4047e1f6da82db01b98"
        );
        assert_eq!(
            hex(&leaf(1)),
            "7aae4e39becebe53bac972d923789b2da2c386b8ff6ba792a01869b5062f6e94"
        );
    }

    #[test]
    fn inventory_digest_golden_vectors() {
        assert_eq!(
            hex(&inventory_digest(&CID, &batch(), &[leaf(0), leaf(1)])),
            "5761aa62285638f6a9fe55c610ddc648bd84276635fa92a7b9136d1a7050a78c"
        );
        assert_eq!(
            hex(&inventory_digest(&CID, &batch(), &[leaf(0)])),
            "d4d4f4c6a51caa85b4017a9c1b2b7cb6df081fedde62cdfe930aab413e984a29"
        );
    }

    #[test]
    fn inventory_hashes_bind_every_field() {
        let base = leaf(0);
        assert_ne!(
            base,
            inventory_leaf(&[0xC1; 32], &batch(), 0, &nft(0), &[0x30; 32])
        );
        assert_ne!(
            base,
            inventory_leaf(&CID, &ah(0xB1, 1), 0, &nft(0), &[0x30; 32])
        );
        assert_ne!(
            base,
            inventory_leaf(&CID, &batch(), 1, &nft(0), &[0x30; 32])
        );
        assert_ne!(
            base,
            inventory_leaf(&CID, &batch(), 0, &nft(1), &[0x30; 32])
        );
        assert_ne!(
            base,
            inventory_leaf(&CID, &batch(), 0, &nft(0), &[0x31; 32])
        );
        // Order of leaves matters.
        assert_ne!(
            inventory_digest(&CID, &batch(), &[leaf(0), leaf(1)]),
            inventory_digest(&CID, &batch(), &[leaf(1), leaf(0)])
        );
    }

    fn vector_seed() -> [u8; 32] {
        assignment_seed(
            &CID,
            &batch(),
            &ah(0xD0, 2),
            "dev-test",
            b"event-1",
            &[0x5A; 32],
        )
    }

    #[test]
    fn assignment_seed_golden_vector() {
        assert_eq!(
            hex(&vector_seed()),
            "fb7e64d9aa2fe672ae3fc9316b32f41d8e43e25263753cf030f8ce2a8c30b568"
        );
    }

    #[test]
    fn assignment_golden_vectors() {
        assert_eq!(assign_slots(&vector_seed(), 5, 3).unwrap(), vec![4, 0, 3]);
        assert_eq!(
            assign_slots(&vector_seed(), 5, 5).unwrap(),
            vec![4, 0, 3, 1, 2]
        );
    }

    #[test]
    fn assignment_is_deterministic_injective_and_prefix_stable() {
        let seed = vector_seed();
        for n in [1u32, 2, 5, 17, 64] {
            for k in 1..=n {
                let a = assign_slots(&seed, n, k).unwrap();
                assert_eq!(a, assign_slots(&seed, n, k).unwrap());
                assert_eq!(a.len(), k as usize, "exactly K slots");
                let set: BTreeSet<_> = a.iter().collect();
                assert_eq!(set.len(), a.len(), "no duplicate slots");
                assert!(a.iter().all(|s| *s < n), "slots within 0..N");
                // The slot at position j does not depend on K.
                let full = assign_slots(&seed, n, n).unwrap();
                assert_eq!(a, full[..k as usize].to_vec());
            }
        }
    }

    #[test]
    fn k_equals_one_and_k_equals_n() {
        let seed = vector_seed();
        assert_eq!(assign_slots(&seed, 5, 1).unwrap(), vec![4]);
        let full = assign_slots(&seed, 10, 10).unwrap();
        let mut sorted = full.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            (0..10).collect::<Vec<u32>>(),
            "K=N is a full permutation"
        );
        assert_eq!(assign_slots(&seed, 1, 1).unwrap(), vec![0]);
        assert!(assign_slots(&seed, 3, 4).is_err(), "K > N is rejected");
        assert_eq!(slot_for_position(&seed, 5, 2).unwrap(), 3);
        assert!(slot_for_position(&seed, 5, 5).is_err());
    }

    #[test]
    fn assignment_ignores_input_order_once_canonically_sorted() {
        let ps: Vec<ActionHash> = (1..=6).map(|i| ah(0x10 * i, i)).collect();
        let mut reversed = ps.clone();
        reversed.reverse();
        let mut rotated = ps.clone();
        rotated.rotate_left(2);
        let a = canonical_purchase_order(ps.clone());
        assert_eq!(a, canonical_purchase_order(reversed));
        assert_eq!(a, canonical_purchase_order(rotated));
        assert!(purchases_strictly_ascending(&a));
        // And so the buyer -> slot map is identical whatever order they arrived in.
        let seed = vector_seed();
        let slots = assign_slots(&seed, 8, a.len() as u32).unwrap();
        let map: BTreeMap<Vec<u8>, u32> = a
            .iter()
            .zip(slots.iter())
            .map(|(p, s)| (p.get_raw_39().to_vec(), *s))
            .collect();
        assert_eq!(map.len(), 6);
    }

    #[test]
    fn rejection_sampling_counts_every_draw() {
        // m just above 2^63: roughly half of all words are rejected.
        let m = (1u64 << 63) + 1;
        let excess = (1u128 << 64) % m as u128;
        let limit = (1u128 << 64) - excess;
        let mut saw_rejection = false;
        for s in 0u8..64 {
            let seed = [s; 32];
            // Independent recomputation of the stream.
            let mut expected_counter = 0u32;
            let expected = loop {
                let mut buf = seed.to_vec();
                buf.extend_from_slice(&expected_counter.to_le_bytes());
                expected_counter += 1;
                let h = commit_hash(&buf);
                let w = u64::from_le_bytes(h[..8].try_into().unwrap());
                if (w as u128) < limit {
                    break w % m;
                }
            };
            let mut rng = WordStream::new(&seed);
            assert_eq!(rng.uniform(m), expected);
            assert_eq!(
                rng.counter, expected_counter,
                "counter advances on rejected draws too"
            );
            saw_rejection |= expected_counter > 1;
        }
        assert!(
            saw_rejection,
            "test must exercise at least one rejected draw"
        );
    }

    #[test]
    fn uniform_of_one_draws_nothing() {
        let seed = [9u8; 32];
        let mut rng = WordStream::new(&seed);
        assert_eq!(rng.uniform(1), 0);
        assert_eq!(rng.counter, 0);
    }

    #[test]
    fn batch_size_and_version_bounds() {
        assert!(check_batch_params(1, 1).is_ok());
        assert!(check_batch_params(1024, 1).is_ok());
        assert!(check_batch_params(0, 1).is_err());
        assert!(check_batch_params(1025, 1).is_err());
        assert!(check_batch_params(10, 0).is_err());
        assert!(check_batch_params(10, 2).is_err());
    }

    #[test]
    fn close_shape_rules() {
        let a = ah(0x01, 1);
        let b = ah(0x02, 2);
        let c = ah(0x03, 3);
        // K = 1, K < N, K = N are all valid.
        assert!(check_close_shape(std::slice::from_ref(&a), 10).is_ok());
        assert!(check_close_shape(&[a.clone(), b.clone(), c.clone()], 10).is_ok());
        assert!(check_close_shape(&[a.clone(), b.clone(), c.clone()], 3).is_ok());
        // K = 0, K > N, unsorted, duplicated.
        assert!(check_close_shape(&[], 10).is_err());
        assert!(check_close_shape(&[a.clone(), b.clone(), c.clone()], 2).is_err());
        assert!(check_close_shape(&[b.clone(), a.clone()], 10).is_err());
        assert!(check_close_shape(&[a.clone(), a.clone()], 10).is_err());
    }
}
