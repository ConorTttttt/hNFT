import type {
  ActionHash,
  AgentPubKey,
  AppWebsocket,
  Record as HcRecord,
} from "@holochain/client";
import { SignalType } from "@holochain/client";
import { decode } from "@msgpack/msgpack";
import type { Commitment } from "./commitment.js";
import type {
  AgreementView,
  CollectionInfo,
  DelegatedRevealView,
  ListingView,
  LocalSignal,
  AssignmentView,
  BatchStateView,
  GenerationRandomness,
  GenerationRecord,
  MintBlindBatchOutput,
  MintNftOutput,
  MintStatus,
  NftAttribute,
  NftManifest,
  ProvenanceHop,
  RemoteSignal,
  ReserveBlindPurchaseOutput,
} from "./types.js";

export interface NftClientOptions {
  client: AppWebsocket;
  /** Role name from `happ.yaml`. For a provisioned collection clone, pass its clone id, e.g. `"nft.0"`. Default `"nft"`. */
  roleName?: string;
  /** Coordinator zome name. Default `"nft"`. */
  zomeName?: string;
}

/** `[u8; 32]` fields (`metadata_hash`, `salt`) must go on the wire as a MessagePack
 * array of ints, not a `bin` — Rust deserializes them as a fixed-size sequence. */
const bytes32 = (b: Uint8Array): number[] => {
  if (b.length !== 32) throw new Error(`expected 32 bytes, got ${b.length}`);
  return Array.from(b);
};

export class NftClient {
  private readonly client: AppWebsocket;
  private readonly roleName: string;
  private readonly zomeName: string;

  constructor(opts: NftClientOptions) {
    this.client = opts.client;
    this.roleName = opts.roleName ?? "nft";
    this.zomeName = opts.zomeName ?? "nft";
  }

  private call<T>(fn_name: string, payload: unknown): Promise<T> {
    return this.client.callZome({
      role_name: this.roleName,
      zome_name: this.zomeName,
      fn_name,
      payload,
    }) as Promise<T>;
  }

  // --- collection ------------------------------------------------------------

  /** This collection's config (name, symbol, `max_supply`, `transfer_window_secs`, …). */
  collectionInfo(): Promise<CollectionInfo> {
    return this.call("collection_info", null);
  }

  // --- minting --------------------------------------------------------------

  /** May the connected agent mint in this collection? */
  isMinter(): Promise<boolean> {
    return this.call("is_minter", null);
  }

  /** The connected agent's authoritative mint eligibility/policy/allowance
   * progress (Phase J.2) — a superset of `isMinter()`. */
  mintStatus(): Promise<MintStatus> {
    return this.call("mint_status", null);
  }

  /** Mint a sealed NFT for a commitment computed with `prepareCommitment`. */
  mintNft(metadataHash: Uint8Array): Promise<MintNftOutput> {
    return this.call("mint_nft", { metadata_hash: bytes32(metadataHash) });
  }

  mintFromCommitment(c: Commitment): Promise<MintNftOutput> {
    return this.mintNft(c.metadataHash);
  }

  /**
   * Mint sealed under a reveal agreement: a deadline (`revealBy`), a
   * `defaultAfter` past which anyone may mark it defaulted, an optional
   * `delegate` who may reveal after `revealBy`, and — when `secret` is given —
   * the reveal preimage parked privately on-chain so the conductor's auto-reveal
   * sweep can reveal it without this browser.
   */
  mintWithAgreement(args: {
    metadataHash: Uint8Array;
    revealBy: Date;
    defaultAfter: Date;
    delegate?: AgentPubKey;
    secret?: { salt: Uint8Array; attributes: NftAttribute[]; imageUri: string };
  }): Promise<MintNftOutput> {
    return this.call("mint_with_agreement", {
      metadata_hash: bytes32(args.metadataHash),
      reveal_by_micros: args.revealBy.getTime() * 1000,
      default_after_micros: args.defaultAfter.getTime() * 1000,
      delegate: args.delegate ?? null,
      secret: args.secret
        ? {
            salt: bytes32(args.secret.salt),
            attributes: args.secret.attributes,
            image_uri: args.secret.imageUri,
          }
        : null,
    });
  }

  // --- reveal -------------------------------------------------------------

