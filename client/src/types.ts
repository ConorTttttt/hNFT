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
  /** base64 AgentPubKey, or `null` for open (dev) minting. */
  minter_pubkey: string | null;
  royalty_bps: number;
  transfer_window_secs: number;
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

/** The `BlindPurchaseCommitment` entry as it decodes off the wire (`decodeEntry`). */
export interface BlindPurchaseCommitment {
  batch: ActionHash;
  /** 32 bytes, MessagePack array of ints on the wire. */
  nonce_hash: number[];
  /** Opaque to the client — never decrypt or interpret this here. */
  encrypted_nonce: number[];
}

export interface MintBlindBatchOutput {
  batch: ActionHash;
  /** Sealed mint action hashes, in slot order (`nfts[i]` is slot `i`). */
  nfts: ActionHash[];
}

export interface ReserveBlindPurchaseOutput {
  purchase: ActionHash;
  /**
   * Generated and returned by the zome call itself — NOT generated
   * client-side. The buyer's client must hold onto this (same as a reveal
   * salt) to be able to confirm the eventual `BlindAssignmentReveal`
   * matches what they actually committed to. There is no way to recover
   * it later — only its hash is ever committed on-chain.
   */
  buyerNonce: Uint8Array;
}

export interface ResolveBlindAssignmentInput {
  purchase: ActionHash;
  /** The minter's own secret for this batch — 32 bytes. */
  mintSeed: Uint8Array;
  /** Audit-trail hash, same role as in `initiate_transfer_from_rave`. */
  rave: ActionHash;
  fulfilled: boolean;
  nftId: string;
  newOwner: string;
  previousOwner: string;
}

export interface ResolveBlindAssignmentOutput {
  reveal: ActionHash;
  assignedNft: ActionHash;
  commitment: ActionHash;
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
  | { type: "ListingInterest"; listing: ActionHash; nft: ActionHash; buyer: AgentPubKey };

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
