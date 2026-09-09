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
import { b64, eqHash, nftTitle, shortHash, tsToMs, unb64 } from "./format";
import { toast } from "./toast.svelte";
import {
  hasCommitment,
  listCommitments,
  loadCommitment,
  markRevealed,
  saveCommitment,
} from "./saltStore";

export interface NftView {
  /** Stable id: the mint action hash. */
  nft: ActionHash;
  latest: HcRecord;
  manifest: DecodedNftManifest;
  minter: AgentPubKey;
  owner: AgentPubKey;
  /** Is the reveal commitment stored on this device? */
  hasSalt: boolean;
  /** Hidden from the collection view on this device (localStorage). */
  hidden: boolean;
}

export interface InboundOffer {
  commitment: ActionHash;
  nft: ActionHash;
  from: AgentPubKey;
  sequence: number;
  openedAtMs: number;
  deadlineMs: number;
  title: string;
  imageUri: string | null;
  /** `accepted` if a TransferReceipt for this commitment is in the provenance chain. */
  status: "pending" | "accepted";
  /** Dismissed on this device (localStorage). */
  hidden: boolean;
}

export interface ActivityItem {
  id: number;
  at: number;
  kind: "info" | "success";
  text: string;
}

/** An NFT this device minted whose metadata it can still reveal, whoever owns it now. */
export interface DraftView {
  nft: ActionHash;
  title: string;
  imageUri: string;
  ownedByOther: boolean;
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

const DISMISSED_KEY = "p2p-art:dismissed-offers";
const HIDDEN_NFTS_KEY = "p2p-art:hidden-nfts";

function loadStringSet(key: string): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(key) ?? "[]"));
  } catch {
    return new Set();
  }
}
function persistStringSet(key: string, set: Set<string>): void {
  try {
    localStorage.setItem(key, JSON.stringify([...set]));
  } catch {
    /* private mode — session-only */
  }
}

export class AppState {
  readonly nft: NftClient;
  readonly me: AgentPubKey;
  private readonly client: AppWebsocket;

  collection = $state<CollectionInfo | null>(null);
  isMinter = $state(false);

  /** Which top-level view is showing; owned here so signals can navigate. */
  view = $state<"collection" | "activity">("collection");
  /** Commitment to flash in the offers list (set from a "new offer" toast). */
  highlightOffer = $state<ActionHash | null>(null);

  myNfts = $state<NftView[]>([]);
  myNftsLoading = $state(false);
  /** NFTs this device minted that are still sealed — revealable regardless of ownership. */
  drafts = $state<DraftView[]>([]);

  inbox = $state<InboundOffer[]>([]);
  inboxLoading = $state(false);

  detailNft = $state<ActionHash | null>(null);
  detail = $state<NftDetail | null>(null);
  detailLoading = $state(false);

  /** DHT peer connection count; `null` until the first poll. */
  peers = $state<number | null>(null);
  /** Session-only activity log, newest first. */
  activity = $state<ActivityItem[]>([]);

  error = $state<string | null>(null);

  private dismissed = loadStringSet(DISMISSED_KEY);
  private hiddenNfts = loadStringSet(HIDDEN_NFTS_KEY);
  private activitySeq = 0;
  private highlightTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly unsub: () => void;
  private peerTimer: ReturnType<typeof setInterval> | null = null;
  private readonly refreshWallet = new Debounced(() => {
    void this.loadMyNfts();
    void this.loadDrafts();
  });
  private readonly refreshInbox = new Debounced(() => void this.loadInbox());
  private readonly refreshDetail = new Debounced(() => {
    if (this.detailNft) void this.loadDetail(this.detailNft);
  });

  constructor(client: AppWebsocket, me: AgentPubKey, roleName?: string) {
    this.client = client;
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
    void this.pollPeers();
    this.peerTimer = setInterval(() => void this.pollPeers(), 6000);
    await Promise.all([this.loadMyNfts(), this.loadInbox(), this.loadDrafts()]);
  }

  dispose(): void {
    this.unsub();
    if (this.peerTimer) clearInterval(this.peerTimer);
    if (this.highlightTimer) clearTimeout(this.highlightTimer);
  }

  /** Jump to the Activity view and flash the given offer for a few seconds. */
  goToOffer(commitment: ActionHash): void {
    this.view = "activity";
    this.highlightOffer = commitment;
    this.refreshInbox.trigger();
    if (this.highlightTimer) clearTimeout(this.highlightTimer);
    this.highlightTimer = setTimeout(() => (this.highlightOffer = null), 4000);
  }

