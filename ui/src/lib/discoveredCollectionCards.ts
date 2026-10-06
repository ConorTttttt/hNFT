import {
  mintPolicyOf,
  type DiscoveredCollectionCard,
  type MintPolicy,
} from "@p2p-art-marketplace/client";
import { b64 } from "./format";
import { collectionKind } from "./collectionCards";
import type { RegisteredCollectionKind } from "./collectionRegistry";

/**
 * Phase J.3 — pure card-building for the network-discovery section of the
 * Collections browser. No protocol call, no Svelte dependency; separate from
 * `collectionCards.ts` (this device's LOCAL registry) on purpose — see
 * `AppState.discoveredCards`/`localCloneDnaHashes` for where the inputs come
 * from and `dnas/social/.../nft_collection_registry.rs` for what "discovered"
 * does and doesn't prove.
 */
export interface NetworkCollectionCardData {
  registration: Uint8Array;
  /** Base64 DnaHash — the collection's true identity (see the protocol doc). */
  dnaHashB64: string;
  name: string;
  symbol?: string;
  kind: RegisteredCollectionKind;
  mintPolicy: MintPolicy;
  /** `0` = unlimited, mirroring `CollectionProperties.max_supply`'s own convention. */
  maxSupply: number;
  registeredAtMs: number;
  /** `true` iff this device already has a matching clone installed — distinguishes
   * "Available locally" from "Discovered" per the J.3 report §10. */
  availableLocally: boolean;
  /** The original card, needed to actually open it (`AppState.openDiscovered`). */
  source: DiscoveredCollectionCard;
}

/**
 * Builds one card per discovered collection. `localDnaHashesB64` is every
 * DnaHash (base64) this device already has an installed `nft` clone for
 * (`AppState.localCloneDnaHashes`'s values) — used ONLY to set
 * `availableLocally`, never to filter anything out: a collection this device
 * already has is still shown, just labeled differently (§10).
 */
export function buildNetworkCollectionCards(
  discovered: DiscoveredCollectionCard[],
  localDnaHashesB64: ReadonlySet<string>,
): NetworkCollectionCardData[] {
  return discovered.map((d) => {
    const dnaHashB64 = b64(d.dnaHash);
    return {
      registration: d.registration,
      dnaHashB64,
      name: d.properties.name,
      symbol: d.properties.symbol,
      kind: collectionKind(d.properties),
      mintPolicy: mintPolicyOf(d.properties),
      maxSupply: d.properties.max_supply,
      registeredAtMs: d.registeredAtMs,
      availableLocally: localDnaHashesB64.has(dnaHashB64),
      source: d,
    };
  });
}

/** Newest-registered first. */
export function sortNetworkCardsByRecent(
  cards: NetworkCollectionCardData[],
): NetworkCollectionCardData[] {
  return [...cards].sort((a, b) => b.registeredAtMs - a.registeredAtMs);
}

/** Local, in-memory name/symbol search — same shape as `filterCollectionCards`. */
export function searchNetworkCards(
  cards: NetworkCollectionCardData[],
  query: string,
): NetworkCollectionCardData[] {
  const q = query.trim().toLowerCase();
  if (!q) return cards;
  return cards.filter(
    (c) => c.name.toLowerCase().includes(q) || (c.symbol ?? "").toLowerCase().includes(q),
  );
}
