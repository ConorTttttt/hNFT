import { mintPolicyOf, type CollectionInfo, type MintPolicy } from "@p2p-art-marketplace/client";
import { supplyStats, type SupplyStats } from "./collectionStats";
import type { RegisteredCollection, RegisteredCollectionKind } from "./collectionRegistry";

/**
 * Pure helpers for the Collections browser (post-G5b). No protocol call, no
 * Svelte runtime dependency — safe to unit test with plain fixtures.
 *
 * "Kind" is a display grouping only; it never changes how a bound `AppState`
 * behaves — the frozen G5a/G3a protocols already keep generative and blind
 * behavior separate regardless of what badge the browser shows.
 *
 * This is a browser-LOCAL collection library, not a global marketplace index:
 * every card comes from either this device's `collectionRegistry` (set once,
 * at creation time, on this device) or — for the collection currently bound —
 * already-loaded `AppState` data. Nothing here queries, binds, or scans any
 * OTHER registered collection's clone cell.
 */

/** Blind and generative are mutually exclusive per `genesis_self_check`
 * (a blind batch cannot exist in a generative collection); a collection with
 * neither a generation config nor a randomness spec is a plain collection. */
export function collectionKind(
  info: Pick<CollectionInfo, "generation" | "randomness"> | null | undefined,
): RegisteredCollectionKind {
  if (!info) return "normal";
  if (info.generation) return "generative";
  if (info.randomness) return "blind";
  return "normal";
}

export const COLLECTION_KIND_LABEL: Record<RegisteredCollectionKind, string> = {
  normal: "Normal",
  generative: "Generative",
  blind: "Blind boxes",
};

export const MINT_POLICY_LABEL: Record<MintPolicy, string> = {
  public: "Public mint",
  creator_only: "Creator only",
  whitelist: "Whitelist",
};

/**
 * A card's mint-policy badge for a collection that ISN'T the active one: the
 * registry only ever records what was true at creation time (`mintPolicy`,
 * Phase J.2 onward), or — for older entries registered before that field
 * existed — the coarser `restrictedMinting` boolean it replaces. `null` means
 * genuinely unknown (an old entry with neither), never a guess.
 */
function registeredMintPolicy(c: RegisteredCollection): MintPolicy | null {
  if (c.mintPolicy) return c.mintPolicy;
  if (c.restrictedMinting === true) return "creator_only";
  if (c.restrictedMinting === false) return "public";
  return null;
}

/** One card's worth of ALREADY-AVAILABLE data — never fabricated fields like
 * floor price, volume, owner count or trending, none of which this
 * application tracks. Every optional/`null` field means exactly that: not
 * known without doing something this architecture deliberately avoids
 * (binding another collection's clone cell). */
export interface CollectionCardData {
  /** `undefined` = the provisioned default role (not a registry entry). */
  cloneId: string | undefined;
  name: string;
  symbol?: string;
  kind: RegisteredCollectionKind;
  /** `null` = restriction unknown (an older registry entry without it). */
  restrictedMinting: boolean | null;
  /** `null` = genuinely unknown (an old registry entry with neither
   * `mintPolicy` nor `restrictedMinting` recorded) — see `registeredMintPolicy`. */
  mintPolicy: MintPolicy | null;
  /** Agent-specific ("can THIS agent mint"), active collection only — never a
   * collection-wide "minting open" status, because no such status exists. */
  canMint: boolean | null;
  supply: SupplyStats;
  /** Active collection only (`allListings().length`); `null` for every other
   * card, by design — computing this for an inactive collection would mean
   * binding its clone cell just to render a browser card. */
  listingCount: number | null;
  /** When this device registered the collection (local, not on-chain). `null`
   * for the provisioned default, which has no registry entry. */
  registeredAt: number | null;
  isActive: boolean;
}

export interface ActiveCollectionData {
  cloneId: string | undefined;
  collectionInfo: CollectionInfo | null;
  /** `null` = not loaded yet for this collection. */
  itemCount: number | null;
  listingCount: number | null;
  canMint: boolean | null;
}

/**
 * Build the Collections-browser card list: the provisioned default role first,
 * then every locally registered collection, in registry order (callers sort —
 * see `sortCollectionCards`). Only the currently active collection may carry
 * live data (`collectionInfo`, `itemCount`, `listingCount`, `canMint`) — cards
 * for collections that aren't bound right now show only what was stored in
 * the registry at creation time, all optional fields, per the
 * "don't activate every clone cell at once" rule.
 */
export function buildCollectionCards(
  registered: RegisteredCollection[],
  active: ActiveCollectionData,
): CollectionCardData[] {
  const defaultIsActive = active.cloneId === undefined;
  const cards: CollectionCardData[] = [
    {
      cloneId: undefined,
      name: (defaultIsActive && active.collectionInfo?.name) || "Default collection",
      symbol: defaultIsActive ? active.collectionInfo?.symbol : undefined,
      kind: defaultIsActive ? collectionKind(active.collectionInfo) : "normal",
      restrictedMinting: defaultIsActive
        ? (active.collectionInfo?.minter_pubkey ?? null) !== null
        : null,
      mintPolicy: defaultIsActive ? mintPolicyOf(active.collectionInfo) : null,
      canMint: defaultIsActive ? active.canMint : null,
      supply: supplyStats(
        defaultIsActive ? active.collectionInfo?.max_supply : undefined,
        defaultIsActive ? active.itemCount : null,
      ),
      listingCount: defaultIsActive ? active.listingCount : null,
      registeredAt: null,
      isActive: defaultIsActive,
    },
  ];
  for (const c of registered) {
    const isActive = active.cloneId === c.cloneId;
    cards.push({
      cloneId: c.cloneId,
      name: (isActive && active.collectionInfo?.name) || c.name,
      symbol: (isActive ? active.collectionInfo?.symbol : undefined) ?? c.symbol,
      kind: isActive ? collectionKind(active.collectionInfo) : (c.kind ?? "normal"),
      restrictedMinting: isActive
        ? (active.collectionInfo?.minter_pubkey ?? null) !== null
        : (c.restrictedMinting ?? null),
      mintPolicy: isActive ? mintPolicyOf(active.collectionInfo) : registeredMintPolicy(c),
      canMint: isActive ? active.canMint : null,
      supply: supplyStats(
        (isActive ? active.collectionInfo?.max_supply : undefined) ?? c.maxSupply,
        isActive ? active.itemCount : null,
      ),
      listingCount: isActive ? active.listingCount : null,
      registeredAt: c.createdAt,
      isActive,
    });
  }
  return cards;
}

/**
 * "Recently added" — a purely local ordering by this device's OWN
 * registration timestamp (`registeredAt`/`RegisteredCollection.createdAt`),
 * never presented as on-chain creation time. The active/default card (whose
 * `registeredAt` is `null` when it has no registry entry) sorts last among
 * ties, not first, so it doesn't visually dominate a "recently added" list.
 */
export function sortCollectionCardsByRecent(cards: CollectionCardData[]): CollectionCardData[] {
  return [...cards].sort((a, b) => (b.registeredAt ?? -Infinity) - (a.registeredAt ?? -Infinity));
}
