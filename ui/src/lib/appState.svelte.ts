import { CellType } from "@holochain/client";
import type {
  ActionHash,
  AgentPubKey,
  AnyDhtHash,
  AppWebsocket,
  Record as HcRecord,
} from "@holochain/client";
import {
  NftClient,
  SocialClient,
  createNftCollection as createNftCollectionCell,
  decodeEntry,
  decodeNftManifest,
  discoverNetworkMarketplace,
  discoverNetworkNfts,
  mintPolicyOf,
  openDiscoveredCollection,
  parseDiscoveredCollections,
  publishNftCollectionForDiscovery,
  recordActionHash,
  type DiscoveredCollectionCard,
  type NetworkMarketplaceItem,
  type NetworkNftReference,
  type NftCollectionDiscoveryView,
  type AgreementView,
  type BlindBatchCommitment,
  type CollectionInfo,
  type CollectionItemView,
  type Commitment,
  type CreateNftCollectionInput,
  type CreatedNftCollection,
  type DecodedNftManifest,
  type DelegatedRevealView,
  type ListingView,
  type LocalSignal,
  type AssignmentView,
  type BatchState,
  type MintBlindBatchOutput,
  type MintStatus,
  type NftAttribute,
  type ProvenanceHop,
  type RemoteSignal,
  type SocialRemoteSignal,
  type TransferCommitment,
} from "@p2p-art-marketplace/client";
import { b64, eqHash, nftTitle, shortHash, tsToMs, unb64 } from "./format";
import { collectionKind } from "./collectionCards";
import { registerCollection } from "./collectionRegistry";
import { toast } from "./toast.svelte";
import {
  hasCommitment,
  listCommitments,
  loadCommitment,
  markRevealed,
  saveCommitment,
} from "./saltStore";

/**
 * The `nft` role name this device's `AppState` is wired to. The DNA supports
 * many clone cells (one per art collection — see `workdir/happ.yaml`), but
 * this UI doesn't yet expose switching between them, so every like/comment/
 * collection-item authored here names this role. It's the `target_role` a
 * social entry/link needs to resolve its target back to the right `nft` cell.
 */
export const NFT_ROLE = "nft";

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
  /** Its `MintAgreement`, if minted under one. */
  agreement: AgreementView | null;
  /** A private `RevealSecret` is parked on this chain — the sweep will auto-reveal. */
  autoReveal: boolean;
}

export interface CollectionSummary {
  collection: ActionHash;
  name: string;
  description: string;
}

/** One buyer's expressed interest in one of this agent's own listings. */
export interface InterestRow {
  listing: ActionHash;
  nft: ActionHash;
  price: number;
  currency: string;
  buyer: AgentPubKey;
}

/** A discoverable blind-box batch, any minter — see docs/blind-box-collections.md. */
export interface BlindBatchView {
  batch: ActionHash;
  batchSize: number;
  minter: AgentPubKey;
  mintedAtMs: number;
  /** DRAFT / SALES_OPEN / CLOSED / RANDOMNESS_FIXED (derived from the chain). */
  state: BatchState;
  inventory: ActionHash | null;
  close: ActionHash | null;
  result: ActionHash | null;
}

/** A purchase against one of this agent's own batches that is still in an open sale. */
export interface BlindPurchaseRow {
  purchase: ActionHash;
  batch: ActionHash;
  buyer: AgentPubKey;
  reservedAtMs: number;
}

export interface NftDetail {
  manifest: DecodedNftManifest;
  /** Author of the latest manifest record — always the minter (only the minter reveals). */
  minter: AgentPubKey;
  owner: AgentPubKey;
  provenance: ProvenanceHop[];
  hasSalt: boolean;
  /** The `MintAgreement` for this NFT, if minted under one. */
  agreement: AgreementView | null;
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
  readonly social: SocialClient;
  readonly me: AgentPubKey;
  private readonly client: AppWebsocket;
  /** This instance's own `nft` cell — the provisioned role, or a clone id
   * (e.g. `"nft.0"`) when bound to a runtime-created collection. Social
   * collection items authored here must name THIS role as their `target_role`,
   * not the hardcoded default, so cross-collection social links resolve. */
  readonly nftRoleName: string;

  collection = $state<CollectionInfo | null>(null);
  isMinter = $state(false);
  /** Full authoritative mint eligibility/policy/allowance picture (Phase
   * J.2) — `isMinter` above is exactly `mintStatus?.eligible`, kept as its
   * own field since most callers only need the boolean. */
  mintStatus = $state<MintStatus | null>(null);

  // --- Phase J.3: network-wide NFT-collection discovery -------------------
  // Cross-collection state (not per-active-collection), same category as
  // `createNftCollection` below — uses `this.social` (the one global cell)
  // and `this.client` (for local clone-cell/`appInfo` lookups), not `this.nft`.

  discoveredCollections = $state<NftCollectionDiscoveryView[]>([]);
  discoveredLoading = $state(false);
  discoveredLoaded = $state(false);
  /** `clone_id -> base64 DnaHash`, for every `nft` clone THIS device has
   * installed — resolved live from `appInfo()`, never remembered separately.
   * Used to tell "discovered AND locally installed" apart from "discovered
   * only" without needing a schema change to `collectionRegistry.ts`. */
  private localDnaHashByCloneId = $state<Map<string, string>>(new Map());

  /** `local dna hash lookup, keyed by clone id` — public read for card-building. */
  get localCloneDnaHashes(): ReadonlyMap<string, string> {
    return this.localDnaHashByCloneId;
  }

  /** Loads (or reloads) the network-wide discovery list plus this device's
   * own installed-clone DnaHashes, in one pass. Cheap: one `social` query +
   * one `appInfo()` call, neither of which scans any `nft` clone's own DHT. */
  async loadDiscoveredCollections(): Promise<void> {
    this.discoveredLoading = true;
    try {
      const [views, info] = await Promise.all([
        this.social.discoverNftCollections(),
        this.client.appInfo(),
      ]);
      this.discoveredCollections = views;
      const byClone = new Map<string, string>();
      for (const cellInfo of info?.cell_info[NFT_ROLE] ?? []) {
        if (cellInfo.type === CellType.Cloned) {
          byClone.set(cellInfo.value.clone_id, b64(cellInfo.value.cell_id[0]));
        }
      }
      this.localDnaHashByCloneId = byClone;
      this.discoveredLoaded = true;
    } catch (e) {
      toast.error(`Failed to load network collections: ${errText(e)}`);
    } finally {
      this.discoveredLoading = false;
    }
  }