  /**
   * Reveal the sealed metadata. `previousActionHash` is the head of this agent's
   * chain for the NFT — the mint action for a first reveal. `force` reveals even
   * if a `RevealDefault` exists (a late reveal).
   */
  revealNft(args: {
    nft: ActionHash;
    previousActionHash: ActionHash;
    salt: Uint8Array;
    attributes: { trait_type: string; value: string }[];
    imageUri: string;
    force?: boolean;
  }): Promise<HcRecord> {
    return this.call("reveal_nft", {
      nft: args.nft,
      previous_action_hash: args.previousActionHash,
      salt: bytes32(args.salt),
      attributes: args.attributes,
      image_uri: args.imageUri,
      force: args.force ?? false,
    });
  }

  revealFromCommitment(
    nft: ActionHash,
    previousActionHash: ActionHash,
    c: Commitment,
  ): Promise<HcRecord> {
    return this.revealNft({
      nft,
      previousActionHash,
      salt: c.salt,
      attributes: c.attributes,
      imageUri: c.imageUri,
    });
  }

  // --- G5a: generative NFTs -------------------------------------------------
  //
  // The seed is always DERIVED from verified randomness (`generation_seed(...)`
  // in `randomness.rs`) — there is no seed input anywhere here. The only
  // provider is `dev-insecure`: forgeable, development/testing only, never
  // production-grade. See `docs/generative-collections.md`.

  /**
   * The dev-insecure proof for `(nft, eventId, randomness)` under this
   * collection's `RandomnessSpec`. Public and forgeable by design — exists only
   * to exercise the protocol.
   */
  devGenerationRandomnessProof(
    nft: ActionHash,
    eventId: Uint8Array,
    randomness: Uint8Array,
  ): Promise<number[]> {
    return this.call("dev_generation_randomness_proof", {
      nft,
      event_id: Array.from(eventId),
      randomness: bytes32(randomness),
    });
  }

  /**
   * Record verified randomness for `nft` (minter-only, once per NFT, before any
   * reveal — enforced by `validate()`). Returns the `GenerationRandomness`
   * action hash.
   */
  createGenerationRandomness(
    nft: ActionHash,
    eventId: Uint8Array,
    randomness: Uint8Array,
    proof: number[],
  ): Promise<ActionHash> {
    return this.call("create_generation_randomness", {
      nft,
      event_id: Array.from(eventId),
      randomness: bytes32(randomness),
      proof,
    });
  }

  /**
   * Read back the `GenerationRandomness` for `nft`, if any (via its discovery
   * link), along with its own action hash — needed as
   * `createGenerationRecord`'s `result` argument.
   */
  getGenerationRandomness(
    nft: ActionHash,
  ): Promise<[ActionHash, GenerationRandomness] | null> {
    return this.call("get_generation_randomness", nft);
  }

  /**
   * Derive the seed from `result` and record the generated traits. There is no
   * seed input: the coordinator computes `generation_seed(...)` and `generate(...)`.
   */
  createGenerationRecord(nft: ActionHash, result: ActionHash): Promise<HcRecord> {
    return this.call("create_generation_record", { nft, result });
  }

  /** Read back the `GenerationRecord` for `nft`, if any. */
  getGenerationRecord(nft: ActionHash): Promise<GenerationRecord | null> {
    return this.call("get_generation_record", nft);
  }

  /**
   * Reveal a generative NFT with the attributes derived from its
   * `GenerationRecord` — fails if no record exists. `imageUri` is minter-supplied
   * and NOT cryptographically bound to the generated traits.
   */
  revealGenerated(args: {
    nft: ActionHash;
    previousActionHash: ActionHash;
    salt: Uint8Array;
    imageUri: string;
  }): Promise<HcRecord> {
    return this.call("reveal_generated", {
      nft: args.nft,
      previous_action_hash: args.previousActionHash,
      salt: bytes32(args.salt),
      image_uri: args.imageUri,
    });
  }

  // --- transfer (two-phase handshake) -----------------------------------

  /** Offer the NFT to `toAgent`. Returns the `TransferCommitment` action hash. */
  async initiateTransfer(
    nft: ActionHash,
    toAgent: AgentPubKey,
  ): Promise<ActionHash> {
    const out = await this.call<{ commitment: ActionHash }>("initiate_transfer", {
      nft,
      to_agent: toAgent,
    });
    return out.commitment;
  }

