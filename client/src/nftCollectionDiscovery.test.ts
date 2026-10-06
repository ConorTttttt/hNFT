import { describe, expect, it } from "vitest";
import {
  isRegisteredDnaHash,
  parseDiscoveredCollection,
  parseDiscoveredCollections,
} from "./nftCollectionDiscovery.js";
import type { NftCollectionDiscoveryView } from "./social.js";

function dnaHash(byte: number): Uint8Array {
  return new Uint8Array(39).fill(byte);
}
function agent(byte: number): Uint8Array {
  return new Uint8Array(39).fill(byte);
}

function view(overrides: Partial<NftCollectionDiscoveryView>): NftCollectionDiscoveryView {
  return {
    registration: new Uint8Array([1]),
    dna_hash: dnaHash(1),
    network_seed: "seed-1",
    properties_json: JSON.stringify({ name: "Test", symbol: "T", max_supply: 100 }),
    registered_by: agent(1),
    registered_at_micros: 1_700_000_000_000_000,
    ...overrides,
  };
}

describe("parseDiscoveredCollection", () => {
  it("parses a well-formed registration into a card", () => {
    const card = parseDiscoveredCollection(view({}));
    expect(card).not.toBeNull();
    expect(card!.properties.name).toBe("Test");
    expect(card!.properties.symbol).toBe("T");
    expect(card!.properties.max_supply).toBe(100);
    expect(card!.networkSeed).toBe("seed-1");
    expect(card!.registeredAtMs).toBe(1_700_000_000_000);
  });

  it("returns null for properties_json that isn't valid JSON", () => {
    expect(parseDiscoveredCollection(view({ properties_json: "not json" }))).toBeNull();
  });

  it("returns null for valid JSON that isn't CollectionInfo-shaped", () => {
    expect(parseDiscoveredCollection(view({ properties_json: JSON.stringify({ foo: "bar" }) }))).toBeNull();
    expect(
      parseDiscoveredCollection(view({ properties_json: JSON.stringify({ name: "X" }) })),
    ).toBeNull();
    expect(parseDiscoveredCollection(view({ properties_json: JSON.stringify([1, 2, 3]) }))).toBeNull();
    expect(parseDiscoveredCollection(view({ properties_json: "null" }))).toBeNull();
  });

  it("returns null for max_supply of the wrong type", () => {
    expect(
      parseDiscoveredCollection(
        view({ properties_json: JSON.stringify({ name: "X", symbol: "Y", max_supply: "100" }) }),
      ),
    ).toBeNull();
  });
});

describe("parseDiscoveredCollections", () => {
  it("drops malformed entries without failing the whole list", () => {
    const views = [
      view({ dna_hash: dnaHash(1) }),
      view({ dna_hash: dnaHash(2), properties_json: "garbage" }),
      view({ dna_hash: dnaHash(3) }),
    ];
    const cards = parseDiscoveredCollections(views);
    expect(cards).toHaveLength(2);
  });

  it("returns an empty array for an empty list", () => {
    expect(parseDiscoveredCollections([])).toEqual([]);
  });
});

describe("isRegisteredDnaHash", () => {
  it("is true when a registration's dna_hash matches", () => {
    const registrations = [view({ dna_hash: dnaHash(5) })];
    expect(isRegisteredDnaHash(registrations, dnaHash(5))).toBe(true);
  });

  it("is false when no registration matches", () => {
    const registrations = [view({ dna_hash: dnaHash(5) })];
    expect(isRegisteredDnaHash(registrations, dnaHash(9))).toBe(false);
  });

  it("is false for an empty registration list", () => {
    expect(isRegisteredDnaHash([], dnaHash(1))).toBe(false);
  });
});