  /** Publish the CURRENTLY ACTIVE collection for network-wide discovery.
   * Only meaningful for a runtime-created clone (`this.nftRoleName` is a
   * clone id) — the provisioned default role is identical on every install
   * by construction, so there is nothing to "discover" about it. */
  async publishActiveCollection(): Promise<ActionHash | null> {
    if (this.nftRoleName === NFT_ROLE) {
      toast.error("The default collection doesn't need publishing — every install already has it.");
      return null;
    }
    try {
      const hash = await publishNftCollectionForDiscovery(this.client, this.social, this.nftRoleName);
      toast.success("Published to the network — other peers can now discover this collection");
      await this.loadDiscoveredCollections();
      return hash;
    } catch (e) {
      toast.error(`Failed to publish collection: ${errText(e)}`);
      return null;
    }
  }

  /** Reconstruct the local clone a discovered collection points to (see
   * `openDiscoveredCollection`'s doc for the DnaHash verification it does),
   * then cache it in the local registry — the LOCAL registry is a cache of
   * "collections this device can open", never the authority on which
   * collections exist (that's `discoverNftCollections`). */
  async openDiscovered(card: DiscoveredCollectionCard): Promise<{ cloneId: string } | null> {
    try {
      const opened = await openDiscoveredCollection(this.client, card);
      registerCollection({
        cloneId: opened.cloneId,
        name: card.properties.name,
        collectionId: card.properties.collection_id ?? null,
        createdAt: Date.now(),
        symbol: card.properties.symbol,
        kind: collectionKind(card.properties),
        maxSupply: card.properties.max_supply,
        restrictedMinting: card.properties.minter_pubkey !== null,
        mintPolicy: mintPolicyOf(card.properties),
      });
      toast.success(`Opened "${card.properties.name}"`);
      return { cloneId: opened.cloneId };
    } catch (e) {
      toast.error(`Failed to open collection: ${errText(e)}`);
      return null;
    }
  }

  /** Parsed, display-ready view of `discoveredCollections` — malformed
   * entries are dropped, never thrown (see `parseDiscoveredCollections`). */
  get discoveredCards(): DiscoveredCollectionCard[] {
    return parseDiscoveredCollections(this.discoveredCollections);
  }

  // --- Phase J.4: network-wide NFT discovery -------------------------------
  // A bounded, on-demand PREVIEW across discovered collections — not a
  // finished cross-collection Items feed (see `discoverNetworkNfts`'s doc).
  // Composed entirely from `discoveredCards` (J.3) + each collection's own,
  // unmodified `get_all_nfts` — no new protocol underneath this.

  networkNftReferences = $state<NetworkNftReference[]>([]);
  networkNftsLoading = $state(false);
  networkNftsLoaded = $state(false);
  /** Discovered collections this pass could not resolve (clone failed to
   * open, or its `get_all_nfts` call failed) — surfaced so the UI can show
   * "unable to resolve" rather than silently omitting them. */
  networkNftsUnresolved = $state<DiscoveredCollectionCard[]>([]);

  /** Bounded preview: resolves NFTs from up to a handful of discovered
   * collections not already local. Opens any needed clones and folds them
   * into the local registry cache (never a hidden, untracked install). */
  async loadNetworkNfts(): Promise<void> {
    this.networkNftsLoading = true;
    try {
      const already = new Map<string, string>();
      for (const [cloneId, dnaHashB64] of this.localDnaHashByCloneId) {
        already.set(dnaHashB64, cloneId);
      }
      const result = await discoverNetworkNfts(this.client, this.discoveredCards, already);
      this.networkNftReferences = result.references;
      this.networkNftsUnresolved = result.unresolvedCollections;
      for (const { cloneId, card } of result.openedCollections) {
        registerCollection({
          cloneId,
          name: card.properties.name,
          collectionId: card.properties.collection_id ?? null,
          createdAt: Date.now(),
          symbol: card.properties.symbol,
          kind: collectionKind(card.properties),
          maxSupply: card.properties.max_supply,
          restrictedMinting: card.properties.minter_pubkey !== null,
          mintPolicy: mintPolicyOf(card.properties),
        });
      }
      if (result.openedCollections.length) await this.loadDiscoveredCollections();
      this.networkNftsLoaded = true;
    } catch (e) {
      toast.error(`Failed to load network items: ${errText(e)}`);
    } finally {
      this.networkNftsLoading = false;
    }
  }

  // --- Phase J.5: Network Marketplace ---------------------------------------
  // Built strictly on top of J.4's `discoverNetworkNfts` (via
  // `discoverNetworkMarketplace`, which adds only one more authoritative call
  // per resolved collection: its own `all_listings()`). Still no new DHT
  // entry type, no global index, no durable network cache — this state is
  // rebuilt from scratch on every "Refresh", and "Load more" only ever
  // queries collections not already folded in this session.

  static readonly DEFAULT_MARKETPLACE_MAX_COLLECTIONS = 6;
  static readonly DEFAULT_MARKETPLACE_MAX_NFTS_PER_COLLECTION = 24;

  networkMarketplaceItems = $state<NetworkMarketplaceItem[]>([]);
  networkMarketplaceLoading = $state(false);
  networkMarketplaceLoaded = $state(false);
  /** Discovered collections this session could not resolve into the
   * marketplace (clone failed to open, or its queries failed) — surfaced,
   * never silently dropped. */
  networkMarketplaceUnresolved = $state<DiscoveredCollectionCard[]>([]);
  /** Base64 dna_hash of every collection already folded into
   * `networkMarketplaceItems`/`networkMarketplaceUnresolved` this session —
   * what "Load more collections" uses to avoid re-querying the same ones. */
  private networkMarketplaceSeenDnaHashes = $state<Set<string>>(new Set());