  /**
   * Offer the NFT to `toAgent`, gated on a claimed Unyt `nft_sale_escrow`
   * settlement — DEMO MODE: this is NOT independently re-verified against
   * the Unyt network (see `payment.rs`'s module doc); the caller must have
   * already read the RAVE and be passing its real `computed_values`. Once
   * called, the resulting commitment goes through the exact same
   * `acceptTransfer` flow as a free `initiateTransfer` offer.
   *
   * `nftId` must be built as `nft.toString()`-equivalent (this NFT's mint
   * action hash, base64) to match the convention `payment.rs::nft_id_for`
   * expects — the same value must be what got written into the Unyt
   * listing's `nft_id` field when it was created.
   */
  async initiateTransferFromRave(args: {
    nft: ActionHash;
    toAgent: AgentPubKey;
    rave: ActionHash;
    fulfilled: boolean;
    nftId: string;
    newOwner: string;
    previousOwner: string;
  }): Promise<ActionHash> {
    const out = await this.call<{ commitment: ActionHash }>("initiate_transfer_from_rave", {
      nft: args.nft,
      to_agent: args.toAgent,
      rave: args.rave,
      fulfilled: args.fulfilled,
      nft_id: args.nftId,
      new_owner: args.newOwner,
      previous_owner: args.previousOwner,
    });
    return out.commitment;
  }

  /** Accept an offer (must be the named buyer, within the transfer window). */
  acceptTransfer(commitment: ActionHash): Promise<HcRecord> {
    return this.call("accept_transfer", { commitment });
  }

  /** Void an unaccepted offer (only the offerer, only after the window closes). */
  cancelTransfer(commitment: ActionHash): Promise<HcRecord> {
    return this.call("cancel_transfer", { commitment });
  }

  /** Housekeeping: drop this agent's now-stale ownership-index link for `nft`. */
  dropStaleOwnershipLink(nft: ActionHash): Promise<void> {
    return this.call("drop_stale_ownership_link", nft);
  }

  /** `TransferCommitment` records addressed to the connected agent. */
  inboundCommitments(): Promise<HcRecord[]> {
    return this.call("get_inbound_commitments", null);
  }

  // --- queries -----------------------------------------------------------

  /** Latest manifest version (follows the reveal chain). `null` if unknown. */
  getNft(nft: ActionHash): Promise<HcRecord | null> {
    return this.call("get_nft", nft);
  }

  /** Wallet view via the derived `AgentToNft` index (fast, not authoritative). */
  getNftsForAgent(agent: AgentPubKey): Promise<HcRecord[]> {
    return this.call("get_nfts_for_agent", { agent });
  }

  /**
   * Every NFT ever minted in this collection (mint records — the sealed
   * version, not necessarily the latest reveal state). Discovery only, not
   * authoritative for ownership; follow up with `getNft` per item for that.
   */
  getAllNfts(): Promise<HcRecord[]> {
    return this.call("get_all_nfts", null);
  }

  /** Authoritative owner from the provenance chain. */
  currentOwner(nft: ActionHash): Promise<AgentPubKey> {
    return this.call("current_owner", nft);
  }

  /** Chain of custody, ordered by `sequence` (1-based, contiguous). */
  getProvenance(nft: ActionHash): Promise<ProvenanceHop[]> {
    return this.call("get_provenance", nft);
  }

  // --- reveal agreements -----------------------------------------------

  /** The `MintAgreement` for an NFT (deadline, delegate, default state), or `null`. */
  agreementFor(nft: ActionHash): Promise<AgreementView | null> {
    return this.call("agreement_for", nft);
  }

  /** Mark an agreement's reveal defaulted (valid only after `defaultAfter`). */
  markRevealDefault(nft: ActionHash): Promise<HcRecord> {
    return this.call("mark_reveal_default", nft);
  }

  /** NFTs the connected agent is the reveal delegate for. */
  myDelegatedReveals(): Promise<DelegatedRevealView[]> {
    return this.call("my_delegated_reveals", null);
  }

  /** The reveal secret this device parked at mint for `nft`, if any (minter only). */
  async revealSecretFor(
    nft: ActionHash,
  ): Promise<{ salt: Uint8Array; attributes: NftAttribute[]; imageUri: string } | null> {
    const raw = await this.call<{
      salt: number[];
      attributes: NftAttribute[];
      image_uri: string;
    } | null>("reveal_secret_for", nft);
    return raw
      ? {
          salt: Uint8Array.from(raw.salt),
          attributes: raw.attributes,
          imageUri: raw.image_uri,
        }
      : null;
  }

