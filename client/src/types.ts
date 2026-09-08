import type { ActionHash, AgentPubKey, Record as HcRecord } from "@holochain/client";

/**
 * These mirror the Rust structs in `nft_integrity` / `nft`. Field names match the
 * on-wire (MessagePack) names, i.e. Rust snake_case.
 */

export interface NftAttribute {
  trait_type: string;
  value: string;
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

// --- coordinator I/O ------------------------------------------------------------

export interface MintNftOutput {
  /** The NFT's permanent identity (its mint Create action hash). */
  nft: ActionHash;
  record: HcRecord;
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
  | { type: "TransferAccepted"; commitment: ActionHash };

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