  /** `true` once more discovered collections exist than this session has
   * attempted to resolve — i.e. "Load more collections" would do something. */
  get networkMarketplaceMoreAvailable(): boolean {
    return this.discoveredCards.some(
      (c) => !this.networkMarketplaceSeenDnaHashes.has(b64(c.dnaHash)),
    );
  }

  get networkMarketplaceLoadedCollectionCount(): number {
    return new Set(this.networkMarketplaceItems.map((i) => i.collectionDnaHashB64)).size;
  }

  /**
   * Loads (or extends) the Network Marketplace.
   * `more: true` only queries collections not yet seen this session and
   * APPENDS to the existing items — "Load more collections" (§10). Anything
   * else is a full "Refresh": the item/unresolved/seen state is reset first,
   * so no stale listing/ownership state can survive a refresh (§20/§23).
   */
  async loadNetworkMarketplace(
    opts: { more?: boolean; maxCollections?: number; maxNftsPerCollection?: number } = {},
  ): Promise<void> {
    this.networkMarketplaceLoading = true;
    try {
      const already = new Map<string, string>();
      for (const [cloneId, dnaHashB64] of this.localDnaHashByCloneId) {
        already.set(dnaHashB64, cloneId);
      }

      if (!opts.more) {
        this.networkMarketplaceItems = [];
        this.networkMarketplaceUnresolved = [];
        this.networkMarketplaceSeenDnaHashes = new Set();
      }

      const pending = this.discoveredCards.filter(
        (c) => !this.networkMarketplaceSeenDnaHashes.has(b64(c.dnaHash)),
      );

      const result = await discoverNetworkMarketplace(this.client, pending, already, {
        maxCollections: opts.maxCollections ?? AppState.DEFAULT_MARKETPLACE_MAX_COLLECTIONS,
        maxNftsPerCollection:
          opts.maxNftsPerCollection ?? AppState.DEFAULT_MARKETPLACE_MAX_NFTS_PER_COLLECTION,
      });

      this.networkMarketplaceItems = [...this.networkMarketplaceItems, ...result.items];
      this.networkMarketplaceUnresolved = [
        ...this.networkMarketplaceUnresolved,
        ...result.unresolvedCollections,
      ];
      const seen = new Set(this.networkMarketplaceSeenDnaHashes);
      for (const item of result.items) seen.add(item.collectionDnaHashB64);
      for (const c of result.unresolvedCollections) seen.add(b64(c.dnaHash));
      this.networkMarketplaceSeenDnaHashes = seen;

      for (const { cloneId, card } of result.openedCollections) {
        registerCollection({
          cloneId,
          name: card.properties.name,
          collectionId: card.properties.collection_id ?? null,
          createdAt: Date.now(),
          symbol: card.properties.symbol,
          kind: collectionKind(card.properties),
          maxSupply: card.properties.max_supply,
          restrictedMinting: card.properties.minter_pubkey !== null,
          mintPolicy: mintPolicyOf(card.properties),
        });
      }
      if (result.openedCollections.length) await this.loadDiscoveredCollections();
      this.networkMarketplaceLoaded = true;
    } catch (e) {
      toast.error(`Failed to load network marketplace: ${errText(e)}`);
    } finally {
      this.networkMarketplaceLoading = false;
    }
  }

  /** Which top-level view is showing; owned here so signals can navigate. */
  view = $state<"overview" | "collection" | "discover" | "collections" | "activity" | "blind">(
    "overview",
  );
  /** Commitment to flash in the offers list (set from a "new offer" toast). */
  highlightOffer = $state<ActionHash | null>(null);

  myNfts = $state<NftView[]>([]);
  myNftsLoading = $state(false);
  /** Every NFT in the collection, for the Discover view. Loaded lazily. */
  discover = $state<NftView[]>([]);
  discoverLoading = $state(false);
  /** Has `loadDiscover()` completed at least once for this collection? Lets
   * the UI tell "not loaded yet" apart from "loaded, zero items". */
  discoverLoaded = $state(false);
  /** Every active listing in this collection (`allListings()`), fetched once
   * and shared by the Overview, Items grid and NFT cards. */
  listings = $state<ListingView[]>([]);
  listingsLoading = $state(false);
  listingsLoaded = $state(false);
  /** `listings` keyed by b64 NFT hash, for per-card lookups without a query per card. */
  listingByNft = $derived(new Map(this.listings.map((l) => [b64(l.nft), l] as const)));
  private blindBatchesLoaded = false;
  /** NFTs this device minted that are still sealed — revealable regardless of ownership. */
  drafts = $state<DraftView[]>([]);
  /** NFTs this agent is the reveal delegate for. */
  delegatedReveals = $state<DelegatedRevealView[]>([]);

  /** b64 agent keys this agent follows. */
  myFollowing = $state<Set<string>>(new Set());
  myCollections = $state<CollectionSummary[]>([]);
  myListings = $state<ListingView[]>([]);
  /** Interest expressed in any of `myListings` — the seller's queue. */
  interestInMyListings = $state<InterestRow[]>([]);

  /** Every blind-box batch in the collection, any minter. */
  blindBatches = $state<BlindBatchView[]>([]);
  blindBatchesLoading = $state(false);
  /** Purchases awaiting resolution against this agent's OWN batches. */
  myBatchPurchases = $state<BlindPurchaseRow[]>([]);

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
  private readonly unsubSocial: () => void;
  private peerTimer: ReturnType<typeof setInterval> | null = null;
  private readonly refreshWallet = new Debounced(() => {
    void this.loadMyNfts();
    void this.loadDrafts();
    void this.loadDelegatedReveals();
    void this.loadMyListings();
  });
  private readonly refreshInbox = new Debounced(() => void this.loadInbox());
  private readonly refreshDiscover = new Debounced(() => {
    if (this.discoverLoaded) void this.loadDiscover();
    if (this.listingsLoaded) void this.loadListings();
  });
  private readonly refreshDetail = new Debounced(() => {
    if (this.detailNft) void this.loadDetail(this.detailNft);
  });
  private readonly refreshBlindBatches = new Debounced(() => {
    if (this.blindBatchesLoaded) void this.loadBlindBatches();
  });

