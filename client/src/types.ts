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
  | { type: "DelegatedReveal"; nft: ActionHash; agreement: ActionHash };

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
