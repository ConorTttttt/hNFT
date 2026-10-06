import { describe, expect, it } from "vitest";
import { filterCollectionCards } from "./collectionFiltering";
import type { CollectionCardData } from "./collectionCards";
import { supplyStats } from "./collectionStats";

function card(overrides: Partial<CollectionCardData>): CollectionCardData {
  return {
    cloneId: "nft.0",
    name: "Untitled",
    kind: "normal",
    restrictedMinting: null,
    mintPolicy: null,
    canMint: null,
    supply: supplyStats(undefined, null),
    listingCount: null,
    registeredAt: null,
    isActive: false,
    ...overrides,
  };
}

describe("filterCollectionCards", () => {
  const artBlocks = card({ cloneId: "nft.0", name: "Art Blocks", symbol: "AB" });
  const myArt = card({ cloneId: "nft.1", name: "My Art Collection", symbol: "MAC" });
  const genesis = card({ cloneId: "nft.2", name: "Genesis", symbol: "GEN" });
  const all = [artBlocks, myArt, genesis];

  it("returns every card for an empty query", () => {
    expect(filterCollectionCards(all, "")).toEqual(all);
  });

  it("returns every card for a whitespace-only query", () => {
    expect(filterCollectionCards(all, "   ")).toEqual(all);
  });

  it("matches by name, case-insensitively", () => {
    expect(filterCollectionCards(all, "art")).toEqual([artBlocks, myArt]);
    expect(filterCollectionCards(all, "ART")).toEqual([artBlocks, myArt]);
  });

  it("matches by symbol, case-insensitively", () => {
    expect(filterCollectionCards(all, "gen")).toEqual([genesis]);
    expect(filterCollectionCards(all, "GEN")).toEqual([genesis]);
  });

  it("trims surrounding whitespace before matching", () => {
    expect(filterCollectionCards(all, "  genesis  ")).toEqual([genesis]);
  });

  it("returns an empty list when nothing matches", () => {
    expect(filterCollectionCards(all, "xyz")).toEqual([]);
  });

  it("does not match against fields other than name/symbol", () => {
    // A query matching only cloneId/kind/etc. must not match.
    expect(filterCollectionCards(all, "nft.0")).toEqual([]);
    expect(filterCollectionCards(all, "generative")).toEqual([]);
  });

  it("tolerates a card with no symbol", () => {
    const noSymbol = card({ cloneId: "nft.3", name: "No Symbol Here", symbol: undefined });
    expect(filterCollectionCards([noSymbol], "symbol")).toEqual([noSymbol]);
    expect(filterCollectionCards([noSymbol], "zzz")).toEqual([]);
  });
});
