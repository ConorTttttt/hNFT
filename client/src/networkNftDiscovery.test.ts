import { describe, expect, it, vi } from "vitest";
import { discoverNetworkNfts } from "./networkNftDiscovery.js";
import type { DiscoveredCollectionCard } from "./nftCollectionDiscovery.js";
import type { AppWebsocket } from "@holochain/client";

function dnaHash(byte: number): Uint8Array {
  return new Uint8Array(39).fill(byte);
}

function card(overrides: Partial<DiscoveredCollectionCard> = {}): DiscoveredCollectionCard {
  return {
    registration: new Uint8Array([1]),
    dnaHash: dnaHash(1),
    networkSeed: "seed",
    properties: { name: "Test", symbol: "T", max_supply: 0, minter_pubkey: null, royalty_bps: 0, transfer_window_secs: 300 },
    registeredBy: new Uint8Array(39).fill(9),
    registeredAtMs: 1000,
    ...overrides,
  };
}

function fakeRecord(seed: number) {
  return {
    signed_action: {
      hashed: {
        hash: new Uint8Array(39).fill(seed),
        content: { author: new Uint8Array(39).fill(1) },
      },
    },
    entry: { Present: { entry_type: "App", entry: new Uint8Array() } },
  } as unknown;
}

/** A fake AppWebsocket. Each test overrides `createCloneCell`'s resolution
 * explicitly where it matters (matching `openDiscoveredCollection`'s own
 * hash check, already covered by its own test file) — the default here is
 * only exercised by tests that assert it's never called at all. */
function fakeClient(opts: {
  callZomeImpl?: (args: { role_name: string; fn_name: string }) => unknown;
} = {}): AppWebsocket {
  return {
    createCloneCell: vi.fn().mockRejectedValue(new Error("createCloneCell should not be called in this test")),
    callZome: vi.fn().mockImplementation((args: { fn_name: string; role_name: string }) => {
      if (opts.callZomeImpl) return Promise.resolve(opts.callZomeImpl(args));
      return Promise.resolve([]);
    }),
  } as unknown as AppWebsocket;
}

describe("discoverNetworkNfts", () => {
  it("resolves references from an already-open collection without cloning", async () => {
    const c = card({ dnaHash: dnaHash(1) });
    const client = fakeClient({
      callZomeImpl: ({ fn_name }) => (fn_name === "get_all_nfts" ? [fakeRecord(1), fakeRecord(2)] : []),
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const alreadyOpen = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    const result = await discoverNetworkNfts(client, [c], alreadyOpen);
    expect(client.createCloneCell).not.toHaveBeenCalled();
    expect(result.references).toHaveLength(2);
    expect(result.references[0]!.collectionCloneId).toBe("nft.existing");
    expect(result.references[0]!.collectionName).toBe("Test");
    expect(result.openedCollections).toHaveLength(0);
    expect(result.unresolvedCollections).toHaveLength(0);
  });

  it("opens a clone for a collection that isn't local yet", async () => {
    const c = card({ dnaHash: dnaHash(2) });
    const client = fakeClient({ callZomeImpl: () => [] });
    // Monkeypatch createCloneCell to return the SAME dnaHash as the card, so
    // openDiscoveredCollection's own hash check succeeds.
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockResolvedValue({
      clone_id: "nft.new",
      cell_id: [c.dnaHash, new Uint8Array(39)],
    });

    const result = await discoverNetworkNfts(client, [c], new Map());
    expect(client.createCloneCell).toHaveBeenCalledTimes(1);
    expect(result.openedCollections).toEqual([{ cloneId: "nft.new", card: c }]);
  });

  it("marks a collection unresolved if opening its clone fails", async () => {
    const c = card({ dnaHash: dnaHash(3) });
    const client = fakeClient();
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("boom"));

    const result = await discoverNetworkNfts(client, [c], new Map());
    expect(result.unresolvedCollections).toEqual([c]);
    expect(result.references).toEqual([]);
  });

  it("marks a collection unresolved if get_all_nfts fails after opening", async () => {
    const c = card({ dnaHash: dnaHash(4) });
    const client = fakeClient();
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockResolvedValue({
      clone_id: "nft.x",
      cell_id: [c.dnaHash, new Uint8Array(39)],
    });
    (client.callZome as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("network error"));

    const result = await discoverNetworkNfts(client, [c], new Map());
    expect(result.unresolvedCollections).toEqual([c]);
  });

  it("bounds the number of collections resolved to maxCollections", async () => {
    const cards = [1, 2, 3].map((n) => card({ dnaHash: dnaHash(n) }));
    const client = fakeClient({ callZomeImpl: () => [fakeRecord(1)] });
    let n = 0;
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockImplementation(async () => {
      n++;
      return { clone_id: `nft.${n}`, cell_id: [cards[n - 1]!.dnaHash, new Uint8Array(39)] };
    });

    const result = await discoverNetworkNfts(client, cards, new Map(), { maxCollections: 2 });
    expect(result.openedCollections).toHaveLength(2);
  });

  it("bounds the number of NFTs kept per collection to maxNftsPerCollection", async () => {
    const c = card({ dnaHash: dnaHash(5) });
    const client = fakeClient({
      callZomeImpl: ({ fn_name }) =>
        fn_name === "get_all_nfts" ? [fakeRecord(1), fakeRecord(2), fakeRecord(3), fakeRecord(4)] : [],
    });
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockResolvedValue({
      clone_id: "nft.bounded",
      cell_id: [c.dnaHash, new Uint8Array(39)],
    });

    const result = await discoverNetworkNfts(client, [c], new Map(), { maxNftsPerCollection: 2 });
    expect(result.references).toHaveLength(2);
  });

  it("deduplicates by collection+nft identity", async () => {
    const c = card({ dnaHash: dnaHash(6) });
    const client = fakeClient({
      callZomeImpl: ({ fn_name }) => (fn_name === "get_all_nfts" ? [fakeRecord(1), fakeRecord(1)] : []),
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const alreadyOpen = new Map([[encodeHashToBase64(c.dnaHash), "nft.dup"]]);

    const result = await discoverNetworkNfts(client, [c], alreadyOpen);
    expect(result.references).toHaveLength(1);
  });

  it("returns nothing for an empty collection list", async () => {
    const client = fakeClient();
    const result = await discoverNetworkNfts(client, [], new Map());
    expect(result).toEqual({ references: [], openedCollections: [], unresolvedCollections: [] });
  });
});
