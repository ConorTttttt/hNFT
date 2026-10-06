import type { ActionHash, AgentPubKey, Record as HcRecord } from "@holochain/client";

/**
 * These mirror the Rust structs in `nft_integrity` / `nft`. Field names match the
 * on-wire (MessagePack) names, i.e. Rust snake_case.
 */

export interface NftAttribute {
  trait_type: string;
  value: string;
}

/** DNA `modifiers.properties` for this collection cell (from `collection_info`). */
export interface CollectionInfo {
  name: string;
  symbol: string;
  max_supply: number;
  /** base64 AgentPubKey, or `null` for open (dev) minting. Ignored once
   * `whitelist` is non-empty — see `mintPolicyOf`. */
  minter_pubkey: string | null;
  royalty_bps: number;
  transfer_window_secs: number;
  /** Seconds before anyone may flag a blind purchase abandoned (UI-only signal). */
  blind_resolution_window_secs?: number;
  /** 64 lowercase hex chars (32 bytes); the stable collection namespace. Required for blind batches. */
  collection_id?: string | null;
  /** Randomness provider config. `dev-insecure` is the INSECURE dev verifier. */
  randomness?: RandomnessSpec | null;
  /** Generative trait configuration. `null`/absent = not a generative collection. */
  generation?: GenerationConfig | null;
  /** Per-agent mint whitelist (Phase J.1). `null`/absent/`[]` = whitelist mode
   * is off — `minter_pubkey` behaves as before. Non-empty = WHITELIST mode:
   * `minter_pubkey` is no longer consulted for mint authorization. Immutable
   * for the collection's lifetime, same as every other field here — see
   * `mintPolicyOf`. */
  whitelist?: WhitelistEntry[] | null;
}

/** One whitelist entry, as `CollectionProperties.whitelist` carries it. */
export interface WhitelistEntry {
  /** Base64 `AgentPubKey`. */
  agent: string;
  /** Lifetime mint cap for this agent. Must be `>= 1`. */
  allowance: number;
}

/** This collection's mint-authorization mode — derived, never sent as-is;
 * see `mintPolicyOf`. Mirrors the Rust `MintMode`/`MintPolicyView`. */
export type MintPolicy = "public" | "creator_only" | "whitelist";

/** From `mint_status()` — the connected agent's authoritative mint
 * eligibility/progress under this collection's active policy. A read-only
 * reflection: the integrity validator reaches `eligible`'s answer
 * independently at mint time. */
export interface MintStatus {
  policy: MintPolicy;
  eligible: boolean;
  /** This agent's own lifetime mint count in this collection. */
  minted_by_me: number;
  /** This agent's personal cap, when one applies to them — `null` under
   * PUBLIC, for a non-creator under CREATOR_ONLY, or for an agent absent
   * from the whitelist under WHITELIST. */
  my_allowance: number | null;
}

// --- G5a: generative NFTs -----------------------------------------------------
//
// See `dnas/nft/zomes/integrity/nft/src/generation.rs`,
// `generation_randomness.rs`, `generation_record.rs`, and
// `docs/generative-collections.md`. The seed is always DERIVED from verified
// randomness; there is no seed input anywhere in this API. The only randomness
// provider that exists is `dev-insecure`, which is forgeable/development-only.

export const INSECURE_DEV_PROVIDER_ID = "dev-insecure";

export interface RandomnessSpec {
  provider_id: string;
  verifier_params: string;
}

export interface TraitOption {
  id: string;
  display_name: string;
  weight: number;
}

export interface TraitCategory {
  id: string;
  display_name: string;
  traits: TraitOption[];
}

export interface GenerationConfig {
  generation_version: number;
  categories: TraitCategory[];
}

export interface GeneratedTrait {
  category_id: string;
  trait_id: string;
}

export interface GeneratedTraits {
  generation_version: number;
  traits: GeneratedTrait[];
}

/** The `GenerationRandomness` entry as it decodes off the wire. */
export interface GenerationRandomness {
  nft: ActionHash;
  provider_id: string;
  /** MessagePack array of ints on the wire. */
  event_id: number[];
  /** 32 bytes, MessagePack array of ints on the wire. */
  randomness: number[];
  /** MessagePack array of ints on the wire. */
  proof: number[];
}

/** The `GenerationRecord` entry as it decodes off the wire. */
export interface GenerationRecord {
  nft: ActionHash;
  generation_version: number;
  /** 32 bytes, MessagePack array of ints on the wire. */
  seed: number[];
  generated_traits: GeneratedTraits;
  randomness: { kind: "verified"; result: ActionHash };
}

/** Discriminated on `status` (Rust `#[serde(tag = "status", rename_all = "snake_case")]`). */
export type NftPayload =
  | { status: "sealed" }
  | {
      status: "revealed";
      /** 32 bytes, as a MessagePack array of ints when it comes off the wire. */
      salt: number[];
      attributes: NftAttribute[];
      image_uri: string;
      /** The `MintAgreement` this reveal was authorised under, if any. */
      agreement?: ActionHash | null;
    };

export interface NftManifest {
  /** 32-byte blake2b commitment, MessagePack array of ints on the wire. */
  metadata_hash: number[];
  payload: NftPayload;
  /** The blind batch this NFT was minted into (G3a), if any. */
  batch?: ActionHash | null;
  /** Its slot within that batch, if any. */
  batch_slot?: number | null;
}

export interface TransferCommitment {
  nft: ActionHash;
  to_agent: AgentPubKey;
  prev_state: ActionHash;
  sequence: number;
}

export interface TransferReceipt {
  commitment: ActionHash;
}

