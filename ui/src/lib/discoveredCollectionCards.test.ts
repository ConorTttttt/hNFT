import { describe, expect, it } from "vitest";
import {
  buildNetworkCollectionCards,
  searchNetworkCards,
  sortNetworkCardsByRecent,
} from "./discoveredCollectionCards";
import type { DiscoveredCollectionCard } from "@p2p-art-marketplace/client";

function card(overrides: Partial<DiscoveredCollectionCard> = {}): DiscoveredCollectionCard {
  return {
    registration: new Uint8Array([1]),
    dnaHash: new Uint8Array(39).fill(1),
    networkSeed: "seed",
    properties: { name: "Art Blocks", symbol: "AB", max_supply: 100, minter_pubkey: null, royalty_bps: 0, transfer_window_secs: 300 },
    registeredBy: new Uint8Array(39).fill(2),
    registeredAtMs: 1000,
    ...overrides,
  };
}

describe("buildNetworkCollectionCards", () => {
  it("maps every field from the discovered card", () => {
    const [c] = buildNetworkCollectionCards([card()], new Set());
    expect(c!.name).toBe("Art Blocks");
    expect(c!.symbol).toBe("AB");
    expect(c!.maxSupply).toBe(100);
    expect(c!.mintPolicy).toBe("public");
    expect(c!.kind).toBe("normal");
    expect(c!.registeredAtMs).toBe(1000);
  });

  it("marks availableLocally true when the dna hash is in the local set", () => {
    const d = card();
    // Derive the expected base64 from the module's own encoding (no need to
    // duplicate holochain's hash-encoding scheme here).
    const [built] = buildNetworkCollectionCards([d], new Set());
    const [withMatch] = buildNetworkCollectionCards([d], new Set([built!.dnaHashB64]));
    expect(withMatch!.availableLocally).toBe(true);
  });

  it("marks availableLocally false when no local dna hash matches", () => {
    const [c] = buildNetworkCollectionCards([card()], new Set(["something-else"]));
    expect(c!.availableLocally).toBe(false);
  });

  it("returns an empty array for no discovered collections", () => {
    expect(buildNetworkCollectionCards([], new Set())).toEqual([]);
  });
});

describe("sortNetworkCardsByRecent", () => {
  it("orders newest registeredAtMs first", () => {
    const cards = buildNetworkCollectionCards(
      [card({ registeredAtMs: 100 }), card({ registeredAtMs: 300 }), card({ registeredAtMs: 200 })],
      new Set(),
    );
    expect(sortNetworkCardsByRecent(cards).map((c) => c.registeredAtMs)).toEqual([300, 200, 100]);
  });

  it("does not mutate the input", () => {
    const cards = buildNetworkCollectionCards([card({ registeredAtMs: 1 }), card({ registeredAtMs: 2 })], new Set());
    const copy = [...cards];
    sortNetworkCardsByRecent(cards);
    expect(cards).toEqual(copy);
  });
});

describe("searchNetworkCards", () => {
  const cards = buildNetworkCollectionCards(
    [
      card({ properties: { name: "Art Blocks", symbol: "AB", max_supply: 0, minter_pubkey: null, royalty_bps: 0, transfer_window_secs: 300 } }),
      card({ properties: { name: "My Art", symbol: "MA", max_supply: 0, minter_pubkey: null, royalty_bps: 0, transfer_window_secs: 300 } }),
      card({ properties: { name: "Genesis", symbol: "GEN", max_supply: 0, minter_pubkey: null, royalty_bps: 0, transfer_window_secs: 300 } }),
    ],
    new Set(),
  );

  it("returns every card for an empty query", () => {
    expect(searchNetworkCards(cards, "")).toEqual(cards);
  });

  it("matches by name, case-insensitively", () => {
    expect(searchNetworkCards(cards, "art").map((c) => c.name)).toEqual(["Art Blocks", "My Art"]);
  });

  it("matches by symbol", () => {
    expect(searchNetworkCards(cards, "gen").map((c) => c.name)).toEqual(["Genesis"]);
  });

  it("returns nothing for a query with no matches", () => {
    expect(searchNetworkCards(cards, "xyz")).toEqual([]);
  });
});
