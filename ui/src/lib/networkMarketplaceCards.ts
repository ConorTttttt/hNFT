import { decodeNftManifest, type NetworkMarketplaceItem } from "@p2p-art-marketplace/client";
import { b64, nftTitle } from "./format";

/**
 * Phase J.5 — pure card-building/filtering for the Network Marketplace view.
 * Deliberately its own small type, not a reused/stretched `NftView`: a
 * marketplace item has no resolved `owner` (that would mean an extra
 * `current_owner()` call per item, which this phase's lightweight-reference
 * model explicitly avoids — see `discoverNetworkMarketplace`'s doc). Never
 * fabricate that field; the "mine" filter is therefore not offered here (see
 * the J.5 report §7) — only what J.4/J.5's existing calls actually resolved.
 */
export interface NetworkMarketplaceCardData {
  key: string;
  nftIdB64: string;
  title: string;
  sealed: boolean;
  collectionName: string;
  collectionCloneId: string;
  collectionDnaHashB64: string;
  /** From that collection's own authoritative `all_listings()` — never
   * inferred. `null` = not currently listed. */
  listed: boolean;
  price: number | null;
  currency: string | null;
}

/** `null` if the record can't be decoded as an `NftManifest` — dropped, not
 * fatal to the rest of a marketplace load (same rule as `networkNftCards.ts`). */
export function buildNetworkMarketplaceCard(
  item: NetworkMarketplaceItem,
): NetworkMarketplaceCardData | null {
  let manifest;
  try {
    manifest = decodeNftManifest(item.record);
  } catch {
    return null;
  }
  return {
    key: `${item.collectionDnaHashB64}:${b64(item.nft)}`,
    nftIdB64: b64(item.nft),
    title: nftTitle(manifest),
    sealed: manifest.payload.status === "sealed",
    collectionName: item.collectionName,
    collectionCloneId: item.collectionCloneId,
    collectionDnaHashB64: item.collectionDnaHashB64,
    listed: item.listing !== null,
    price: item.listing?.price ?? null,
    currency: item.listing?.currency ?? null,
  };
}

export function buildNetworkMarketplaceCards(
  items: NetworkMarketplaceItem[],
): NetworkMarketplaceCardData[] {
  const out: NetworkMarketplaceCardData[] = [];
  for (const item of items) {
    const card = buildNetworkMarketplaceCard(item);
    if (card) out.push(card);
  }
  return out;
}

/** Only the reveal states `NftManifest` actually carries — "mine" is
 * intentionally absent (see this module's doc comment). */
export type MarketplaceRevealFilter = "all" | "unrevealed" | "revealed";

export interface MarketplaceFilterOptions {
  reveal: MarketplaceRevealFilter;
  /** `null` = every collection. */
  collectionCloneId: string | null;
  forSaleOnly: boolean;
}

export function filterMarketplaceCards(
  cards: NetworkMarketplaceCardData[],
  opts: MarketplaceFilterOptions,
): NetworkMarketplaceCardData[] {
  return cards.filter((c) => {
    if (opts.reveal === "unrevealed" && !c.sealed) return false;
    if (opts.reveal === "revealed" && c.sealed) return false;
    if (opts.collectionCloneId && c.collectionCloneId !== opts.collectionCloneId) return false;
    if (opts.forSaleOnly && !c.listed) return false;
    return true;
  });
}

/** Local, in-memory title/id search — same shape as `nftFiltering.ts`'s
 * `searchNfts` for a single collection's Items view. */
export function searchMarketplaceCards(
  cards: NetworkMarketplaceCardData[],
  query: string,
): NetworkMarketplaceCardData[] {
  const q = query.trim().toLowerCase();
  if (!q) return cards;
  return cards.filter(
    (c) => c.title.toLowerCase().includes(q) || c.nftIdB64.toLowerCase().includes(q),
  );
}

/**
 * "Listed first" — the only cross-collection ordering supported, mirroring
 * `nftFiltering.ts`'s own `sortNfts("listed-first")` semantics exactly
 * (stable partition, listed group first). Price and mint-time sorting are
 * deliberately NOT offered: `nftFiltering.ts` already established that
 * cross-listing prices can't be safely compared (free-text `currency`, no
 * normalization) and that no reliable mint timestamp exists — nothing about
 * having more collections changes either fact, so this module does not
 * re-attempt them.
 */
export function sortMarketplaceCardsListedFirst(
  cards: NetworkMarketplaceCardData[],
): NetworkMarketplaceCardData[] {
  const listed: NetworkMarketplaceCardData[] = [];
  const unlisted: NetworkMarketplaceCardData[] = [];
  for (const c of cards) (c.listed ? listed : unlisted).push(c);
  return [...listed, ...unlisted];
}
