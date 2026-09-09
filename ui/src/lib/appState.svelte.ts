import type {
  ActionHash,
  AgentPubKey,
  AppWebsocket,
  Record as HcRecord,
} from "@holochain/client";
import {
  NftClient,
  decodeEntry,
  decodeNftManifest,
  recordActionHash,
  type CollectionInfo,
  type Commitment,
  type DecodedNftManifest,
  type LocalSignal,
  type ProvenanceHop,
  type RemoteSignal,
  type TransferCommitment,
} from "@p2p-art-marketplace/client";
import { b64, eqHash, tsToMs } from "./format";
import {
  hasCommitment,
  loadCommitment,
  markRevealed,
  saveCommitment,
} from "./saltStore";

export interface NftView {
  /** Stable id: the mint action hash. */
  nft: ActionHash;
  latest: HcRecord;
  manifest: DecodedNftManifest;
  /** Is the reveal commitment stored on this device? */
  hasSalt: boolean;
}

export interface InboundOffer {
  commitment: ActionHash;
  nft: ActionHash;
  from: AgentPubKey;
  sequence: number;
  openedAtMs: number;
  deadlineMs: number;
}

export interface NftDetail {
  manifest: DecodedNftManifest;
  /** Author of the latest manifest record — always the minter (only the minter reveals). */
  minter: AgentPubKey;
  owner: AgentPubKey;
  provenance: ProvenanceHop[];
  hasSalt: boolean;
}

// --- module-level singleton --------------------------------------------------
// `setContext` can't be used here (the client is created after an await, outside
// component init), so the live AppState lives in module rune state instead.

let current = $state<AppState | null>(null);

export function setCurrentAppState(state: AppState | null): void {
  current = state;
}

/** The connected AppState. Throws if called before <App/> has connected. */
export function useAppState(): AppState {
  if (!current) {
    throw new Error("AppState not initialised — render inside <App/>");
  }
  return current;
}

/** Coalesce a burst of signals into one refresh. */
class Debounced {
  private timer: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private readonly fn: () => void,
    private readonly ms = 150,
  ) {}
  trigger(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.fn();
    }, this.ms);
  }
}

export class AppState {
  readonly nft: NftClient;
  readonly me: AgentPubKey;

  collection = $state<CollectionInfo | null>(null);
  isMinter = $state(false);

  myNfts = $state<NftView[]>([]);
  myNftsLoading = $state(false);

  inbox = $state<InboundOffer[]>([]);
  inboxLoading = $state(false);

  detailNft = $state<ActionHash | null>(null);
  detail = $state<NftDetail | null>(null);
  detailLoading = $state(false);

  error = $state<string | null>(null);

  private readonly unsub: () => void;
  private readonly refreshWallet = new Debounced(() => void this.loadMyNfts());
  private readonly refreshInbox = new Debounced(() => void this.loadInbox());
  private readonly refreshDetail = new Debounced(() => {
    if (this.detailNft) void this.loadDetail(this.detailNft);
  });

  constructor(client: AppWebsocket, me: AgentPubKey, roleName?: string) {
    this.nft = new NftClient({ client, roleName });
    this.me = me;
    this.unsub = this.nft.onSignal((s) => this.onSignal(s));
  }

  async init(): Promise<void> {
    try {
      const [info, minter] = await Promise.all([
        this.nft.collectionInfo(),
        this.nft.isMinter(),
      ]);
      this.collection = info;
      this.isMinter = minter;
    } catch (e) {
      this.error = String(e);
    }
    await Promise.all([this.loadMyNfts(), this.loadInbox()]);
  }

  dispose(): void {
    this.unsub();
  }

  isMe(who: AgentPubKey): boolean {
    return eqHash(who, this.me);
  }

  // --- signal wiring -----------------------------------------------------

  private onSignal(s: RemoteSignal | LocalSignal): void {
    switch (s.type) {
      case "TransferOffered":
        this.refreshInbox.trigger();
        break;
      case "TransferAccepted":
        this.refreshWallet.trigger();
        this.refreshDetail.trigger();
        break;
      default:
        // local post_commit Signals: EntryCreated / EntryUpdated / LinkCreated / …
        this.refreshWallet.trigger();
        this.refreshInbox.trigger();
        this.refreshDetail.trigger();
    }
  }

  // --- loads ----------------------------------------------------------