  /** NFTs this device minted whose reveal salt we still hold and that are still sealed. */
  async loadDrafts(): Promise<void> {
    try {
      const stored = (await listCommitments()).filter((c) => c.revealedAt === null);
      const views = await Promise.all(
        stored.map(async (c) => {
          const nft = unb64(c.nftB64);
          const rec = await this.nft.getNft(nft).catch(() => null);
          if (!rec) return null;
          const m = decodeNftManifest(rec);
          if (m.payload.status !== "sealed") {
            await markRevealed(nft); // revealed elsewhere — drop it from the shelf
            return null;
          }
          const owner = await this.nft.currentOwner(nft).catch(() => this.me);
          const name = c.attributes.find(
            (a) => a.trait_type.trim().toLowerCase() === "name",
          )?.value;
          return {
            nft,
            title: name?.trim() || "Sealed NFT",
            imageUri: c.imageUri,
            ownedByOther: !eqHash(owner, this.me),
          } satisfies DraftView;
        }),
      );
      this.drafts = views.filter((v): v is DraftView => v !== null);
    } catch (e) {
      this.error = String(e);
    }
  }

  private async pollPeers(): Promise<void> {
    try {
      const stats = await this.client.dumpNetworkStats();
      this.peers = stats.transport_stats.connections.length;
    } catch {
      this.peers = null;
    }
  }

  private log(kind: ActivityItem["kind"], text: string): void {
    this.activity.unshift({ id: ++this.activitySeq, at: Date.now(), kind, text });
    if (this.activity.length > 60) this.activity.length = 60;
  }

  private applyOfferHidden(): void {
    this.inbox = this.inbox.map((o) => ({
      ...o,
      hidden: this.dismissed.has(b64(o.commitment)),
    }));
  }

  dismissOffer(commitment: ActionHash): void {
    this.dismissed.add(b64(commitment));
    persistStringSet(DISMISSED_KEY, this.dismissed);
    this.applyOfferHidden();
  }

  restoreOffer(commitment: ActionHash): void {
    this.dismissed.delete(b64(commitment));
    persistStringSet(DISMISSED_KEY, this.dismissed);
    this.applyOfferHidden();
  }

  // --- hiding NFTs from the collection view (this device only) ---------------

  isNftHidden(nft: ActionHash): boolean {
    return this.hiddenNfts.has(b64(nft));
  }

  private applyNftHidden(): void {
    this.myNfts = this.myNfts.map((v) => ({
      ...v,
      hidden: this.hiddenNfts.has(b64(v.nft)),
    }));
  }

  hideNft(nft: ActionHash): void {
    this.hiddenNfts.add(b64(nft));
    persistStringSet(HIDDEN_NFTS_KEY, this.hiddenNfts);
    this.applyNftHidden();
  }

  unhideNft(nft: ActionHash): void {
    this.hiddenNfts.delete(b64(nft));
    persistStringSet(HIDDEN_NFTS_KEY, this.hiddenNfts);
    this.applyNftHidden();
  }

  isMe(who: AgentPubKey): boolean {
    return eqHash(who, this.me);
  }

  /** Offers that still need a decision (pending, not hidden, not past the window). */
  get pendingOfferCount(): number {
    const now = Date.now();
    return this.inbox.filter(
      (o) => o.status === "pending" && !o.hidden && now < o.deadlineMs,
    ).length;
  }

  // --- signal wiring -----------------------------------------------------

