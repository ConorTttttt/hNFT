import { encodeHashToBase64 } from "@holochain/client";
import type { ActionHash, AppWebsocket, Record as HcRecord } from "@holochain/client";
import { NftClient, recordActionHash } from "./client.js";
import { openDiscoveredCollection } from "./nftCollectionDiscovery.js";
import type { DiscoveredCollectionCard } from "./nftCollectionDiscovery.js";

/**
 * Phase J.4 — network-wide NFT discovery, composed entirely from two
 * already-existing, already-tested primitives:
 *   1. J.3's collection discovery (`discoverNftCollections` /
 *      `openDiscoveredCollection`) — finds and joins a collection.
 *   2. The `nft` zome's own, UNMODIFIED `get_all_nfts()` — once a
 *      collection's clone is open, this already IS its authoritative NFT
 *      discovery/enumeration.
 *
 * There is deliberately no new DHT entry type here, and no global per-NFT
 * index: the `social` DHT would otherwise grow one durable, permanent entry
 * for every NFT ever minted, network-wide, forever — exactly the "copying
 * authoritative NFT state into the social DHT" outcome the J.4 audit's stop
 * conditions rule out, for no benefit `get_all_nfts` doesn't already give
 * once a collection is discovered+joined. See the J.4 report for the full
 * comparison of the discovery-location options considered.
 *
 * Every result here is explicitly a REFERENCE — `nft`/`record` — never a
 * resolved, owner-aware `NftView`; that resolution step belongs to whatever
 * UI-side code already builds `NftView`s for the active collection's own
 * Items view (reusing exactly the same per-item calls, not a second
 * implementation of them).
 */
export interface NetworkNftReference {
  /** Base64 DnaHash of the collection this NFT belongs to — see J.3's
   * `mintPolicyOf`/discovery types for why this, not `collection_id`, is the
   * collection's authoritative identity. */
  collectionDnaHashB64: string;
  /** The LOCAL clone id this device now has open for that collection —
   * needed to make any further authoritative call (owner, reveal state,
   * mint, …) against the right cell. */
  collectionCloneId: string;
  collectionName: string;
  /** The NFT's identity: its mint Create action hash. */
  nft: ActionHash;
  /** The raw mint record from `get_all_nfts()` — NOT necessarily the latest
   * (revealed) version; same caveat `get_all_nfts` itself already documents. */
  record: HcRecord;
}

export interface DiscoverNetworkNftsOptions {
  /** How many DISCOVERED collections to resolve NFTs from in one pass.
   * Bounds the number of clone-cell opens/`get_all_nfts` calls — this is
   * never an unbounded "scan every collection" feed. Default 6. */
  maxCollections?: number;
  /** How many NFTs to keep per collection (client-side slice — `get_all_nfts`
   * has no native limit param; Holochain link-fetch order isn't a meaningful
   * "first N" anyway, so this is a bound, not a ranking). Default 24. */
  maxNftsPerCollection?: number;
}

export interface DiscoverNetworkNftsResult {
  references: NetworkNftReference[];
  /** Collections newly opened (clone-created) during this call — the caller
   * decides whether/how to remember these locally (e.g. registering them in
   * a local collection registry); this function itself never writes to any
   * such local cache — see the J.3 "local registry is a cache, not the
   * authority" principle. */
  openedCollections: { cloneId: string; card: DiscoveredCollectionCard }[];
  /** Discovered collections that could NOT be resolved this pass (e.g. the
   * clone couldn't be created, or `get_all_nfts` failed) — surfaced instead
   * of silently dropped, so the UI can distinguish "discovered" from "unable
   * to resolve" (per the J.4 report §11) rather than just omitting them. */
  unresolvedCollections: DiscoveredCollectionCard[];
}

const DEFAULT_MAX_COLLECTIONS = 6;
const DEFAULT_MAX_NFTS_PER_COLLECTION = 24;

/**
 * Resolves a bounded set of NFT references across discovered collections.
 * `alreadyOpenCloneIdByDnaHashB64` lets the caller skip re-opening a clone it
 * already has locally (from `AppState.localCloneDnaHashes`, inverted) — a
 * collection already open is queried directly, never re-cloned.
 */
export async function discoverNetworkNfts(
  client: AppWebsocket,
  collections: DiscoveredCollectionCard[],
  alreadyOpenCloneIdByDnaHashB64: ReadonlyMap<string, string>,
  options: DiscoverNetworkNftsOptions = {},
): Promise<DiscoverNetworkNftsResult> {
  const maxCollections = options.maxCollections ?? DEFAULT_MAX_COLLECTIONS;
  const maxNftsPerCollection = options.maxNftsPerCollection ?? DEFAULT_MAX_NFTS_PER_COLLECTION;

  const references: NetworkNftReference[] = [];
  const openedCollections: { cloneId: string; card: DiscoveredCollectionCard }[] = [];
  const unresolvedCollections: DiscoveredCollectionCard[] = [];
  const seen = new Set<string>();

  let considered = 0;
  for (const card of collections) {
    if (considered >= maxCollections) break;
    considered++;

    const dnaHashB64 = encodeHashToBase64(card.dnaHash);
    let cloneId = alreadyOpenCloneIdByDnaHashB64.get(dnaHashB64);
    if (!cloneId) {
      try {
        const opened = await openDiscoveredCollection(client, card);
        cloneId = opened.cloneId;
        openedCollections.push({ cloneId, card });
      } catch {
        unresolvedCollections.push(card);
        continue;
      }
    }

    let records: HcRecord[];
    try {
      records = await new NftClient({ client, roleName: cloneId }).getAllNfts();
    } catch {
      unresolvedCollections.push(card);
      continue;
    }

    for (const record of records.slice(0, maxNftsPerCollection)) {
      const nft = recordActionHash(record);
      const key = `${dnaHashB64}:${encodeHashToBase64(nft)}`;
      if (seen.has(key)) continue;
      seen.add(key);
      references.push({
        collectionDnaHashB64: dnaHashB64,
        collectionCloneId: cloneId,
        collectionName: card.properties.name,
        nft,
        record,
      });
    }
  }

  return { references, openedCollections, unresolvedCollections };
}