// --- Unyt settlement bridge (demo mode) --------------------------------------
//
// See `dnas/nft/zomes/coordinator/nft/src/payment.rs`'s module doc: this zome
// does NOT re-verify the claimed RAVE against the Unyt network — the caller
// (this client, ultimately whatever UI code read the RAVE) is trusted to have
// read `output.computed_values` correctly. `fulfilled`/`nft_id`/`new_owner`/
// `previous_owner` are exactly those four `computed_values` fields.

export interface SettleFromRaveInput {
  nft: ActionHash;
  to_agent: AgentPubKey;
  /** The Unyt RAVE's own action hash — carried through for the audit trail
   * in the inbound-commitment link's tag; not re-fetched or re-checked. */
  rave: ActionHash;
  fulfilled: boolean;
  nft_id: string;
  new_owner: string;
  previous_owner: string;
}

// --- reveal agreements -------------------------------------------------------

/** Flattened `MintAgreement` from `agreement_for`. Timestamps are epoch micros. */
export interface AgreementView {
  agreement: ActionHash;
  reveal_by_micros: number;
  default_after_micros: number;
  delegate: AgentPubKey | null;
  defaulted: boolean;
  /** Is the connected agent the delegate? */
  am_delegate: boolean;
}

/** From `my_delegated_reveals`. */
export interface DelegatedRevealView {
  nft: ActionHash;
  agreement: ActionHash;
  reveal_by_micros: number;
  default_after_micros: number;
  defaulted: boolean;
  sealed: boolean;
}

// --- listings -----------------------------------------------------------

/** The `Listing` entry as it decodes off the wire (`decodeEntry`). */
export interface Listing {
  nft: ActionHash;
  price: number;
  currency: string;
}

/** Flattened active listing from `listing_for_nft` / `all_listings` /
 * `my_listings` — already resolved to latest price and confirmed the lister
 * still owns the NFT. */
export interface ListingView {
  listing: ActionHash;
  nft: ActionHash;
  price: number;
  currency: string;
  seller: AgentPubKey;
}

// --- blind-box collections ---------------------------------------------------
//
// See `dnas/nft/zomes/coordinator/nft/src/blind.rs` and
// `docs/blind-box-collections.md`. Mirrors the actual Rust structs — field
// shapes below are confirmed against `blind.rs`, not inferred.

/** The `BlindBatchCommitment` entry as it decodes off the wire (`decodeEntry`). */
export interface BlindBatchCommitment {
  /** N — the committed inventory size (protocol max 1024; the UI offers at most 256). */
  batch_size: number;
  /** Advisory only. Never used by validation to close or release anything. */
  sale_window_secs: number;
  assignment_algorithm_version: number;
}

/** The `BlindPurchaseCommitment` entry as it decodes off the wire (`decodeEntry`). */
export interface BlindPurchaseCommitment {
  batch: ActionHash;
  /** The batch's `InventoryFinalized` action. */
  inventory: ActionHash;
}

/** Result of the composite `NftClient.mintBlindBatch` (create + chunked mint + finalize). */
export interface MintBlindBatchOutput {
  batch: ActionHash;
  /** Sealed mint action hashes, in slot order (`nfts[i]` is slot `i`). */
  nfts: ActionHash[];
  /** The `InventoryFinalized` action — the authoritative slot → NFT binding. */
  inventory: ActionHash;
}

export interface ReserveBlindPurchaseOutput {
  purchase: ActionHash;
}

/**
 * DRAFT → SALES_OPEN → CLOSED → RANDOMNESS_FIXED. ASSIGNED, TRANSFERRED and
 * REVEALED are derived per NFT, not per batch.
 */
export type BatchState = "DRAFT" | "SALES_OPEN" | "CLOSED" | "RANDOMNESS_FIXED";

export interface BatchStateView {
  state: BatchState;
  batch_size: number;
  inventory?: ActionHash | null;
  close?: ActionHash | null;
  result?: ActionHash | null;
}

/** One purchase's deterministic assignment (derived; there is no assignment entry). */
export interface AssignmentView {
  purchase: ActionHash;
  buyer: AgentPubKey;
  /** Position of the purchase in the close's sorted list. */
  position: number;
  slot: number;
  /** `InventoryFinalized.nfts[slot]`. */
  nft: ActionHash;
}

// --- coordinator I/O ------------------------------------------------------------

export interface MintNftOutput {
  /** The NFT's permanent identity (its mint Create action hash). */
  nft: ActionHash;
  record: HcRecord;
  /** The `MintAgreement` action, when minted with one. */
  agreement?: ActionHash | null;
}

export interface ProvenanceHop {
  sequence: number;
  from_agent: AgentPubKey;
  to_agent: AgentPubKey;
  receipt_action: ActionHash;
  commitment_action: ActionHash;
}

/** Best-effort handshake nudge delivered via `recv_remote_signal`. */
export type RemoteSignal =
  | { type: "TransferOffered"; commitment: ActionHash }
  | { type: "TransferAccepted"; commitment: ActionHash }
  | { type: "NftRevealed"; nft: ActionHash }
  | { type: "DelegatedReveal"; nft: ActionHash; agreement: ActionHash }
  | { type: "ListingInterest"; listing: ActionHash; nft: ActionHash; buyer: AgentPubKey }
  | { type: "BlindPurchaseReserved"; batch: ActionHash; purchase: ActionHash };

/** Local UI signal emitted from `post_commit` (loosely typed — shape follows the scaffolder's `Signal` enum). */
export interface LocalSignal {
  type:
    | "LinkCreated"
    | "LinkDeleted"
    | "EntryCreated"
    | "EntryUpdated"
    | "EntryDeleted";
  [k: string]: unknown;
}