  private onSignal(s: RemoteSignal | LocalSignal): void {
    switch (s.type) {
      case "TransferOffered": {
        const commitment = s.commitment;
        toast.info("New transfer offer — open it", () =>
          this.goToOffer(commitment),
        );
        this.log("info", "Received a transfer offer");
        this.refreshInbox.trigger();
        break;
      }
      case "TransferAccepted":
        toast.success("Your offer was accepted");
        this.log("success", "A transfer you offered was accepted");
        this.refreshWallet.trigger();
        this.refreshDetail.trigger();
        break;
      case "NftRevealed":
        toast.info("An NFT you own was revealed");
        this.log("info", "An NFT you own was revealed by its minter");
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
          const nft = recordActionHash(r); // identity = mint action hash
          const [owner, latest, hasSalt] = await Promise.all([
            this.nft.currentOwner(nft).catch(() => this.me), // tolerate a lagging DHT
            this.nft.getNft(nft).catch(() => null), // latest version (walks the reveal chain)
            hasCommitment(nft),
          ]);
          const record = latest ?? r;
          const view: NftView = {
            nft,
            latest: record,
            manifest: decodeNftManifest(record),
            minter: r.signed_action.hashed.content.header.author,
            owner,
            hasSalt,
            hidden: this.hiddenNfts.has(b64(nft)),
          };
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
      const offers = await Promise.all(
        records.map(async (r) => {
          const c = decodeEntry<TransferCommitment>(r);
          const header = r.signed_action.hashed.content.header;
          const openedAtMs = tsToMs(header.timestamp);
          const commitment = recordActionHash(r);
          const [nftRec, hops] = await Promise.all([
            this.nft.getNft(c.nft).catch(() => null),
            this.nft.getProvenance(c.nft).catch(() => []),
          ]);
          const m = nftRec ? decodeNftManifest(nftRec) : null;
          const accepted = hops.some((h) =>
            eqHash(h.commitment_action, commitment),
          );
          return {
            commitment,
            nft: c.nft,
            from: header.author,
            sequence: c.sequence,
            openedAtMs,
            deadlineMs: openedAtMs + windowMs,
            title: m ? nftTitle(m) : "NFT",
            imageUri:
              m?.payload.status === "revealed" ? m.payload.image_uri : null,
            status: accepted ? "accepted" : "pending",
            hidden: this.dismissed.has(b64(commitment)),
          } satisfies InboundOffer;
        }),
      );
      // Newest first.
      this.inbox = offers.sort((a, b) => b.openedAtMs - a.openedAtMs);
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

  // --- actions ---------------------------------------------------------------
  // Each toasts its own outcome and refreshes the affected stores, so views can
  // just `await` and not thread error/loading state through every button.

  async mint(commitment: Commitment): Promise<ActionHash> {
    try {
      const out = await this.nft.mintFromCommitment(commitment);
      await saveCommitment(out.nft, commitment);
      // The zome call has returned, so the entry + links are on our source chain;
      // DHT propagation to peers happens asynchronously from here.
      toast.success("Minted — publishing to peers…");
      this.log("success", `Minted ${shortHash(out.nft)} (sealed)`);
      await Promise.all([this.loadMyNfts(), this.loadDrafts()]);
      return out.nft;
    } catch (e) {
      toast.error(`Mint failed: ${errText(e)}`);
      throw e;
    }
  }

  async reveal(nft: ActionHash, previousActionHash: ActionHash): Promise<void> {
    const stored = await loadCommitment(nft);
    if (!stored) {
      toast.error(
        "The reveal salt for this NFT isn't on this device — it can't be revealed here.",
      );
      return;
    }
    try {
      await this.nft.revealNft({
        nft,
        previousActionHash,
        salt: Uint8Array.from(stored.salt),
        attributes: stored.attributes,
        imageUri: stored.imageUri,
      });
      await markRevealed(nft);
      await Promise.all([
        this.loadMyNfts(),
        this.loadDrafts(),
        this.loadDetail(nft),
      ]);
      toast.success("Metadata revealed");
      this.log("success", `Revealed ${shortHash(nft)}`);
    } catch (e) {
      toast.error(`Reveal failed: ${errText(e)}`);
    }
  }

  async initiateTransfer(
    nft: ActionHash,
    toAgent: AgentPubKey,
  ): Promise<ActionHash> {
    try {
      const commitment = await this.nft.initiateTransfer(nft, toAgent);
      await this.loadMyNfts();
      toast.success(`Offer sent to ${shortHash(toAgent)}`);
      this.log("info", `Offered ${shortHash(nft)} to ${shortHash(toAgent)}`);
      return commitment;
    } catch (e) {
      toast.error(`Transfer offer failed: ${errText(e)}`);
      throw e;
    }
  }

  async acceptOffer(commitment: ActionHash): Promise<void> {
    try {
      await this.nft.acceptTransfer(commitment);
      await Promise.all([this.loadInbox(), this.loadMyNfts()]);
      toast.success("Offer accepted — NFT is yours");
      this.log("success", "Accepted a transfer offer");
    } catch (e) {
      toast.error(`Accept failed: ${errText(e)}`);
    }
  }

  async cancelOffer(commitment: ActionHash): Promise<void> {
    try {
      await this.nft.cancelTransfer(commitment);
      await this.loadMyNfts();
      toast.info("Offer cancelled");
    } catch (e) {
      toast.error(`Cancel failed: ${errText(e)}`);
    }
  }

  async finalizeSale(nft: ActionHash): Promise<void> {
    try {
      await this.nft.dropStaleOwnershipLink(nft);
      await this.loadMyNfts();
    } catch (e) {
      toast.error(`Cleanup failed: ${errText(e)}`);
    }
  }
}

/** Holochain guest errors arrive as `... Guest("message") ...` — pull the message. */
function errText(e: unknown): string {
  const s = String(e);
  const m = s.match(/Guest\("(.+?)"\)/);
  return m?.[1] ?? s;
}
