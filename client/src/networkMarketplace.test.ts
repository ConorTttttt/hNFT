import { describe, expect, it, vi } from "vitest";
import { discoverNetworkMarketplace } from "./networkMarketplace.js";
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
    properties: {
      name: "Test",
      symbol: "T",
      max_supply: 0,
      minter_pubkey: null,
      royalty_bps: 0,
      transfer_window_secs: 300,
    },
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

function listing(nftByte: number, price: number) {
  return {
    listing: new Uint8Array([100 + nftByte]),
    nft: new Uint8Array(39).fill(nftByte),
    price,
    currency: "USD",
    seller: new Uint8Array(39).fill(1),
  };
}

/** A fake AppWebsocket whose `callZome` dispatches on `fn_name` and
 * `role_name`, so a test can give different collections different NFTs and
 * listings. `createCloneCell` always "succeeds" by echoing back the card's
 * own `dnaHash` — mirroring `openDiscoveredCollection`'s success path
 * (verified separately in its own test file). */
function fakeClient(opts: {
  nftsByCloneId?: Record<string, unknown[]>;
  listingsByCloneId?: Record<string, unknown[]>;
  failListingsForCloneId?: string;
} = {}): AppWebsocket {
  let counter = 0;
  const dnaHashByCloneId = new Map<string, Uint8Array>();
  return {
    createCloneCell: vi.fn().mockImplementation(async ({ modifiers }: { modifiers: { properties: unknown } }) => {
      counter++;
      const cloneId = `nft.${counter}`;
      const props = modifiers.properties as { __dnaHash?: Uint8Array };
      const dnaHash = props.__dnaHash ?? new Uint8Array(39).fill(counter);
      dnaHashByCloneId.set(cloneId, dnaHash);
      return { clone_id: cloneId, cell_id: [dnaHash, new Uint8Array(39)] };
    }),
    callZome: vi.fn().mockImplementation(async ({ role_name, fn_name }: { role_name: string; fn_name: string }) => {
      if (fn_name === "get_all_nfts") return opts.nftsByCloneId?.[role_name] ?? [];
      if (fn_name === "all_listings") {
        if (opts.failListingsForCloneId === role_name) throw new Error("listing fetch failed");
        return opts.listingsByCloneId?.[role_name] ?? [];
      }
      return [];
    }),
  } as unknown as AppWebsocket;
}

describe("discoverNetworkMarketplace", () => {
  it("pairs a listed NFT with its listing", async () => {
    const c = card({ dnaHash: dnaHash(1) });
    const client = fakeClient({
      nftsByCloneId: { "nft.existing": [fakeRecord(5)] },
      listingsByCloneId: { "nft.existing": [listing(5, 100)] },
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    const result = await discoverNetworkMarketplace(client, [c], already);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]!.listing).not.toBeNull();
    expect(result.items[0]!.listing!.price).toBe(100);
  });

  it("leaves listing null for an NFT with no active listing", async () => {
    const c = card({ dnaHash: dnaHash(2) });
    const client = fakeClient({
      nftsByCloneId: { "nft.existing": [fakeRecord(6)] },
      listingsByCloneId: { "nft.existing": [] },
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    const result = await discoverNetworkMarketplace(client, [c], already);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]!.listing).toBeNull();
  });

  it("a stale listing (for a different NFT) does not attach to an unrelated NFT", async () => {
    const c = card({ dnaHash: dnaHash(3) });
    const client = fakeClient({
      nftsByCloneId: { "nft.existing": [fakeRecord(7)] },
      listingsByCloneId: { "nft.existing": [listing(99, 50)] }, // listing for a DIFFERENT nft
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    const result = await discoverNetworkMarketplace(client, [c], already);
    expect(result.items[0]!.listing).toBeNull();
  });

  it("a listing-fetch failure for one collection still resolves its NFTs, unlisted", async () => {
    const c = card({ dnaHash: dnaHash(4) });
    const client = fakeClient({
      nftsByCloneId: { "nft.existing": [fakeRecord(8)] },
      failListingsForCloneId: "nft.existing",
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    const result = await discoverNetworkMarketplace(client, [c], already);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]!.listing).toBeNull();
  });

  it("composes listings independently across multiple collections", async () => {
    const c1 = card({ dnaHash: dnaHash(1) });
    const c2 = card({ dnaHash: dnaHash(2) });
    const client = fakeClient({
      nftsByCloneId: { "nft.a": [fakeRecord(1)], "nft.b": [fakeRecord(2)] },
      listingsByCloneId: { "nft.a": [listing(1, 10)], "nft.b": [] },
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([
      [encodeHashToBase64(c1.dnaHash), "nft.a"],
      [encodeHashToBase64(c2.dnaHash), "nft.b"],
    ]);

    const result = await discoverNetworkMarketplace(client, [c1, c2], already);
    expect(result.items).toHaveLength(2);
    const listed = result.items.find((i) => i.collectionCloneId === "nft.a");
    const unlisted = result.items.find((i) => i.collectionCloneId === "nft.b");
    expect(listed!.listing).not.toBeNull();
    expect(unlisted!.listing).toBeNull();
  });

  it("only fetches listings once per distinct collection, not once per NFT", async () => {
    const c = card({ dnaHash: dnaHash(1) });
    const client = fakeClient({
      nftsByCloneId: { "nft.existing": [fakeRecord(1), fakeRecord(2), fakeRecord(3)] },
      listingsByCloneId: { "nft.existing": [] },
    });
    const { encodeHashToBase64 } = await import("@holochain/client");
    const already = new Map([[encodeHashToBase64(c.dnaHash), "nft.existing"]]);

    await discoverNetworkMarketplace(client, [c], already);
    const listingCalls = (client.callZome as ReturnType<typeof vi.fn>).mock.calls.filter(
      ([args]: [{ fn_name: string }]) => args.fn_name === "all_listings",
    );
    expect(listingCalls).toHaveLength(1);
  });

  it("propagates unresolvedCollections and openedCollections from discoverNetworkNfts unchanged", async () => {
    const c = card({ dnaHash: dnaHash(1) });
    const client = fakeClient();
    (client.createCloneCell as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("boom"));

    const result = await discoverNetworkMarketplace(client, [c], new Map());
    expect(result.unresolvedCollections).toEqual([c]);
    expect(result.items).toEqual([]);
    expect(result.openedCollections).toEqual([]);
  });

  it("returns nothing for an empty collection list", async () => {
    const client = fakeClient();
    const result = await discoverNetworkMarketplace(client, [], new Map());
    expect(result).toEqual({ items: [], openedCollections: [], unresolvedCollections: [] });
  });
});
