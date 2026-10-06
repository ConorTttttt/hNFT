import { describe, expect, it } from "vitest";
import {
  buildCollectionCards,
  collectionKind,
  COLLECTION_KIND_LABEL,
  sortCollectionCardsByRecent,
} from "./collectionCards";
import type { RegisteredCollection } from "./collectionRegistry";

describe("collectionKind", () => {
  it("is normal for null/undefined or a plain collection", () => {
    expect(collectionKind(null)).toBe("normal");
    expect(collectionKind(undefined)).toBe("normal");
    expect(collectionKind({ generation: null, randomness: null })).toBe("normal");
  });

  it("is generative when a generation config is present", () => {
    expect(collectionKind({ generation: { generation_version: 1, categories: [] }, randomness: null })).toBe(
      "generative",
    );
  });

  it("is blind when randomness is present without a generation config", () => {
    expect(
      collectionKind({ generation: null, randomness: { provider_id: "dev-insecure", verifier_params: "" } }),
    ).toBe("blind");
  });

  it("prefers generative over blind if both happen to be set (defensive; the protocol forbids this combination)", () => {
    expect(
      collectionKind({
        generation: { generation_version: 1, categories: [] },
        randomness: { provider_id: "dev-insecure", verifier_params: "" },
      }),
    ).toBe("generative");
  });

  it("has a label for every kind", () => {
    for (const kind of ["normal", "generative", "blind"] as const) {
      expect(COLLECTION_KIND_LABEL[kind]).toBeTruthy();
    }
  });
});

describe("buildCollectionCards", () => {
  const registered: RegisteredCollection[] = [
    {
      cloneId: "nft.0",
      name: "Genesis",
      collectionId: null,
      createdAt: 100,
      symbol: "GEN",
      kind: "normal",
      maxSupply: 100,
      restrictedMinting: true,
    },
    { cloneId: "nft.1", name: "Mystery", collectionId: "ab".repeat(32), createdAt: 200, kind: "blind" },
  ];
  const noActiveData = { cloneId: undefined, collectionInfo: null, itemCount: null, listingCount: null, canMint: null };

  it("always includes the default collection first", () => {
    const cards = buildCollectionCards([], noActiveData);
    expect(cards).toHaveLength(1);
    expect(cards[0]!.cloneId).toBeUndefined();
    expect(cards[0]!.name).toBe("Default collection");
    expect(cards[0]!.isActive).toBe(true);
    expect(cards[0]!.registeredAt).toBeNull();
  });

  it("lists every registered collection after the default, carrying registry-known fields", () => {
    const cards = buildCollectionCards(registered, noActiveData);
    expect(cards.map((c) => c.cloneId)).toEqual([undefined, "nft.0", "nft.1"]);
    expect(cards[1]!.name).toBe("Genesis");
    expect(cards[1]!.symbol).toBe("GEN");
    expect(cards[1]!.supply.max).toBe(100);
    expect(cards[1]!.restrictedMinting).toBe(true);
    expect(cards[1]!.registeredAt).toBe(100);
    expect(cards[2]!.kind).toBe("blind");
    expect(cards[2]!.restrictedMinting).toBeNull(); // not recorded at creation time
  });

  it("marks exactly the active collection, and only it carries live data", () => {
    const cards = buildCollectionCards(registered, {
      cloneId: "nft.0",
      collectionInfo: {
        name: "Genesis Live",
        symbol: "GEN2",
        max_supply: 500,
        minter_pubkey: "some-key",
        royalty_bps: 0,
        transfer_window_secs: 300,
        generation: null,
        randomness: null,
      },
      itemCount: 42,
      listingCount: 7,
      canMint: true,
    });
    const active = cards.find((c) => c.cloneId === "nft.0")!;
    expect(active.isActive).toBe(true);
    expect(active.name).toBe("Genesis Live");
    expect(active.symbol).toBe("GEN2");
    expect(active.supply).toEqual({ max: 500, unlimited: false, minted: 42, remaining: 458 });
    expect(active.listingCount).toBe(7);
    expect(active.canMint).toBe(true);
    expect(active.restrictedMinting).toBe(true);

    const inactive = cards.find((c) => c.cloneId === "nft.1")!;
    expect(inactive.isActive).toBe(false);
    expect(inactive.supply.minted).toBeNull();
    expect(inactive.listingCount).toBeNull();
    expect(inactive.canMint).toBeNull();
    expect(inactive.name).toBe("Mystery"); // falls back to the registered name, never fabricated
    expect(cards[0]!.isActive).toBe(false); // the default is not active when nft.0 is
  });

  it("never invents item/listing counts or mint capability for a collection that isn't bound", () => {
    const cards = buildCollectionCards(registered, noActiveData);
    for (const c of cards.filter((c) => !c.isActive)) {
      expect(c.supply.minted).toBeNull();
      expect(c.listingCount).toBeNull();
      expect(c.canMint).toBeNull();
    }
  });
});

describe("sortCollectionCardsByRecent", () => {
  it("orders by registeredAt descending", () => {
    const cards = buildCollectionCards(
      [
        { cloneId: "a", name: "A", collectionId: null, createdAt: 100 },
        { cloneId: "b", name: "B", collectionId: null, createdAt: 300 },
        { cloneId: "c", name: "C", collectionId: null, createdAt: 200 },
      ],
      { cloneId: undefined, collectionInfo: null, itemCount: null, listingCount: null, canMint: null },
    );
    const sorted = sortCollectionCardsByRecent(cards);
    expect(sorted.map((c) => c.cloneId)).toEqual(["b", "c", "a", undefined]);
  });

  it("does not mutate the input array", () => {
    const cards = buildCollectionCards([], {
      cloneId: undefined,
      collectionInfo: null,
      itemCount: null,
      listingCount: null,
      canMint: null,
    });
    const copy = [...cards];
    sortCollectionCardsByRecent(cards);
    expect(cards).toEqual(copy);
  });
});
