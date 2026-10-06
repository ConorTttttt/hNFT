import type { CollectionCardData } from "./collectionCards";

/**
 * Local, case-insensitive filtering of the collection cards already built by
 * `buildCollectionCards` — i.e. this device's own registry plus the active
 * collection. This is NOT search: it never queries a network, another clone
 * cell, or NFT data. It matches only `name`/`symbol`, the two fields the
 * Collections browser already displays.
 */
export function filterCollectionCards(
  cards: CollectionCardData[],
  query: string,
): CollectionCardData[] {
  const q = query.trim().toLowerCase();
  if (!q) return cards;
  return cards.filter(
    (c) => c.name.toLowerCase().includes(q) || (c.symbol ?? "").toLowerCase().includes(q),
  );
}
