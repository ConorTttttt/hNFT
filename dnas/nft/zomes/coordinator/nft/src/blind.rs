//! Coordinator externs for blind-box collection batches. See
//! `integrity/nft/src/blind_collection.rs` and its DESIGN.md /
//! HANDOFF.md for the full mechanism and the demo-scope decisions this
//! implementation makes:
//!
//! - Resolution order: minter-controlled order is an ACCEPTED TRUST
//!   ASSUMPTION for this demo (no closed-batch redesign). See DESIGN.md
//!   "Resolution ordering."
//! - `BlindPurchaseAbandoned` is a UI-only signal here -- nothing in this
//!   file consumes it. The UI should surface "unresolved, contact seller"
//!   when one exists for a buyer's own purchase; no on-chain dispute/refund
//!   path is implemented. **This is a known gap the README for this
//!   feature needs to call out explicitly** (see HANDOFF.md's liveness
//!   decision) -- flagging here so it isn't lost.
//! - The minter's X25519 public key is baked into `CollectionProperties`
//!   (`minter_x25519_pubkey`, hex-encoded) rather than published as a
//!   separate entry.
//!
//! Payment verification follows the exact same demo trust model
//! `payment.rs` already established: the caller (minter's own UI) supplies
//! the settlement's `computed_values` fields directly, and this only
//! checks internal consistency against this specific purchase -- it does
//! NOT re-fetch or verify the RAVE from the Unyt cell. See `payment.rs`'s
//! module doc for the full trust discussion; it applies unchanged here.

use crate::RemoteSignal;
use hdk::prelude::*;
use nft_integrity::*;

// ---------------------------------------------------------------------------
// Hex encode/decode -- CollectionProperties carries the minter's X25519 key
// as a plain hex string (no base64 crate dependency needed for 32 bytes).
// ---------------------------------------------------------------------------

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> ExternResult<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "invalid hex: odd length".into()
        )));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("invalid hex: {e}"))))
        })
        .collect()
}

fn minter_x25519_pubkey() -> ExternResult<X25519PubKey> {
    let hex = CollectionProperties::get()?
        .minter_x25519_pubkey
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "this collection has no minter_x25519_pubkey configured -- required for blind-box sales"
                .into()
        )))?;
    X25519PubKey::try_from(from_hex(&hex)?)
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("bad minter_x25519_pubkey: {e:?}"))))
}

/// One-time setup helper: the minter runs this once, then pastes the
/// returned hex string into this collection's `dna.yaml`
/// `modifiers.properties.minter_x25519_pubkey` BEFORE cloning/creating the
/// cell that will run blind-box sales. The matching secret stays in this
/// agent's own lair keystore -- never returned, never leaves the
/// conductor.
#[hdk_extern]
pub fn generate_minter_x25519_pubkey(_: ()) -> ExternResult<String> {
    let pubkey = create_x25519_keypair()?;
    Ok(to_hex(pubkey.as_ref()))
}

// ---------------------------------------------------------------------------
// Nonce transport: what actually rides in `BlindPurchaseCommitment.encrypted_nonce`.
// Needs both the buyer's one-time X25519 sender pubkey (so the minter can
// box-decrypt) and the ciphertext+nonce libsodium produced. validate() never
// looks inside this -- only length-bounds it. See DESIGN.md "Nonce delivery."
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
struct NonceTransport {
    sender_pubkey: X25519PubKey,
    encrypted: XSalsa20Poly1305EncryptedData,
}

fn encode_transport(t: &NonceTransport) -> ExternResult<Vec<u8>> {
    holochain_serialized_bytes::encode(t)
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))
}

fn decode_transport(bytes: &[u8]) -> ExternResult<NonceTransport> {
    holochain_serialized_bytes::decode(bytes)
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))
}

