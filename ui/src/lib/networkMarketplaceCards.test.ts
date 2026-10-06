import { describe, expect, it } from "vitest";
import { encode } from "@msgpack/msgpack";
import {
  buildNetworkMarketplaceCard,
  buildNetworkMarketplaceCards,
  filterMarketplaceCards,
  searchMarketplaceCards,
  sortMarketplaceCardsListedFirst,
  type NetworkMarketplaceCardData,
} from "./networkMarketplaceCards";
import type { NetworkMarketplaceItem } from "@p2p-art-marketplace/client";

function sealedRecord() {
  const manifest = { metadata_hash: Array(32).fill(1), payload: { status: "sealed" } };
  return {
    signed_action: { hashed: { hash: new Uint8Array([9]), content: { author: new Uint8Array([1]) } } },
    entry: { Present: { entry_type: "App", entry: encode(manifest) } },
  } as unknown as NetworkMarketplaceItem["record"];
}

function revealedRecord(name: string) {
  const manifest = {
    metadata_hash: Array(32).fill(1),
    payload: { status: "revealed", salt: Array(32).fill(2), attributes: [{ trait_type: "name", value: name }], image_uri: "ipfs://x" },
  };
  return {
    signed_action: { hashed: { hash: new Uint8Array([9]), content: { author: new Uint8Array([1]) } } },
    entry: { Present: { entry_type: "App", entry: encode(manifest) } },
  } as unknown as NetworkMarketplaceItem["record"];
}

function item(overrides: Partial<NetworkMarketplaceItem> = {}): NetworkMarketplaceItem {
  return {
    collectionDnaHashB64: "dna1",
    collectionCloneId: "nft.1",
    collectionName: "Test Collection",
    nft: new Uint8Array([1, 2, 3]),
    record: sealedRecord(),
    listing: null,
    ...overrides,
  };
}

function card(overrides: Partial<NetworkMarketplaceCardData> = {}): NetworkMarketplaceCardData {
  return {
    key: "k",
    nftIdB64: "n",
    title: "Untitled",
    sealed: false,
    collectionName: "C",
    collectionCloneId: "nft.1",
    collectionDnaHashB64: "dna1",
    listed: false,
    price: null,
    currency: null,
    ...overrides,
  };
}

describe("buildNetworkMarketplaceCard", () => {
  it("builds a card for a sealed, unlisted NFT", () => {
    const c = buildNetworkMarketplaceCard(item());
    expect(c!.sealed).toBe(true);
    expect(c!.listed).toBe(false);
    expect(c!.price).toBeNull();
  });

  it("builds a card for a revealed, listed NFT with price/currency", () => {
    const c = buildNetworkMarketplaceCard(
      item({
        record: revealedRecord("Dragon #1"),
        listing: { listing: new Uint8Array([1]), nft: new Uint8Array([1, 2, 3]), price: 250, currency: "USD", seller: new Uint8Array([1]) },
      }),
    );
    expect(c!.sealed).toBe(false);
    expect(c!.title).toBe("Dragon #1");
    expect(c!.listed).toBe(true);
    expect(c!.price).toBe(250);
    expect(c!.currency).toBe("USD");
  });

  it("returns null for an undecodable record", () => {
    const bad = { entry: { Absent: {} } } as unknown as NetworkMarketplaceItem["record"];
    expect(buildNetworkMarketplaceCard(item({ record: bad }))).toBeNull();
  });
});

describe("buildNetworkMarketplaceCards", () => {
  it("drops malformed entries without failing the whole list", () => {
    const bad = { entry: { Absent: {} } } as unknown as NetworkMarketplaceItem["record"];
    const cards = buildNetworkMarketplaceCards([item(), item({ record: bad }), item()]);
    expect(cards).toHaveLength(2);
  });
});

describe("filterMarketplaceCards", () => {
  const sealed = card({ key: "1", sealed: true });
  const revealed = card({ key: "2", sealed: false });
  const listedA = card({ key: "3", collectionCloneId: "nft.a", listed: true });
  const unlistedB = card({ key: "4", collectionCloneId: "nft.b", listed: false });
  const all = [sealed, revealed, listedA, unlistedB];

  it("'all' reveal returns every card", () => {
    expect(filterMarketplaceCards(all, { reveal: "all", collectionCloneId: null, forSaleOnly: false })).toEqual(all);
  });

  it("'unrevealed' keeps only sealed cards", () => {
    const result = filterMarketplaceCards(all, { reveal: "unrevealed", collectionCloneId: null, forSaleOnly: false });
    expect(result.every((c) => c.sealed)).toBe(true);
    expect(result).toContainEqual(sealed);
  });

  it("'revealed' keeps only non-sealed cards", () => {
    const result = filterMarketplaceCards(all, { reveal: "revealed", collectionCloneId: null, forSaleOnly: false });
    expect(result.every((c) => !c.sealed)).toBe(true);
  });

  it("collection filter keeps only the matching collection", () => {
    const result = filterMarketplaceCards(all, { reveal: "all", collectionCloneId: "nft.a", forSaleOnly: false });
    expect(result).toEqual([listedA]);
  });

  it("forSaleOnly keeps only listed cards", () => {
    const result = filterMarketplaceCards(all, { reveal: "all", collectionCloneId: null, forSaleOnly: true });
    expect(result).toEqual([listedA]);
  });

  it("composes filters as an intersection", () => {
    const result = filterMarketplaceCards(all, { reveal: "all", collectionCloneId: "nft.b", forSaleOnly: true });
    expect(result).toEqual([]);
  });
});

describe("searchMarketplaceCards", () => {
  const dragon = card({ key: "1", title: "Dragon #1", nftIdB64: "aaa" });
  const cat = card({ key: "2", title: "Cat", nftIdB64: "bbb" });
  const all = [dragon, cat];

  it("empty query returns everything", () => {
    expect(searchMarketplaceCards(all, "")).toEqual(all);
  });

  it("matches by title, case-insensitively", () => {
    expect(searchMarketplaceCards(all, "DRAGON")).toEqual([dragon]);
  });

  it("matches by nft id", () => {
    expect(searchMarketplaceCards(all, "aaa")).toEqual([dragon]);
  });

  it("returns nothing for no match", () => {
    expect(searchMarketplaceCards(all, "zzz")).toEqual([]);
  });
});

describe("sortMarketplaceCardsListedFirst", () => {
  it("groups listed cards before unlisted, stable within each group", () => {
    const a = card({ key: "a", listed: false });
    const b = card({ key: "b", listed: true });
    const c = card({ key: "c", listed: false });
    const d = card({ key: "d", listed: true });
    expect(sortMarketplaceCardsListedFirst([a, b, c, d]).map((x) => x.key)).toEqual(["b", "d", "a", "c"]);
  });

  it("never mutates the input", () => {
    const cards = [card({ key: "a" }), card({ key: "b", listed: true })];
    const copy = [...cards];
    sortMarketplaceCardsListedFirst(cards);
    expect(cards).toEqual(copy);
  });

  it("handles an empty list", () => {
    expect(sortMarketplaceCardsListedFirst([])).toEqual([]);
  });
});
