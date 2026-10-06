import { describe, expect, it, vi } from "vitest";
import {
  createNftCollection,
  generateCollectionId,
  mintPolicyOf,
  NFT_COLLECTION_ROLE,
  validateWhitelistEntries,
} from "./collections.js";
import type { AppWebsocket } from "@holochain/client";
import { encodeHashToBase64 } from "@holochain/client";

function fakeAgent(byte: number): string {
  const raw = new Uint8Array(39);
  raw.set([0x84, 0x20, 0x24]); // AgentPubKey prefix
  raw.fill(byte, 3, 35);
  return encodeHashToBase64(raw);
}

function fakeClient(impl?: Partial<AppWebsocket>): AppWebsocket {
  return {
    createCloneCell: vi.fn().mockResolvedValue({
      clone_id: "nft.0",
      cell_id: ["dna-hash", "agent-key"],
      original_dna_hash: "dna-hash",
      dna_modifiers: {},
      name: "Test",
      enabled: true,
    }),
    ...impl,
  } as unknown as AppWebsocket;
}

describe("generateCollectionId", () => {
  it("produces 64 lowercase hex characters", () => {
    const id = generateCollectionId();
    expect(id).toMatch(/^[0-9a-f]{64}$/);
  });

  it("is different on every call", () => {
    expect(generateCollectionId()).not.toBe(generateCollectionId());
  });
});

describe("createNftCollection", () => {
  it("clones the NFT role and hides the network_seed from the caller", async () => {
    const client = fakeClient();
    const result = await createNftCollection(client, {
      name: "My Drop",
      symbol: "DROP",
      maxSupply: 100,
      transferWindowSecs: 300,
    });

    expect(client.createCloneCell).toHaveBeenCalledTimes(1);
    const call = (client.createCloneCell as ReturnType<typeof vi.fn>).mock.calls[0]![0];
    expect(call.role_name).toBe(NFT_COLLECTION_ROLE);
    expect(typeof call.modifiers.network_seed).toBe("string");
    expect(call.modifiers.network_seed.length).toBeGreaterThan(0);
    // The caller never sees the network_seed anywhere in the result.
    expect(JSON.stringify(result)).not.toContain(call.modifiers.network_seed);

    expect(result.cloneId).toBe("nft.0");
    expect(result.properties.name).toBe("My Drop");
    expect(result.properties.max_supply).toBe(100);
  });

  it("omits collection_id and randomness for a plain (non-generative, non-blind) collection", async () => {
    const client = fakeClient();
    const result = await createNftCollection(client, {
      name: "Plain",
      symbol: "PLN",
      maxSupply: 0,
      transferWindowSecs: 300,
    });
    expect(result.properties.collection_id).toBeNull();
    expect(result.properties.randomness).toBeNull();
    expect(result.properties.generation).toBeNull();
  });

  it("sets collection_id and randomness for a generative collection", async () => {
    const client = fakeClient();
    const generation = {
      generation_version: 1,
      categories: [{ id: "eyes", display_name: "Eyes", traits: [{ id: "a", display_name: "A", weight: 1 }] }],
    };
    const result = await createNftCollection(client, {
      name: "Generative",
      symbol: "GEN",
      maxSupply: 0,
      transferWindowSecs: 300,
      generation,
      randomness: { providerId: "dev-insecure" },
    });
    expect(result.properties.collection_id).toMatch(/^[0-9a-f]{64}$/);
    expect(result.properties.randomness).toEqual({ provider_id: "dev-insecure", verifier_params: "" });
    expect(result.properties.generation).toEqual(generation);
    expect(result.collectionId).toBe(result.properties.collection_id);
  });

  it("propagates rejection when the conductor refuses the properties (e.g. genesis_self_check)", async () => {
    const client = fakeClient({
      createCloneCell: vi.fn().mockRejectedValue(new Error("Guest(\"invalid generation config: ...\")")),
    });
    await expect(
      createNftCollection(client, { name: "Bad", symbol: "BAD", maxSupply: 0, transferWindowSecs: 300 }),
    ).rejects.toThrow(/invalid generation config/);
  });

  it("passes a non-empty whitelist through as-is", async () => {
    const alice = fakeAgent(1);
    const client = fakeClient();
    const result = await createNftCollection(client, {
      name: "WL",
      symbol: "WL",
      maxSupply: 10,
      transferWindowSecs: 300,
      whitelist: [{ agent: alice, allowance: 5 }],
    });
    expect(result.properties.whitelist).toEqual([{ agent: alice, allowance: 5 }]);
  });

  it("normalizes an empty whitelist array to null (PUBLIC/CREATOR_ONLY, not an empty WHITELIST)", async () => {
    const client = fakeClient();
    const result = await createNftCollection(client, {
      name: "Empty WL",
      symbol: "EWL",
      maxSupply: 0,
      transferWindowSecs: 300,
      whitelist: [],
    });
    expect(result.properties.whitelist).toBeNull();
  });
});