// ---------------------------------------------------------------------------
// mint_blind_batch (minter)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
pub struct MintBlindBatchInput {
    /// `metadata_hashes[i]` is the commitment for slot `i`. The minter's
    /// secret pre-shuffle (which real attributes go with which slot) is
    /// implicit in this ordering -- never written down anywhere validate()
    /// can see. See DESIGN.md step 1.
    pub metadata_hashes: Vec<[u8; 32]>,
    /// The minter's secret. Restated in full on every `BlindAssignmentReveal`
    /// for this batch later -- kept off-chain by the minter's own client
    /// until then, same as `mint_with_agreement`'s reveal secrets.
    pub mint_seed: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MintBlindBatchOutput {
    pub batch: ActionHash,
    /// Sealed mint action hashes, in slot order (`nfts[i]` is slot `i`).
    pub nfts: Vec<ActionHash>,
}

#[hdk_extern]
pub fn mint_blind_batch(input: MintBlindBatchInput) -> ExternResult<MintBlindBatchOutput> {
    if input.metadata_hashes.is_empty() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "batch_size must be greater than zero".into()
        )));
    }
    let batch_size = input.metadata_hashes.len() as u32;
    let seed_hash = commit_hash(&input.mint_seed);

    let batch_hash = create_entry(&EntryTypes::BlindBatchCommitment(BlindBatchCommitment {
        batch_size,
        seed_hash,
    }))?;

    let path = Path::from(crate::COLLECTION_PATH);
    create_link(
        path.path_entry_hash()?,
        batch_hash.clone(),
        LinkTypes::PathToBlindBatch,
        (),
    )?;

    let mut nfts = Vec::with_capacity(input.metadata_hashes.len());
    for (slot, metadata_hash) in input.metadata_hashes.into_iter().enumerate() {
        let (nft_hash, _) = crate::mint::mint_manifest_in_batch(
            metadata_hash,
            Some(batch_hash.clone()),
            Some(slot as u32),
        )?;
        nfts.push(nft_hash);
    }

    Ok(MintBlindBatchOutput {
        batch: batch_hash,
        nfts,
    })
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
    /// The buyer's own secret. The buyer's client should hold onto this
    /// (same pattern as a reveal salt) so it can independently confirm the
    /// eventual `BlindAssignmentReveal.buyer_nonce` matches what they
    /// actually committed to.
    pub buyer_nonce: [u8; 32],
}

#[hdk_extern]
pub fn reserve_blind_purchase(
    input: ReserveBlindPurchaseInput,
) -> ExternResult<ReserveBlindPurchaseOutput> {
    let batch_rec = must_get_valid_record(input.batch.clone())?; // the batch is real
    let minter = batch_rec.action().author().clone();

    let nonce_bytes = random_bytes(32)?;
    let buyer_nonce: [u8; 32] = nonce_bytes.as_ref().try_into().map_err(|_| {
        wasm_error!(WasmErrorInner::Guest(
            "random_bytes did not return 32 bytes".into()
        ))
    })?;
    let nonce_hash = commit_hash(&buyer_nonce);

    // Encrypt buyer_nonce to the minter's X25519 key. The buyer's own
    // ephemeral keypair's secret half stays in lair; only its pubkey rides
    // in the transport so the minter can box-decrypt.
    let sender_pubkey = create_x25519_keypair()?;
    let recipient = minter_x25519_pubkey()?;
    let encrypted =
        x_25519_x_salsa20_poly1305_encrypt(sender_pubkey, recipient, buyer_nonce.to_vec().into())?;
    let encrypted_nonce = encode_transport(&NonceTransport {
        sender_pubkey,
        encrypted,
    })?;

    let purchase_hash = create_entry(&EntryTypes::BlindPurchaseCommitment(
        BlindPurchaseCommitment {
            batch: input.batch.clone(),
            nonce_hash,
            encrypted_nonce,
        },
    ))?;

    // Lets the minter discover this purchase against their own batch,
    // mirroring NftToListing's "is this listed" role.
    create_link(
        input.batch,
        purchase_hash.clone(),
        LinkTypes::BatchToPurchase,
        (),
    )?;

    // Best-effort nudge (not load-bearing -- BatchToPurchase is the durable
    // discovery path). Mirrors TransferOffered/ListingInterest.
    let _ = send_remote_signal(
        RemoteSignal::BlindPurchaseReserved {
            batch: batch_rec.action_address().clone(),
            purchase: purchase_hash.clone(),
        },
        vec![minter],
    );

    Ok(ReserveBlindPurchaseOutput {
        purchase: purchase_hash,
        buyer_nonce,
    })
}

/// Every blind-box batch ever minted in this collection, any minter.
/// Discovery only, mirrors `get_all_nfts` -- callers that need `batch_size`
/// or `seed_hash` should `decodeEntry` per item; this doesn't decode for
/// them, same as `get_all_nfts` returns raw mint records.
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

/// Purchases reserved against `batch`, any buyer -- the minter's own
/// resolution queue. Mirrors `interested_buyers`'s "seller's queue" role.
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

/// Purchases against `batch` that this (calling) minter has already
/// resolved -- an own-chain scan, same technique as `taken_slots_for_batch`.
/// Only meaningful when called by the batch's own minter; lets the UI hide
/// already-resolved purchases from the resolution queue instead of leaving
/// them there confusingly forever.
#[hdk_extern]
pub fn resolved_purchases_for_batch(batch: ActionHash) -> ExternResult<Vec<ActionHash>> {
    let records = query(ChainQueryFilter::new().include_entries(true))?;
    let mut resolved = Vec::new();
    for rec in records {
        let Ok(Some(reveal)) = rec.entry().to_app_option::<BlindAssignmentReveal>() else {
            continue;
        };
        if reveal.batch == batch {
            resolved.push(reveal.purchase);
        }
    }
    Ok(resolved)
}