  // --- blind fixed-inventory batches (G3a) ---------------------------------
  // See `blind.rs` and `docs/blind-box-collections.md`. N = committed inventory
  // size, K = purchases included in the close, 1 <= K <= N. The randomness
  // verifier in G3a is DEV-ONLY and INSECURE.

  /**
   * Create a batch, mint its N sealed NFTs (in chunks), and finalize the
   * inventory. `metadataHashes[i]` is slot `i`'s reveal commitment (build each
   * with `prepareCommitment`, exactly as for an ordinary mint). Chunking means a
   * failure part-way leaves a harmless DRAFT batch.
   */
  async mintBlindBatch(
    metadataHashes: Uint8Array[],
    opts: { saleWindowSecs?: number; chunkSize?: number } = {},
  ): Promise<MintBlindBatchOutput> {
    const chunk = Math.max(1, Math.min(opts.chunkSize ?? 64, 128));
    const batch = await this.call<ActionHash>("create_blind_batch", {
      batch_size: metadataHashes.length,
      sale_window_secs: opts.saleWindowSecs ?? 0,
    });
    const nfts: ActionHash[] = [];
    for (let start = 0; start < metadataHashes.length; start += chunk) {
      const part = await this.call<ActionHash[]>("mint_batch_nfts", {
        batch,
        start_slot: start,
        metadata_hashes: metadataHashes.slice(start, start + chunk).map(bytes32),
      });
      nfts.push(...part);
    }
    const inventory = await this.call<ActionHash>("finalize_inventory", batch);
    return { batch, nfts, inventory };
  }

  /** Buyer-side: reserve a purchase against a finalized batch. Payment is out of scope. */
  reserveBlindPurchase(batch: ActionHash): Promise<ReserveBlindPurchaseOutput> {
    return this.call("reserve_blind_purchase", { batch });
  }

  /** Every blind batch ever created in this collection, any minter.
   * Discovery only — `decodeEntry` per item for `batch_size`. */
  getAllBlindBatches(): Promise<HcRecord[]> {
    return this.call("get_all_blind_batches", null);
  }

  /** Purchases discoverable against `batch` — the minter's input to `closeBatch`. */
  purchasesForBatch(batch: ActionHash): Promise<HcRecord[]> {
    return this.call("purchases_for_batch", batch);
  }

  /** Read back a `BlindPurchaseCommitment`; the record's author is the buyer. */
  getBlindPurchase(purchase: ActionHash): Promise<HcRecord | null> {
    return this.call("get_blind_purchase", purchase);
  }

  /** DRAFT / SALES_OPEN / CLOSED / RANDOMNESS_FIXED, plus the linked entries. */
  getBatchState(batch: ActionHash): Promise<BatchStateView> {
    return this.call("get_batch_state", batch);
  }

  /**
   * Minter-side: commit the purchase set (1 <= K <= N, sorted) and close the
   * batch. With no `purchases`, every discoverable purchase is included (an
   * error if that exceeds N). The minter chooses the set — omission cannot be
   * prevented by validation.
   */
  closeBatch(batch: ActionHash, purchases?: ActionHash[]): Promise<ActionHash> {
    return this.call("close_batch", { batch, purchases: purchases ?? null });
  }

  /** The proof the INSECURE dev verifier accepts (public, forgeable by design). */
  async devRandomnessProof(
    close: ActionHash,
    eventId: Uint8Array,
    randomness: Uint8Array,
  ): Promise<number[]> {
    return this.call("dev_randomness_proof", {
      close,
      event_id: Array.from(eventId),
      randomness: bytes32(randomness),
    });
  }

  /** Record a randomness result for a close. Anyone may; validation checks the proof. */
  recordRandomness(
    close: ActionHash,
    eventId: Uint8Array,
    randomness: Uint8Array,
    proof: number[],
  ): Promise<ActionHash> {
    return this.call("record_randomness", {
      close,
      event_id: Array.from(eventId),
      randomness: bytes32(randomness),
      proof,
    });
  }