  async loadMyNfts(): Promise<void> {
    this.myNftsLoading = true;
    try {
      // The AgentToNft index can carry duplicate / stale links (one per past
      // ownership stint until dropped). Dedupe, then reconcile each against the
      // authoritative provenance chain: keep only what we actually own, and
      // quietly drop the leftover index links for the rest.
      const records = await this.nft.getNftsForAgent(this.me);
      const byId = new Map<string, HcRecord>();
      for (const r of records) byId.set(b64(recordActionHash(r)), r);

      const resolved = await Promise.all(
        [...byId.values()].map(async (r) => {
          const nft = recordActionHash(r);
          const [owner, hasSalt] = await Promise.all([
            this.nft.currentOwner(nft).catch(() => this.me), // tolerate a lagging DHT
            hasCommitment(nft),
          ]);
          const view: NftView = { nft, latest: r, manifest: decodeNftManifest(r), hasSalt };
          return { view, owned: eqHash(owner, this.me) };
        }),
      );

      this.myNfts = resolved.filter((x) => x.owned).map((x) => x.view);
      for (const { view, owned } of resolved) {
        if (!owned) void this.nft.dropStaleOwnershipLink(view.nft).catch(() => {});
      }
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.myNftsLoading = false;
    }
  }

  async loadInbox(): Promise<void> {
    this.inboxLoading = true;
    try {
      const windowMs = (await this.ensureCollection()).transfer_window_secs * 1000;
      const records = await this.nft.inboundCommitments();
      this.inbox = records.map((r) => {
        const c = decodeEntry<TransferCommitment>(r);
        const header = r.signed_action.hashed.content.header;
        const openedAtMs = tsToMs(header.timestamp);
        return {
          commitment: recordActionHash(r),
          nft: c.nft,
          from: header.author,
          sequence: c.sequence,
          openedAtMs,
          deadlineMs: openedAtMs + windowMs,
        };
      });
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.inboxLoading = false;
    }
  }

  private async ensureCollection(): Promise<CollectionInfo> {
    if (!this.collection) this.collection = await this.nft.collectionInfo();
    return this.collection;
  }

  async openDetail(nft: ActionHash): Promise<void> {
    this.detailNft = nft;
    await this.loadDetail(nft);
  }

  closeDetail(): void {
    this.detailNft = null;
    this.detail = null;
  }

  async loadDetail(nft: ActionHash): Promise<void> {
    this.detailLoading = true;
    try {
      const [rec, owner, provenance, salt] = await Promise.all([
        this.nft.getNft(nft),
        this.nft.currentOwner(nft),
        this.nft.getProvenance(nft),
        hasCommitment(nft),
      ]);
      this.detail = rec
        ? {
            manifest: decodeNftManifest(rec),
            minter: rec.signed_action.hashed.content.header.author,
            owner,
            provenance,
            hasSalt: salt,
          }
        : null;
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.detailLoading = false;
    }
  }

  // --- actions (persist salt / refresh handled here; views just call these) ---

  async mint(commitment: Commitment): Promise<ActionHash> {
    const out = await this.nft.mintFromCommitment(commitment);
    await saveCommitment(out.nft, commitment);
    await this.loadMyNfts();
    return out.nft;
  }

  async reveal(nft: ActionHash, previousActionHash: ActionHash): Promise<void> {
    const stored = await loadCommitment(nft);
    if (!stored) {
      throw new Error(
        "No reveal commitment for this NFT on this device — the salt is gone, it cannot be revealed here.",
      );
    }
    await this.nft.revealNft({
      nft,
      previousActionHash,
      salt: Uint8Array.from(stored.salt),
      attributes: stored.attributes,
      imageUri: stored.imageUri,
    });
    await markRevealed(nft);
    await Promise.all([this.loadMyNfts(), this.loadDetail(nft)]);
  }

  async initiateTransfer(
    nft: ActionHash,
    toAgent: AgentPubKey,
  ): Promise<ActionHash> {
    const commitment = await this.nft.initiateTransfer(nft, toAgent);
    await this.loadMyNfts();
    return commitment;
  }

  async acceptOffer(commitment: ActionHash): Promise<void> {
    await this.nft.acceptTransfer(commitment);
    await Promise.all([this.loadInbox(), this.loadMyNfts()]);
  }

  async cancelOffer(commitment: ActionHash): Promise<void> {
    await this.nft.cancelTransfer(commitment);
    await this.loadMyNfts();
  }

  async finalizeSale(nft: ActionHash): Promise<void> {
    await this.nft.dropStaleOwnershipLink(nft);
    await this.loadMyNfts();
  }
}