/// Read back a `BlindPurchaseCommitment` (immutable — never updated, so
/// this is a plain `get`, unlike `get_listing`'s update-chain walk). Lets a
/// UI resolve the purchase's `batch` and its authoritative buyer (the
/// record's own author) without asking the minter to paste a buyer key by
/// hand.
#[hdk_extern]
pub fn get_blind_purchase(purchase: ActionHash) -> ExternResult<Option<Record>> {
    get(purchase, GetOptions::default())
}

// ---------------------------------------------------------------------------
// resolve_blind_assignment_from_rave (minter)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolveBlindAssignmentInput {
    pub purchase: ActionHash,
    /// The minter's own secret for this batch -- must match the
    /// `BlindBatchCommitment.seed_hash` this purchase's batch was created
    /// with.
    pub mint_seed: [u8; 32],
    /// The Unyt RAVE's own action hash. Not re-fetched here -- carried
    /// through only to record in the inbox link's tag. See `payment.rs`.
    pub rave: ActionHash,
    /// `output.computed_values.*` from that RAVE, AS THE UI READ THEM. Same
    /// demo trust model as `payment.rs::SettleFromRaveInput` -- trusted,
    /// not re-verified against the Unyt network.
    pub fulfilled: bool,
    /// Convention: `purchase.to_string()`, NOT an NFT hash -- nobody knows
    /// which NFT this purchase resolves to until this call. See DESIGN.md
    /// step 3.
    pub nft_id: String,
    pub new_owner: String,
    pub previous_owner: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResolveBlindAssignmentOutput {
    pub reveal: ActionHash,
    pub assigned_nft: ActionHash,
    pub commitment: ActionHash,
}

fn nft_id_for_purchase(purchase: &ActionHash) -> String {
    purchase.to_string()
}

fn check_claimed_blind_settlement(
    claim: &ResolveBlindAssignmentInput,
    minter: &AgentPubKey,
    buyer: &AgentPubKey,
) -> ExternResult<()> {
    if !claim.fulfilled {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement is not marked fulfilled".into()
        )));
    }
    if claim.nft_id != nft_id_for_purchase(&claim.purchase) {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's nft_id does not match this purchase".into()
        )));
    }
    if claim.new_owner != buyer.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's new_owner does not match this purchase's buyer".into()
        )));
    }
    if claim.previous_owner != minter.to_string() {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "claimed settlement's previous_owner does not match this batch's minter".into()
        )));
    }
    Ok(())
}

/// Own-chain scan (this agent IS the minter, so a local `query()` sees the
/// authoritative, already-ordered truth -- no network round trip needed,
/// same shape as `validate()`'s `must_get_agent_activity` walk, just from
/// the minter's own side). Re-derives each earlier reveal's consumed slot
/// from its `assigned_nft`, same as `validate()` does.
fn taken_slots_for_batch(batch: &ActionHash) -> ExternResult<Vec<u32>> {
    let records = query(ChainQueryFilter::new().include_entries(true))?;
    let mut taken = Vec::new();
    for rec in records {
        let Ok(Some(reveal)) = rec.entry().to_app_option::<BlindAssignmentReveal>() else {
            continue;
        };
        if &reveal.batch != batch {
            continue;
        }
        if let Some(nft_rec) = get(reveal.assigned_nft.clone(), GetOptions::default())? {
            if let Ok(Some(nft)) = nft_rec.entry().to_app_option::<NftManifest>() {
                if let Some(slot) = nft.batch_slot {
                    taken.push(slot);
                }
            }
        }
    }
    Ok(taken)
}

/// Find this minter's own Sealed mint in `batch` at `slot`, from their own
/// chain (they authored every mint in their own batch at `mint_blind_batch`
/// time).
fn find_sealed_nft_for_slot(batch: &ActionHash, slot: u32) -> ExternResult<ActionHash> {
    let records = query(ChainQueryFilter::new().include_entries(true))?;
    for rec in records {
        let Ok(Some(nft)) = rec.entry().to_app_option::<NftManifest>() else {
            continue;
        };
        if nft.batch.as_ref() == Some(batch) && nft.batch_slot == Some(slot) {
            return Ok(rec.action_address().clone());
        }
    }
    Err(wasm_error!(WasmErrorInner::Guest(
        "no sealed NFT found in this batch for the resolved slot".into()
    )))
}