  /** DEV ONLY: record randomness using the dev proof. Not secure randomness. */
  async recordDevRandomness(
    close: ActionHash,
    eventId: Uint8Array,
    randomness: Uint8Array,
  ): Promise<ActionHash> {
    const proof = await this.devRandomnessProof(close, eventId, randomness);
    return this.recordRandomness(close, eventId, randomness, proof);
  }

  /** The full deterministic assignment for a closed batch with randomness. */
  getAssignments(close: ActionHash, result: ActionHash): Promise<AssignmentView[]> {
    return this.call("get_assignments", { close, result });
  }

  /**
   * Minter-side: first transfer of a batch NFT to its assigned buyer. The
   * recipient is the purchase's author; validation recomputes the assignment.
   */
  async releaseBatchTransfer(input: {
    nft: ActionHash;
    close: ActionHash;
    result: ActionHash;
    purchase: ActionHash;
  }): Promise<ActionHash> {
    const out = await this.call<{ commitment: ActionHash }>("release_batch_transfer", input);
    return out.commitment;
  }

  // --- listings ----------------------------------------------------------

  /** Post a public asking price for an NFT you currently own. */
  createListing(nft: ActionHash, price: number, currency: string): Promise<HcRecord> {
    return this.call("create_listing", { nft, price, currency });
  }

  /** Change the price on your own listing. `previousActionHash` is the head
   * of your update chain for it — the create action for a first edit, or the
   * previous update's action thereafter. */
  updateListingPrice(
    previousActionHash: ActionHash,
    price: number,
    currency: string,
  ): Promise<HcRecord> {
    return this.call("update_listing_price", {
      previous_action_hash: previousActionHash,
      price,
      currency,
    });
  }

  /** Cancel your own listing. */
  cancelListing(listing: ActionHash): Promise<void> {
    return this.call("cancel_listing", listing);
  }

  /** Latest version of a listing (follows price edits). `null` if unknown. */
  getListing(listing: ActionHash): Promise<HcRecord | null> {
    return this.call("get_listing", listing);
  }

  /** The active listing for an NFT, if any — already confirmed the lister
   * still owns it. */
  listingForNft(nft: ActionHash): Promise<ListingView | null> {
    return this.call("listing_for_nft", nft);
  }

  /** Every active listing in the collection, any lister. */
  allListings(): Promise<ListingView[]> {
    return this.call("all_listings", null);
  }

  /** Listings the connected agent currently has posted. */
  myListings(): Promise<ListingView[]> {
    return this.call("my_listings", null);
  }

  /** Signal interest in a listing. Doesn't move the NFT — the seller
   * completes a sale with `initiateTransfer` at whichever buyer they pick. */
  expressInterest(listing: ActionHash): Promise<void> {
    return this.call("express_interest", listing);
  }

  /** Seller-facing: who has expressed interest in a listing. */
  interestedBuyers(listing: ActionHash): Promise<AgentPubKey[]> {
    return this.call("interested_buyers", listing);
  }

  // --- signals ---------------------------------------------------------

  /**
   * Subscribe to this zome's signals. Yields both the best-effort transfer
   * nudges (`TransferOffered` / `TransferAccepted`) and the local post-commit
   * `Signal`s. Returns an unsubscribe function.
   */
  onSignal(cb: (signal: RemoteSignal | LocalSignal) => void): () => void {
    return this.client.on("signal", (signal) => {
      if (signal.type !== SignalType.App) return;
      if (signal.value.zome_name !== this.zomeName) return;
      cb(signal.value.payload as RemoteSignal | LocalSignal);
    });
  }
}

// --- record helpers ---------------------------------------------------------

export function decodeEntry<T>(record: HcRecord): T {
  const re = record.entry;
  if (!("Present" in re)) {
    throw new Error("record has no Present entry");
  }
  const entry = re.Present;
  if (entry.entry_type !== "App") {
    throw new Error(`expected an App entry, got ${entry.entry_type}`);
  }
  return decode(entry.entry) as T;
}

export function recordActionHash(record: HcRecord): ActionHash {
  return record.signed_action.hashed.hash;
}

export interface DecodedNftManifest extends NftManifest {
  /** `metadata_hash` as bytes, for hashing/comparison. */
  metadataHashBytes: Uint8Array;
}

export function decodeNftManifest(record: HcRecord): DecodedNftManifest {
  const m = decodeEntry<NftManifest>(record);
  return { ...m, metadataHashBytes: Uint8Array.from(m.metadata_hash) };
}
