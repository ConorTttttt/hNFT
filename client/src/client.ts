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
  CollectionInfo,
  LocalSignal,
  MintNftOutput,
  NftManifest,
  ProvenanceHop,
  RemoteSignal,
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

  /** Mint a sealed NFT for a commitment computed with `prepareCommitment`. */
  mintNft(metadataHash: Uint8Array): Promise<MintNftOutput> {
    return this.call("mint_nft", { metadata_hash: bytes32(metadataHash) });
  }

  mintFromCommitment(c: Commitment): Promise<MintNftOutput> {
    return this.mintNft(c.metadataHash);
  }

  // --- reveal -------------------------------------------------------------

  /**
   * Reveal the sealed metadata. `previousActionHash` is the head of this agent's
   * chain for the NFT — the mint action for a first reveal.
   */
  revealNft(args: {
    nft: ActionHash;
    previousActionHash: ActionHash;
    salt: Uint8Array;
    attributes: { trait_type: string; value: string }[];
    imageUri: string;
  }): Promise<HcRecord> {
    return this.call("reveal_nft", {
      nft: args.nft,
      previous_action_hash: args.previousActionHash,
      salt: bytes32(args.salt),
      attributes: args.attributes,
      image_uri: args.imageUri,
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

  /** Authoritative owner from the provenance chain. */
  currentOwner(nft: ActionHash): Promise<AgentPubKey> {
    return this.call("current_owner", nft);
  }

  /** Chain of custody, ordered by `sequence` (1-based, contiguous). */
  getProvenance(nft: ActionHash): Promise<ProvenanceHop[]> {
    return this.call("get_provenance", nft);
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