/// Minter-side resolution: verifies the claimed Unyt settlement, decrypts
/// and checks `buyer_nonce`, calls the shared `resolve_slot` (never
/// reimplemented here), writes `BlindAssignmentReveal`, then offers the
/// resolved NFT to the buyer via the existing `TransferCommitment` flow
/// with `blind_reveal` set. See `INTEGRATION.md` section 2 for what
/// `validate()` enforces on that link.
#[hdk_extern]
pub fn resolve_blind_assignment_from_rave(
    input: ResolveBlindAssignmentInput,
) -> ExternResult<ResolveBlindAssignmentOutput> {
    let me = agent_info()?.agent_initial_pubkey;

    let purchase_rec = get(input.purchase.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("purchase not found".into())
    ))?;
    let purchase: BlindPurchaseCommitment = purchase_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a BlindPurchaseCommitment".into()
        )))?;
    let buyer = purchase_rec.action().author().clone();

    let batch_rec = get(purchase.batch.clone(), GetOptions::default())?.ok_or(wasm_error!(
        WasmErrorInner::Guest("batch not found".into())
    ))?;
    if batch_rec.action().author() != &me {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "caller is not this batch's own minter".into()
        )));
    }
    let batch: BlindBatchCommitment = batch_rec
        .entry()
        .to_app_option()
        .map_err(|e| wasm_error!(WasmErrorInner::Guest(format!("{e:?}"))))?
        .ok_or(wasm_error!(WasmErrorInner::Guest(
            "referenced action is not a BlindBatchCommitment".into()
        )))?;

    check_claimed_blind_settlement(&input, &me, &buyer)?;

    if commit_hash(&input.mint_seed) != batch.seed_hash {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "mint_seed does not match this batch's seed_hash".into()
        )));
    }

    // Decrypt and sanity-check buyer_nonce before spending the effort of an
    // on-chain write -- see DESIGN.md step 4.
    let transport = decode_transport(&purchase.encrypted_nonce)?;
    let recipient = minter_x25519_pubkey()?;
    let plaintext =
        x_25519_x_salsa20_poly1305_decrypt(recipient, transport.sender_pubkey, transport.encrypted)?
            .ok_or(wasm_error!(WasmErrorInner::Guest(
                "failed to decrypt buyer_nonce -- not addressed to this minter, or corrupted"
                    .into()
            )))?;
    let buyer_nonce: [u8; 32] = plaintext.as_ref().try_into().map_err(|_| {
        wasm_error!(WasmErrorInner::Guest(
            "decrypted buyer_nonce is not 32 bytes".into()
        ))
    })?;
    if commit_hash(&buyer_nonce) != purchase.nonce_hash {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "decrypted buyer_nonce does not match this purchase's nonce_hash".into()
        )));
    }

    let taken = taken_slots_for_batch(&purchase.batch)?;
    let slot = resolve_slot(
        &purchase.batch,
        &input.purchase,
        &buyer_nonce,
        &input.mint_seed,
        batch.batch_size,
        &taken,
    );
    if slot >= batch.batch_size {
        return Err(wasm_error!(WasmErrorInner::Guest(
            "batch is already fully assigned".into()
        )));
    }
    let assigned_nft = find_sealed_nft_for_slot(&purchase.batch, slot)?;

    let reveal_hash = create_entry(&EntryTypes::BlindAssignmentReveal(BlindAssignmentReveal {
        batch: purchase.batch.clone(),
        purchase: input.purchase.clone(),
        mint_seed: input.mint_seed,
        buyer_nonce,
        assigned_nft: assigned_nft.clone(),
    }))?;

    // First transfer for this NFT (minter -> buyer), same shape
    // `initiate_transfer`/`initiate_transfer_from_rave` already use, plus
    // `blind_reveal` so validate() binds `to_agent` to the purchase's own
    // author -- see INTEGRATION.md section 2.
    let commitment = TransferCommitment {
        nft: assigned_nft.clone(),
        to_agent: buyer.clone(),
        prev_state: assigned_nft.clone(),
        sequence: 1,
        blind_reveal: Some(reveal_hash.clone()),
    };
    let commitment_hash = create_entry(&EntryTypes::TransferCommitment(commitment))?;

    create_link(
        buyer.clone(),
        commitment_hash.clone(),
        LinkTypes::AgentToInboundCommitment,
        LinkTag::new(input.rave.clone().into_inner()),
    )?;

    let _ = send_remote_signal(
        RemoteSignal::TransferOffered {
            commitment: commitment_hash.clone(),
        },
        vec![buyer],
    );

    Ok(ResolveBlindAssignmentOutput {
        reveal: reveal_hash,
        assigned_nft,
        commitment: commitment_hash,
    })
}
