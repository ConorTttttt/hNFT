import { encodeHashToBase64 } from "@holochain/client";
import type { AppWebsocket } from "@holochain/client";
import { NftClient } from "./client.js";
import type { DiscoveredCollectionCard } from "./nftCollectionDiscovery.js";
import {
  discoverNetworkNfts,
  type DiscoverNetworkNftsOptions,
  type NetworkNftReference,
} from "./networkNftDiscovery.js";
import type { ListingView } from "./types.js";

/**
 * Phase J.5 — network marketplace composition, built strictly ON TOP of
 * J.4's `discoverNetworkNfts` (unmodified — see its own file). This adds
 * exactly one more authoritative call per resolved collection: its own
 * `all_listings()`, already backed by `active_listing_view()`
 * (`dnas/nft/zomes/coordinator/nft/src/listing.rs`), which re-derives
 * `current_owner()` on every read and drops any listing whose original
 * author no longer owns the NFT. This module never re-implements that
 * check — a listing here is exactly what that collection's own coordinator
 * currently considers active, nothing more, nothing cached past this call.
 *
 * There is still no new DHT entry type, no global index, and no NFT/listing
 * identity beyond J.4's `(collectionDnaHashB64, nft)` composite key.
 */
export interface NetworkMarketplaceItem extends NetworkNftReference {
  /** `null` = no active listing for this NFT right now, per that
   * collection's own authoritative `all_listings()` — never inferred or
   * cached beyond this load. */
  listing: ListingView | null;
}

export interface DiscoverNetworkMarketplaceResult {
  items: NetworkMarketplaceItem[];
  openedCollections: { cloneId: string; card: DiscoveredCollectionCard }[];
  unresolvedCollections: DiscoveredCollectionCard[];
}

/**
 * Resolves a bounded marketplace view: `discoverNetworkNfts`'s existing
 * bounded NFT references, each paired with its collection's own current
 * listing (if any). One extra `all_listings()` call per DISTINCT resolved
 * collection (not per NFT) — never a query per item.
 */
export async function discoverNetworkMarketplace(
  client: AppWebsocket,
  collections: DiscoveredCollectionCard[],
  alreadyOpenCloneIdByDnaHashB64: ReadonlyMap<string, string>,
  options: DiscoverNetworkNftsOptions = {},
): Promise<DiscoverNetworkMarketplaceResult> {
  const nftResult = await discoverNetworkNfts(
    client,
    collections,
    alreadyOpenCloneIdByDnaHashB64,
    options,
  );

  const cloneIds = new Set(nftResult.references.map((r) => r.collectionCloneId));
  const listingsByClone = new Map<string, ListingView[]>();
  for (const cloneId of cloneIds) {
    try {
      const listings = await new NftClient({ client, roleName: cloneId }).allListings();
      listingsByClone.set(cloneId, listings);
    } catch {
      // Best-effort: a listing-fetch failure doesn't drop that collection's
      // NFTs from the marketplace, it just means none of them show as listed.
      listingsByClone.set(cloneId, []);
    }
  }

  const items: NetworkMarketplaceItem[] = nftResult.references.map((ref) => {
    const listings = listingsByClone.get(ref.collectionCloneId) ?? [];
    const nftB64 = encodeHashToBase64(ref.nft);
    const listing = listings.find((l) => encodeHashToBase64(l.nft) === nftB64) ?? null;
    return { ...ref, listing };
  });

  return {
    items,
    openedCollections: nftResult.openedCollections,
    unresolvedCollections: nftResult.unresolvedCollections,
  };
}

// Re-exported so callers of this module don't also need to import from
// `networkNftDiscovery.js` just for the shared reference shape.
export type { NetworkNftReference };