describe("mintPolicyOf", () => {
  it("is public when neither minter_pubkey nor whitelist is set", () => {
    expect(mintPolicyOf({ minter_pubkey: null, whitelist: null })).toBe("public");
    expect(mintPolicyOf(null)).toBe("public");
    expect(mintPolicyOf(undefined)).toBe("public");
  });

  it("is creator_only when minter_pubkey is set and there is no whitelist", () => {
    expect(mintPolicyOf({ minter_pubkey: "some-key", whitelist: null })).toBe("creator_only");
    expect(mintPolicyOf({ minter_pubkey: "some-key", whitelist: [] })).toBe("creator_only");
  });

  it("is whitelist whenever the whitelist is non-empty, even with a minter_pubkey set", () => {
    expect(
      mintPolicyOf({
        minter_pubkey: "some-key",
        whitelist: [{ agent: "a", allowance: 1 }],
      }),
    ).toBe("whitelist");
    expect(
      mintPolicyOf({ minter_pubkey: null, whitelist: [{ agent: "a", allowance: 1 }] }),
    ).toBe("whitelist");
  });
});

describe("validateWhitelistEntries", () => {
  const alice = fakeAgent(1);
  const bob = fakeAgent(2);

  it("accepts a well-formed whitelist within max_supply", () => {
    expect(
      validateWhitelistEntries(
        [
          { agent: alice, allowance: 5 },
          { agent: bob, allowance: 10 },
        ],
        100,
      ),
    ).toEqual([]);
  });

  it("accepts any total when max_supply is 0 (unlimited)", () => {
    expect(validateWhitelistEntries([{ agent: alice, allowance: 1_000_000 }], 0)).toEqual([]);
  });

  it("flags a missing agent key", () => {
    const issues = validateWhitelistEntries([{ agent: "", allowance: 5 }], 0);
    expect(issues).toHaveLength(1);
    expect(issues[0]).toMatch(/agent public key/);
  });

  it("flags an invalid agent key encoding", () => {
    const issues = validateWhitelistEntries([{ agent: "not-a-valid-key", allowance: 5 }], 0);
    expect(issues).toHaveLength(1);
    expect(issues[0]).toMatch(/not a valid agent public key/);
  });

  it("flags a zero or non-integer allowance", () => {
    expect(validateWhitelistEntries([{ agent: alice, allowance: 0 }], 0)[0]).toMatch(/positive whole number/);
    expect(validateWhitelistEntries([{ agent: alice, allowance: 1.5 }], 0)[0]).toMatch(/positive whole number/);
    expect(validateWhitelistEntries([{ agent: alice, allowance: -1 }], 0)[0]).toMatch(/positive whole number/);
  });

  it("flags a duplicate agent", () => {
    const issues = validateWhitelistEntries(
      [
        { agent: alice, allowance: 5 },
        { agent: alice, allowance: 1 },
      ],
      0,
    );
    expect(issues.some((i) => i.includes("more than once"))).toBe(true);
  });

  it("flags allowances summing above max_supply", () => {
    const issues = validateWhitelistEntries(
      [
        { agent: alice, allowance: 5 },
        { agent: bob, allowance: 10 },
      ],
      14,
    );
    expect(issues.some((i) => i.includes("exceeds"))).toBe(true);
  });

  it("returns no issues for an empty whitelist", () => {
    expect(validateWhitelistEntries([], 100)).toEqual([]);
  });
});
