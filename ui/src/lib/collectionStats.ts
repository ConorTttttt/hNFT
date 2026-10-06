import type { ListingView } from "@p2p-art-marketplace/client";

/**
 * Pure collection-statistics helpers for the collection Overview. Every value
 * is derived from an existing authoritative source — `collection_info`'s
 * `max_supply`, `getAllNfts()` (minted) and `allListings()` (for sale). No
 * floor price, volume, owner count or ranking exists in this application, so
 * none is computed here.
 */

export interface SupplyStats {
  /** `null` = unlimited (`max_supply == 0`) or not yet known. */
  max: number | null;
  unlimited: boolean;
  /** `null` = not loaded yet. */
  minted: number | null;
  /** `null` = unlimited or not computable yet. Never negative. */
  remaining: number | null;
}

export function supplyStats(
  maxSupply: number | null | undefined,
  minted: number | null,
): SupplyStats {
  if (maxSupply === null || maxSupply === undefined) {
    return { max: null, unlimited: false, minted, remaining: null };
  }
  if (maxSupply === 0) {
    return { max: null, unlimited: true, minted, remaining: null };
  }
  return {
    max: maxSupply,
    unlimited: false,
    minted,
    remaining: minted === null ? null : Math.max(0, maxSupply - minted),
  };
}

/** `"12 HOT"` — the listing's own free-text price/currency, unmodified. */
export function formatListingPrice(listing: Pick<ListingView, "price" | "currency">): string {
  return listing.currency ? `${listing.price} ${listing.currency}` : String(listing.price);
}