  constructor(client: AppWebsocket, me: AgentPubKey, roleName?: string) {
    this.client = client;
    this.nftRoleName = roleName ?? NFT_ROLE;
    this.nft = new NftClient({ client, roleName });
    this.social = new SocialClient({ client }); // global cell — no roleName override
    this.me = me;
    this.unsub = this.nft.onSignal((s) => this.onSignal(s));
    this.unsubSocial = this.social.onSignal((s) => this.onSocialSignal(s));
  }

  async init(): Promise<void> {
    try {
      const [info, status] = await Promise.all([
        this.nft.collectionInfo(),
        this.nft.mintStatus(),
      ]);
      this.collection = info;
      this.mintStatus = status;
      this.isMinter = status.eligible;
    } catch (e) {
      this.error = String(e);
    }
    void this.pollPeers();
    this.peerTimer = setInterval(() => void this.pollPeers(), 6000);
    await Promise.all([
      this.loadMyNfts(),
      this.loadInbox(),
      this.loadDrafts(),
      this.loadDelegatedReveals(),
      this.loadMyFollowing(),
      this.loadMyCollections(),
      this.loadMyListings(),
    ]);
  }

  dispose(): void {
    this.unsub();
    this.unsubSocial();
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
          const [owner, agreement, secret] = await Promise.all([
            this.nft.currentOwner(nft).catch(() => this.me),
            this.nft.agreementFor(nft).catch(() => null),
            this.nft.revealSecretFor(nft).catch(() => null),
          ]);
          const name = c.attributes.find(
            (a) => a.trait_type.trim().toLowerCase() === "name",
          )?.value;
          return {
            nft,
            title: name?.trim() || "Sealed NFT",
            imageUri: c.imageUri,
            ownedByOther: !eqHash(owner, this.me),
            agreement,
            autoReveal: secret !== null,
          } satisfies DraftView;
        }),
      );
      this.drafts = views.filter((v): v is DraftView => v !== null);
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadDelegatedReveals(): Promise<void> {
    try {
      this.delegatedReveals = (await this.nft.myDelegatedReveals()).filter(
        (d) => d.sealed,
      );
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadMyFollowing(): Promise<void> {
    try {
      const following = await this.social.following(this.me);
      this.myFollowing = new Set(following.map((a) => b64(a)));
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadMyCollections(): Promise<void> {
    try {
      const records = await this.social.myCollections();
      this.myCollections = records.map((r) => ({
        collection: recordActionHash(r),
        ...decodeEntry<{ name: string; description: string }>(r),
      }));
    } catch (e) {
      this.error = String(e);
    }
  }

  async loadMyListings(): Promise<void> {
    try {
      this.myListings = await this.nft.myListings();
      await this.loadInterestQueue();
    } catch (e) {
      this.error = String(e);
    }
  }

  private async loadInterestQueue(): Promise<void> {
    try {
      const rows = await Promise.all(
        this.myListings.map(async (l) => {
          const buyers = await this.nft.interestedBuyers(l.listing).catch(() => []);
          return buyers.map(
            (buyer) =>
              ({
                listing: l.listing,
                nft: l.nft,
                price: l.price,
                currency: l.currency,
                buyer,
              }) satisfies InterestRow,
          );
        }),
      );
      this.interestInMyListings = rows.flat();
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
    this.discover = this.discover.map((v) => ({
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

  isFollowing(agent: AgentPubKey): boolean {
    return this.myFollowing.has(b64(agent));
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
        this.refreshDiscover.trigger();
        break;
      case "NftRevealed":
        toast.info("An NFT you own was revealed");
        this.log("info", "An NFT you own was revealed by its minter");
        this.refreshWallet.trigger();
        this.refreshDetail.trigger();
        this.refreshDiscover.trigger();
        break;
      case "DelegatedReveal":
        toast.info("You're the reveal delegate for an NFT", () => {
          this.view = "activity";
        });
        this.log("info", "Named as a reveal delegate");
        void this.loadDelegatedReveals();
        break;
      case "ListingInterest":
        toast.info("Someone's interested in one of your listings", () => {
          this.view = "activity";
        });
        this.log("info", "New interest in a listing");
        void this.loadInterestQueue();
        break;
      case "BlindPurchaseReserved":
        toast.info("Someone reserved a blind-box purchase against your batch", () => {
          this.view = "blind";
        });
        this.log("info", "New blind-box purchase reserved against your batch");
        if (this.blindBatchesLoaded) void this.loadMyBatchPurchases();
        break;
      default:
        // local post_commit Signals: EntryCreated / EntryUpdated / LinkCreated / …
        this.refreshWallet.trigger();
        this.refreshInbox.trigger();
        this.refreshDetail.trigger();
        this.refreshDiscover.trigger();
        this.refreshBlindBatches.trigger();
    }
  }

  private onSocialSignal(s: SocialRemoteSignal): void {
    switch (s.type) {
      case "Followed":
        toast.info("You have a new follower");
        this.log("info", "Someone followed you");
        break;
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

  /** Switch to the Items view, loading its contents the first time. */
  async openDiscover(): Promise<void> {
    this.view = "discover";
    await this.ensureMarketData();
  }

  /** Switch to the collection Overview, loading its statistics the first time. */
  async openOverview(): Promise<void> {
    this.view = "overview";
    await this.ensureMarketData();
  }

  /**
   * Load the collection-wide NFT set (`getAllNfts`) and listing set
   * (`allListings`) once, without changing the current view. Both are full
   * scans of this one collection — there are no dedicated count externs and
   * no pagination — so they're loaded once per `AppState` and then refreshed
   * only by signals.
   */
  async ensureMarketData(): Promise<void> {
    await Promise.all([
      this.discoverLoaded || this.discoverLoading ? Promise.resolve() : this.loadDiscover(),
      this.listingsLoaded || this.listingsLoading ? Promise.resolve() : this.loadListings(),
    ]);
  }

  /** Every active listing in the collection — the authoritative for-sale set. */
  async loadListings(): Promise<void> {
    this.listingsLoading = true;
    try {
      this.listings = await this.nft.allListings();
      this.listingsLoaded = true;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.listingsLoading = false;
    }
  }

  /** Every NFT ever minted in the collection — not just this agent's own. */
  async loadDiscover(): Promise<void> {
    this.discoverLoading = true;
    try {
      const records = await this.nft.getAllNfts();
      const byId = new Map<string, HcRecord>();
      for (const r of records) byId.set(b64(recordActionHash(r)), r);

      this.discover = await Promise.all(
        [...byId.values()].map(async (r) => {
          const nft = recordActionHash(r);
          const [owner, latest, hasSalt] = await Promise.all([
            this.nft.currentOwner(nft).catch(() => r.signed_action.hashed.content.header.author),
            this.nft.getNft(nft).catch(() => null),
            hasCommitment(nft),
          ]);
          const record = latest ?? r;
          return {
            nft,
            latest: record,
            manifest: decodeNftManifest(record),
            minter: r.signed_action.hashed.content.header.author,
            owner,
            hasSalt,
            hidden: this.hiddenNfts.has(b64(nft)),
          } satisfies NftView;
        }),
      );
      this.discoverLoaded = true;
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.discoverLoading = false;
    }
  }

  /** Switch to the blind-box view, loading its contents the first time. */
  async openBlindBoxes(): Promise<void> {
    this.view = "blind";
    if (!this.blindBatchesLoaded) await this.loadBlindBatches();
  }

  /** Every blind-box batch in the collection, any minter. */
  async loadBlindBatches(): Promise<void> {
    this.blindBatchesLoading = true;
    try {
      const records = await this.nft.getAllBlindBatches();
      this.blindBatches = await Promise.all(
        records.map(async (r) => {
          const entry = decodeEntry<BlindBatchCommitment>(r);
          const header = r.signed_action.hashed.content.header;
          const batch = recordActionHash(r);
          const st = await this.nft.getBatchState(batch).catch(() => null);
          return {
            batch,
            batchSize: entry.batch_size,
            minter: header.author,
            mintedAtMs: tsToMs(header.timestamp),
            state: st?.state ?? "DRAFT",
            inventory: st?.inventory ?? null,
            close: st?.close ?? null,
            result: st?.result ?? null,
          } satisfies BlindBatchView;
        }),
      );
      this.blindBatchesLoaded = true;
      await this.loadMyBatchPurchases();
      this.error = null;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.blindBatchesLoading = false;
    }
  }

  /** Purchases against batches THIS agent minted that are still in an open sale
   * (the minter's input to closing). Needs `blindBatches` loaded first. */
  async loadMyBatchPurchases(): Promise<void> {
    try {
      const myOpen = this.blindBatches.filter(
        (b) => eqHash(b.minter, this.me) && b.state === "SALES_OPEN",
      );
      const rows = await Promise.all(
        myOpen.map(async (b) => {
          const purchaseRecords = await this.nft.purchasesForBatch(b.batch).catch(() => []);
          return purchaseRecords.map((r) => {
            const header = r.signed_action.hashed.content.header;
            return {
              purchase: recordActionHash(r),
              batch: b.batch,
              buyer: header.author,
              reservedAtMs: tsToMs(header.timestamp),
            } satisfies BlindPurchaseRow;
          });
        }),
      );
      this.myBatchPurchases = rows.flat();
    } catch (e) {
      this.error = String(e);
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
      const [rec, owner, provenance, salt, agreement] = await Promise.all([
        this.nft.getNft(nft),
        this.nft.currentOwner(nft),
        this.nft.getProvenance(nft),
        hasCommitment(nft),
        this.nft.agreementFor(nft).catch(() => null),
      ]);
      this.detail = rec
        ? {
            manifest: decodeNftManifest(rec),
            minter: rec.signed_action.hashed.content.header.author,
            owner,
            provenance,
            hasSalt: salt,
            agreement,
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

  async mint(
    commitment: Commitment,
    agreement?: {
      revealBy: Date;
      defaultAfter: Date;
      delegate?: AgentPubKey;
      /** Park the reveal secret on-chain so the conductor sweep can auto-reveal. */
      autoReveal: boolean;
    },
  ): Promise<ActionHash> {
    try {
      const secret = {
        salt: commitment.salt,
        attributes: commitment.attributes as NftAttribute[],
        imageUri: commitment.imageUri,
      };
      const out = agreement
        ? await this.nft.mintWithAgreement({
            metadataHash: commitment.metadataHash,
            revealBy: agreement.revealBy,
            defaultAfter: agreement.defaultAfter,
            delegate: agreement.delegate,
            secret: agreement.autoReveal ? secret : undefined,
          })
        : await this.nft.mintFromCommitment(commitment);
      await saveCommitment(out.nft, commitment);
      // The zome call has returned, so the entry + links are on our source chain;
      // DHT propagation to peers happens asynchronously from here.
      toast.success("Minted — publishing to peers…");
      this.log(
        "success",
        `Minted ${shortHash(out.nft)}${agreement ? " (with reveal agreement)" : " (sealed)"}`,
      );
      await Promise.all([this.loadMyNfts(), this.loadDrafts(), this.refreshMintStatus()]);
      return out.nft;
    } catch (e) {
      toast.error(`Mint failed: ${errText(e)}`);
      throw e;
    }
  }

  /** Re-read authoritative mint eligibility/allowance after a mint attempt —
   * never decremented client-side. Best-effort: a transient failure here
   * just leaves the previous snapshot displayed a moment longer. */
  private async refreshMintStatus(): Promise<void> {
    try {
      const status = await this.nft.mintStatus();
      this.mintStatus = status;
      this.isMinter = status.eligible;
    } catch {
      // Leave the previous snapshot in place.
    }
  }

  // --- G5b: runtime NFT-collection creation -------------------------------
  // Via `createCloneCell` — never a generated/exported `dna.yaml`. See
  // `client/src/collections.ts`. `network_seed` is generated and hidden there;
  // nothing here ever sees or surfaces it.

  /**
   * Create a new NFT-collection clone cell and remember it in this device's
   * local collection registry (NOT a global/on-chain registry — see
   * `collectionRegistry.ts`). Does not switch to it; the caller decides that
   * (see `App.svelte`'s collection switcher).
   */
  async createNftCollection(input: CreateNftCollectionInput): Promise<CreatedNftCollection | null> {
    try {
      const created = await createNftCollectionCell(this.client, input);
      registerCollection({
        cloneId: created.cloneId,
        name: input.name,
        collectionId: created.collectionId,
        createdAt: Date.now(),
        symbol: created.properties.symbol,
        kind: collectionKind(created.properties),
        maxSupply: created.properties.max_supply,
        restrictedMinting: created.properties.minter_pubkey !== null,
        mintPolicy: mintPolicyOf(created.properties),
      });
      toast.success(`Collection "${input.name}" created`);
      this.log("success", `Created collection "${input.name}" (${created.cloneId})`);
      return created;
    } catch (e) {
      toast.error(`Failed to create collection: ${errText(e)}`);
      return null;
    }
  }

  // --- G5a: generative NFTs ---------------------------------------------
  // See docs/generative-collections.md. The seed is always DERIVED from
  // verified randomness — the user never supplies one. `dev-insecure` is the
  // only provider that exists: forgeable, development/testing only.

  /**
   * Mint a sealed NFT in a generative collection. There is no per-mint
   * commitment scheme to build here: the reveal preimage check the ordinary
   * `mint`/`reveal_nft` path uses does not apply to a generative collection
   * (see `validate_update_nft_manifest`'s generative branch), so
   * `metadata_hash` only needs to be a non-zero placeholder.
   */
  async mintGenerative(): Promise<ActionHash | null> {
    try {
      const placeholder = new Uint8Array(32);
      crypto.getRandomValues(placeholder);
      const out = await this.nft.mintNft(placeholder);
      toast.success("Minted — request randomness to generate its traits");
      this.log("success", `Minted ${shortHash(out.nft)} (generative, sealed)`);
      await Promise.all([this.loadMyNfts(), this.refreshMintStatus()]);
      return out.nft;
    } catch (e) {
      toast.error(`Mint failed: ${errText(e)}`);
      return null;
    }
  }

  /**
   * DEV-INSECURE ONLY: request/record randomness for `nft`. The "proof" is a
   * public hash anyone can compute — this is NOT secure randomness, only a
   * way to exercise the protocol. Returns the new `GenerationRandomness`
   * action hash (needed by `createGenerationRecordFor`), or `null` on failure.
   */
  async requestGenerationRandomness(nft: ActionHash): Promise<ActionHash | null> {
    try {
      const eventId = crypto.getRandomValues(new Uint8Array(16));
      const randomness = crypto.getRandomValues(new Uint8Array(32));
      const proof = await this.nft.devGenerationRandomnessProof(nft, eventId, randomness);
      const result = await this.nft.createGenerationRandomness(nft, eventId, randomness, proof);
      toast.success("Randomness recorded (dev-insecure — not production-grade)");
      this.log("info", `Recorded randomness for ${shortHash(nft)}`);
      return result;
    } catch (e) {
      toast.error(`Failed to record randomness: ${errText(e)}`);
      return null;
    }
  }

  /**
   * Derive the seed from `result` and record the generated traits. There is
   * no seed/trait input — the coordinator computes both from the cited
   * `GenerationRandomness`.
   */
  async createGenerationRecordFor(nft: ActionHash, result: ActionHash): Promise<boolean> {
    try {
      await this.nft.createGenerationRecord(nft, result);
      toast.success("Traits generated — ready to reveal");
      this.log("success", `Generated traits for ${shortHash(nft)}`);
      return true;
    } catch (e) {
      toast.error(`Failed to generate traits: ${errText(e)}`);
      return false;
    }
  }

  /**
   * Reveal a generative NFT. The attributes come from its `GenerationRecord`
   * (recomputed and checked by `validate()`); the caller supplies only the
   * image URI, which is NOT cryptographically bound to the generated traits.
   */
  async revealGenerativeNft(nft: ActionHash, imageUri: string): Promise<boolean> {
    try {
      const salt = new Uint8Array(32); // unused by the generative reveal path
      await this.nft.revealGenerated({ nft, previousActionHash: nft, salt, imageUri });
      toast.success("Revealed");
      this.log("success", `Revealed ${shortHash(nft)}`);
      await this.loadMyNfts();
      return true;
    } catch (e) {
      toast.error(`Reveal failed: ${errText(e)}`);
      return false;
    }
  }

  async markDefault(nft: ActionHash): Promise<void> {
    try {
      await this.nft.markRevealDefault(nft);
      toast.success("Marked reveal as defaulted");
      this.log("info", `Reveal defaulted for ${shortHash(nft)}`);
      await Promise.all([this.loadDrafts(), this.loadDetail(nft)]);
    } catch (e) {
      toast.error(`Could not mark defaulted: ${errText(e)}`);
    }
  }

  /**
   * The reveal secret this device parked for `nft`, as a JSON blob the minter
   * can hand to their delegate out of band (phase 1 keeps the handoff manual).
   * `null` if no secret was stored here.
   */
  async revealSecretBlob(nft: ActionHash): Promise<string | null> {
    const s = await this.nft.revealSecretFor(nft).catch(() => null);
    if (!s) return null;
    return JSON.stringify({
      nft: b64(nft),
      salt: Array.from(s.salt),
      attributes: s.attributes,
      image_uri: s.imageUri,
    });
  }

  /** Reveal as the named delegate, using a secret handed over out of band. */
  async revealAsDelegate(
    nft: ActionHash,
    secret: { salt: Uint8Array; attributes: NftAttribute[]; imageUri: string },
  ): Promise<void> {
    try {
      await this.nft.revealNft({
        nft,
        previousActionHash: nft, // first reveal branches off the mint
        salt: secret.salt,
        attributes: secret.attributes,
        imageUri: secret.imageUri,
      });
      toast.success("Revealed on the minter's behalf");
      this.log("success", `Revealed ${shortHash(nft)} as delegate`);
      await Promise.all([this.loadDelegatedReveals(), this.loadDetail(nft)]);
    } catch (e) {
      toast.error(`Reveal failed: ${errText(e)}`);
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

  /**
   * DEMO MODE — see `payment.rs`'s module doc. This does not talk to Unyt at
   * all; it's a stand-in for "the UI already read a real RAVE and is now
   * passing its computed_values through." `nftId`/`newOwner`/`previousOwner`
   * are computed by the caller from `nft`/`toAgent`/`this.me` so they can
   * never drift from what `payment.rs::nft_id_for` expects — the only thing
   * a demo operator actually chooses is `fulfilled` and who the buyer is.
   */
  async settleFromRave(
    nft: ActionHash,
    toAgent: AgentPubKey,
    fulfilled: boolean,
  ): Promise<ActionHash> {
    try {
      const commitment = await this.nft.initiateTransferFromRave({
        nft,
        toAgent,
        // No real RAVE exists — the NFT's own mint hash is a valid ActionHash
        // to carry in the audit-trail tag slot; it's never re-fetched or
        // interpreted as anything but a placeholder (see payment.rs).
        rave: nft,
        fulfilled,
        nftId: b64(nft),
        newOwner: b64(toAgent),
        previousOwner: b64(this.me),
      });
      await this.loadMyNfts();
      toast.success(`Settled (demo) — offer sent to ${shortHash(toAgent)}`);
      this.log(
        "info",
        `Offered ${shortHash(nft)} to ${shortHash(toAgent)} — settled via Unyt (demo)`,
      );
      return commitment;
    } catch (e) {
      toast.error(`Settlement failed: ${errText(e)}`);
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

  // --- social: follow / like / comment / collections --------------------

  async toggleFollow(agent: AgentPubKey): Promise<void> {
    const key = b64(agent);
    const wasFollowing = this.myFollowing.has(key);
    try {
      if (wasFollowing) {
        await this.social.unfollow(agent);
        this.myFollowing.delete(key);
      } else {
        await this.social.follow(agent);
        this.myFollowing.add(key);
      }
      this.myFollowing = new Set(this.myFollowing); // new ref for reactivity
      if (!wasFollowing) {
        toast.success(`Following ${shortHash(agent)}`);
        this.log("info", `Followed ${shortHash(agent)}`);
      }
    } catch (e) {
      toast.error(`${wasFollowing ? "Unfollow" : "Follow"} failed: ${errText(e)}`);
    }
  }

  /** No toast — likes are meant to feel instant, not noteworthy. */
  async toggleLike(target: AnyDhtHash, targetRole: string, currentlyLiked: boolean) {
    try {
      if (currentlyLiked) {
        await this.social.unlike(target);
      } else {
        await this.social.like({ target, targetRole });
      }
      return await this.social.likesFor(target);
    } catch (e) {
      toast.error(`Like failed: ${errText(e)}`);
      return null;
    }
  }

  async postComment(target: AnyDhtHash, targetRole: string, body: string): Promise<void> {
    try {
      await this.social.createComment({ target, targetRole, body });
      toast.success("Comment posted");
      this.log("info", "Posted a comment");
    } catch (e) {
      toast.error(`Comment failed: ${errText(e)}`);
      throw e;
    }
  }

  async deleteComment(comment: ActionHash): Promise<void> {
    try {
      await this.social.deleteComment(comment);
      toast.info("Comment deleted");
    } catch (e) {
      toast.error(`Delete failed: ${errText(e)}`);
      throw e;
    }
  }

  async createCollection(name: string, description: string): Promise<ActionHash> {
    try {
      const rec = await this.social.createCollection({ name, description });
      const hash = recordActionHash(rec);
      toast.success(`Created "${name}"`);
      this.log("success", `Created collection "${name}"`);
      await this.loadMyCollections();
      return hash;
    } catch (e) {
      toast.error(`Create collection failed: ${errText(e)}`);
      throw e;
    }
  }

  async updateCollection(
    previousActionHash: ActionHash,
    name: string,
    description: string,
  ): Promise<void> {
    try {
      await this.social.updateCollection({ previousActionHash, name, description });
      toast.success("Collection updated");
      await this.loadMyCollections();
    } catch (e) {
      toast.error(`Update failed: ${errText(e)}`);
      throw e;
    }
  }

  async deleteCollection(collection: ActionHash): Promise<void> {
    try {
      await this.social.deleteCollection(collection);
      toast.info("Collection deleted");
      await this.loadMyCollections();
    } catch (e) {
      toast.error(`Delete failed: ${errText(e)}`);
    }
  }

  async addToCollection(
    collection: ActionHash,
    target: AnyDhtHash,
    targetRole: string,
  ): Promise<void> {
    try {
      await this.social.addToCollection({ collection, target, targetRole });
      toast.success("Added to collection");
    } catch (e) {
      toast.error(`Add to collection failed: ${errText(e)}`);
    }
  }

  async removeFromCollection(collection: ActionHash, target: AnyDhtHash): Promise<void> {
    try {
      await this.social.removeFromCollection(collection, target);
      toast.info("Removed from collection");
    } catch (e) {
      toast.error(`Remove failed: ${errText(e)}`);
    }
  }

  // --- listings ---------------------------------------------------------

  async listItem(nft: ActionHash, price: number, currency: string): Promise<void> {
    try {
      await this.nft.createListing(nft, price, currency);
      toast.success("Listed for sale");
      if (this.listingsLoaded) void this.loadListings();
      this.log("success", `Listed ${shortHash(nft)} for ${price} ${currency}`);
      await Promise.all([this.loadMyListings(), this.loadDetail(nft)]);
    } catch (e) {
      toast.error(`Listing failed: ${errText(e)}`);
      throw e;
    }
  }

  async updateListingPrice(
    previousActionHash: ActionHash,
    price: number,
    currency: string,
    nft?: ActionHash,
  ): Promise<void> {
    try {
      await this.nft.updateListingPrice(previousActionHash, price, currency);
      toast.success("Listing updated");
      if (this.listingsLoaded) void this.loadListings();
      await Promise.all([this.loadMyListings(), nft ? this.loadDetail(nft) : Promise.resolve()]);
    } catch (e) {
      toast.error(`Update failed: ${errText(e)}`);
      throw e;
    }
  }

  async cancelListing(listing: ActionHash, nft?: ActionHash): Promise<void> {
    try {
      await this.nft.cancelListing(listing);
      toast.info("Listing cancelled");
      if (this.listingsLoaded) void this.loadListings();
      await Promise.all([this.loadMyListings(), nft ? this.loadDetail(nft) : Promise.resolve()]);
    } catch (e) {
      toast.error(`Cancel failed: ${errText(e)}`);
    }
  }

  async expressInterest(listing: ActionHash): Promise<void> {
    try {
      await this.nft.expressInterest(listing);
      toast.success("Interest sent to the seller");
      this.log("info", `Expressed interest in ${shortHash(listing)}`);
    } catch (e) {
      toast.error(`Failed to express interest: ${errText(e)}`);
    }
  }

  // --- blind fixed-inventory batches (G3a) ----------------------------------
  // See docs/blind-box-collections.md and `blind.rs`. No payment coupling, and
  // the randomness verifier is DEV-ONLY and INSECURE.

  /** Default UI batch-size cap (the protocol maximum is 1024). */
  static readonly MAX_UI_BATCH_SIZE = 256;

  /**
   * Create a batch, mint its sealed NFTs and finalize the inventory.
   * `commitments[i]` is slot `i`'s reveal commitment (build each with
   * `prepareCommitment`). Saves each slot's commitment against its minted NFT,
   * same as `mint()`, so the minter can reveal later.
   *
   * `group`, when given, adds every minted NFT to a social Collection. A
   * failure there doesn't undo the mint; it's reported as a separate toast.
   */
  async mintBlindBatch(
    commitments: Commitment[],
    group?: { name: string; description: string } | { collection: ActionHash },
  ): Promise<MintBlindBatchOutput | null> {
    try {
      const result = await this.nft.mintBlindBatch(commitments.map((c) => c.metadataHash));
      const pairs = result.nfts.map((nft, i) => [nft, commitments[i]!] as const);
      await Promise.all(pairs.map(([nft, c]) => saveCommitment(nft, c)));

      if (group) {
        try {
          const collection =
            "collection" in group
              ? group.collection
              : recordActionHash(await this.social.createCollection(group));
          await Promise.all(
            result.nfts.map((nft) =>
              this.social.addToCollection({ collection, target: nft, targetRole: this.nftRoleName }),
            ),
          );
          await this.loadMyCollections();
        } catch (e) {
          toast.error(`Minted, but grouping into a collection failed: ${errText(e)}`);
        }
      }

      toast.success(`Minted a blind batch of ${result.nfts.length}`);
      this.log("info", `Minted blind batch ${shortHash(result.batch)}`);
      await Promise.all([this.loadMyNfts(), this.loadDrafts(), this.loadBlindBatches()]);
      return result;
    } catch (e) {
      toast.error(`Failed to mint blind batch: ${errText(e)}`);
      return null;
    }
  }

  /** Buyer-side: reserve a purchase against a finalized batch. */
  async reserveBlindPurchase(batch: ActionHash): Promise<ActionHash | null> {
    try {
      const out = await this.nft.reserveBlindPurchase(batch);
      toast.success("Purchase reserved — the minter includes it when closing the batch");
      this.log("info", `Reserved blind purchase for batch ${shortHash(batch)}`);
      return out.purchase;
    } catch (e) {
      toast.error(`Failed to reserve purchase: ${errText(e)}`);
      return null;
    }
  }

  /** Minter-side: commit the purchase set and close the batch (1 <= K <= N). */
  async closeBlindBatch(batch: ActionHash): Promise<boolean> {
    try {
      await this.nft.closeBatch(batch);
      toast.success("Batch closed");
      this.log("info", `Closed blind batch ${shortHash(batch)}`);
      await this.loadBlindBatches();
      return true;
    } catch (e) {
      toast.error(`Failed to close batch: ${errText(e)}`);
      return false;
    }
  }

  /**
   * DEV ONLY / INSECURE: records browser-generated randomness for a close using
   * the dev verifier. This is NOT secure randomness and must not be treated as
   * authoritative; a real provider replaces it.
   */
  async recordDevRandomness(close: ActionHash): Promise<boolean> {
    try {
      const randomness = new Uint8Array(32);
      crypto.getRandomValues(randomness);
      await this.nft.recordDevRandomness(close, new TextEncoder().encode("dev-event"), randomness);
      toast.success("Dev randomness recorded (insecure — testing only)");
      await this.loadBlindBatches();
      return true;
    } catch (e) {
      toast.error(`Failed to record randomness: ${errText(e)}`);
      return false;
    }
  }

  /** The deterministic assignment for a batch whose randomness is fixed. */
  async loadBatchAssignments(close: ActionHash, result: ActionHash): Promise<AssignmentView[]> {
    try {
      return await this.nft.getAssignments(close, result);
    } catch (e) {
      toast.error(`Failed to compute assignments: ${errText(e)}`);
      return [];
    }
  }

  /** Minter-side: offer an assigned NFT to its assigned buyer. */
  async releaseAssignment(
    a: AssignmentView,
    close: ActionHash,
    result: ActionHash,
  ): Promise<boolean> {
    try {
      await this.nft.releaseBatchTransfer({
        nft: a.nft,
        close,
        result,
        purchase: a.purchase,
      });
      toast.success("Transfer offered to the assigned buyer");
      this.log("info", `Released ${shortHash(a.nft)} to ${shortHash(a.buyer)}`);
      return true;
    } catch (e) {
      toast.error(`Failed to release: ${errText(e)}`);
      return false;
    }
  }
}

/** Holochain guest errors arrive as `... Guest("message") ...` — pull the message. */
function errText(e: unknown): string {
  const s = String(e);
  const m = s.match(/Guest\("(.+?)"\)/);
  return m?.[1] ?? s;
}
