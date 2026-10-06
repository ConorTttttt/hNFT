import { describe, expect, it } from "vitest";
import { encode } from "@msgpack/msgpack";
import { buildNetworkNftCard, buildNetworkNftCards } from "./networkNftCards";
import type { NetworkNftReference } from "@p2p-art-marketplace/client";

function sealedRecord() {
  const manifest = { metadata_hash: Array(32).fill(1), payload: { status: "sealed" } };
  return {
    signed_action: { hashed: { hash: new Uint8Array([9]), content: { author: new Uint8Array([1]) } } },
    entry: {
      Present: {
        entry_type: "App",
        entry: encode(manifest),
      },
    },
  } as unknown as NetworkNftReference["record"];
}

function revealedRecord(name: string) {
  const manifest = {
    metadata_hash: Array(32).fill(1),
    payload: { status: "revealed", salt: Array(32).fill(2), attributes: [{ trait_type: "name", value: name }], image_uri: "ipfs://x" },
  };
  return {
    signed_action: { hashed: { hash: new Uint8Array([9]), content: { author: new Uint8Array([1]) } } },
    entry: { Present: { entry_type: "App", entry: encode(manifest) } },
  } as unknown as NetworkNftReference["record"];
}

function ref(overrides: Partial<NetworkNftReference> = {}): NetworkNftReference {
  return {
    collectionDnaHashB64: "dna1",
    collectionCloneId: "nft.1",
    collectionName: "Test Collection",
    nft: new Uint8Array([1, 2, 3]),
    record: sealedRecord(),
    ...overrides,
  };
}

describe("buildNetworkNftCard", () => {
  it("builds a card for a sealed NFT", () => {
    const card = buildNetworkNftCard(ref());
    expect(card).not.toBeNull();
    expect(card!.sealed).toBe(true);
    expect(card!.title).toBe("Sealed NFT");
    expect(card!.collectionName).toBe("Test Collection");
    expect(card!.collectionCloneId).toBe("nft.1");
  });

  it("builds a card for a revealed NFT, using its name attribute as title", () => {
    const card = buildNetworkNftCard(ref({ record: revealedRecord("Dragon #1") }));
    expect(card!.sealed).toBe(false);
    expect(card!.title).toBe("Dragon #1");
  });

  it("returns null for a record that can't be decoded", () => {
    const bad = { entry: { Absent: {} } } as unknown as NetworkNftReference["record"];
    expect(buildNetworkNftCard(ref({ record: bad }))).toBeNull();
  });

  it("keys are unique per collection+nft", () => {
    const card = buildNetworkNftCard(ref());
    expect(card!.key).toContain("dna1");
  });
});

describe("buildNetworkNftCards", () => {
  it("drops malformed entries without failing the whole list", () => {
    const bad = { entry: { Absent: {} } } as unknown as NetworkNftReference["record"];
    const cards = buildNetworkNftCards([ref(), ref({ record: bad }), ref()]);
    expect(cards).toHaveLength(2);
  });

  it("returns an empty array for an empty list", () => {
    expect(buildNetworkNftCards([])).toEqual([]);
  });
});
