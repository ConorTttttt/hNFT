import { describe, expect, it } from "vitest";
import { filterNfts, nftMatchesQuery, searchNfts, sortNfts } from "./nftFiltering";
import { b64 } from "./format";
import type { NftView } from "./appState.svelte";

const me = new Uint8Array([1, 1, 1]);
const other = new Uint8Array([2, 2, 2]);

function nft(overrides: Partial<NftView> & { id: number }): NftView {
  const base = {
    nft: new Uint8Array([overrides.id]),
    latest: {} as never,
    minter: me,
    owner: me,
    hasSalt: false,
    hidden: false,
    manifest: { metadata_hash: [], payload: { status: "sealed" } },
  } as unknown as NftView;
  return { ...base, ...overrides };
}

function revealed(id: number, name: string, owner = me): NftView {
  return nft({
    id,
    owner,
    manifest: {
      metadata_hash: [],
      payload: {
        status: "revealed",
        salt: [],
        attributes: [{ trait_type: "name", value: name }],
        image_uri: "",
      },
    } as never,
  });
}

describe("filterNfts", () => {
  const sealedMine = nft({ id: 1, owner: me, manifest: { metadata_hash: [], payload: { status: "sealed" } } as never });
  const sealedOther = nft({ id: 2, owner: other, manifest: { metadata_hash: [], payload: { status: "sealed" } } as never });
  const revealedMine = nft({
    id: 3,
    owner: me,
    manifest: {
      metadata_hash: [],
      payload: { status: "revealed", salt: [], attributes: [], image_uri: "" },
    } as never,
  });
  const all = [sealedMine, sealedOther, revealedMine];

  it("'all' returns every view unchanged", () => {
    expect(filterNfts(all, "all", me)).toEqual(all);
  });

  it("'mine' keeps only NFTs owned by `me`", () => {
    expect(filterNfts(all, "mine", me)).toEqual([sealedMine, revealedMine]);
  });

  it("'unrevealed' keeps only sealed NFTs, regardless of owner", () => {
    expect(filterNfts(all, "unrevealed", me)).toEqual([sealedMine, sealedOther]);
  });

  it("'revealed' keeps only revealed NFTs, regardless of owner", () => {
    expect(filterNfts(all, "revealed", me)).toEqual([revealedMine]);
  });

  it("an empty list stays empty for every filter", () => {
    for (const f of ["all", "mine", "unrevealed", "revealed"] as const) {
      expect(filterNfts([], f, me)).toEqual([]);
    }
  });
});

describe("searchNfts / nftMatchesQuery", () => {
  const dragon12 = revealed(1, "Dragon #12", me);
  const ancientDragon = revealed(2, "Ancient Dragon", other);
  const dragonfly = revealed(3, "dragonfly", me);
  const sealedOne = nft({ id: 4, owner: me, manifest: { metadata_hash: [], payload: { status: "sealed" } } as never });
  const all = [dragon12, ancientDragon, dragonfly, sealedOne];

  it("empty query returns every view", () => {
    expect(searchNfts(all, "")).toEqual(all);
  });

  it("whitespace-only query returns every view", () => {
    expect(searchNfts(all, "   ")).toEqual(all);
  });

  it("matches a title substring", () => {
    expect(searchNfts(all, "dragon")).toEqual([dragon12, ancientDragon, dragonfly]);
  });

  it("title matching is case-insensitive", () => {
    expect(searchNfts(all, "DRAGON")).toEqual([dragon12, ancientDragon, dragonfly]);
    expect(searchNfts(all, "Dragon")).toEqual(searchNfts(all, "dragon"));
  });

  it("matches an NFT id substring (the same base64 id NftDetail's 'Copy NFT ID' shows)", () => {
    const id = b64(sealedOne.nft);
    expect(searchNfts(all, id)).toEqual([sealedOne]);
    expect(searchNfts(all, id.slice(0, 3))).toContainEqual(sealedOne);
  });

  it("id matching is case-insensitive", () => {
    const id = b64(sealedOne.nft);
    expect(searchNfts(all, id.toUpperCase())).toEqual([sealedOne]);
  });

  it("returns nothing for a query with no matches", () => {
    expect(searchNfts(all, "xyz-nonexistent")).toEqual([]);
  });

  it("trims surrounding whitespace before matching", () => {
    expect(searchNfts(all, "  dragonfly  ")).toEqual([dragonfly]);
  });

  it("a sealed (unrevealed) NFT is searchable by its placeholder title", () => {
    expect(searchNfts([sealedOne], "sealed")).toEqual([sealedOne]);
  });

  it("nftMatchesQuery agrees with searchNfts on a single view", () => {
    expect(nftMatchesQuery(dragon12, "dragon")).toBe(true);
    expect(nftMatchesQuery(sealedOne, "dragon")).toBe(false);
  });
});

describe("search composed with existing filters", () => {
  const myDragon = revealed(1, "My Dragon", me);
  const otherDragon = revealed(2, "Other Dragon", other);
  const mySealed = nft({ id: 3, owner: me, manifest: { metadata_hash: [], payload: { status: "sealed" } } as never });
  const myRevealedCat = revealed(4, "My Cat", me);
  const all = [myDragon, otherDragon, mySealed, myRevealedCat];

  it("search + 'mine' intersects ownership and text match", () => {
    const result = searchNfts(filterNfts(all, "mine", me), "dragon");
    expect(result).toEqual([myDragon]);
  });

  it("search + 'unrevealed' intersects reveal status and text match", () => {
    const result = searchNfts(filterNfts(all, "unrevealed", me), "sealed");
    expect(result).toEqual([mySealed]);
  });

  it("search + 'revealed' intersects reveal status and text match", () => {
    const result = searchNfts(filterNfts(all, "revealed", me), "dragon");
    expect(result).toEqual([myDragon, otherDragon]);
  });

  it("search narrows 'all' to a plain substring match", () => {
    expect(searchNfts(filterNfts(all, "all", me), "cat")).toEqual([myRevealedCat]);
  });

  it("a query matching nothing in the filtered set returns empty, not the unfiltered match", () => {
    // "dragon" matches items outside the "mine" set too, but composition must
    // only return the intersection, not fall back to a wider search.
    const onlyOthers = [otherDragon];
    expect(searchNfts(filterNfts(onlyOthers, "mine", me), "dragon")).toEqual([]);
  });
});

describe("sortNfts", () => {
  const a = nft({ id: 1 });
  const b = nft({ id: 2 });
  const c = nft({ id: 3 });
  const d = nft({ id: 4 });
  const all = [a, b, c, d];
  const listedIds = new Set([b64(b.nft), b64(d.nft)]);

  it("'default' returns the input unchanged (identity, no reorder)", () => {
    expect(sortNfts(all, "default", listedIds)).toEqual(all);
  });

  it("'default' does not require or consult the listed-ids set", () => {
    expect(sortNfts(all, "default", new Set())).toEqual(all);
  });

  it("'listed-first' groups listed items before unlisted, each group stable", () => {
    expect(sortNfts(all, "listed-first", listedIds)).toEqual([b, d, a, c]);
  });

  it("'listed-first' with no listings at all leaves order unchanged", () => {
    expect(sortNfts(all, "listed-first", new Set())).toEqual(all);
  });

  it("'listed-first' with everything listed leaves order unchanged", () => {
    const everyoneListed = new Set(all.map((v) => b64(v.nft)));
    expect(sortNfts(all, "listed-first", everyoneListed)).toEqual(all);
  });

  it("never mutates the input array", () => {
    const copy = [...all];
    sortNfts(all, "listed-first", listedIds);
    expect(all).toEqual(copy);
  });

  it("never widens or narrows the result — same elements, only reordered", () => {
    const result = sortNfts(all, "listed-first", listedIds);
    expect(result).toHaveLength(all.length);
    expect(new Set(result)).toEqual(new Set(all));
  });

  it("handles empty input", () => {
    expect(sortNfts([], "listed-first", listedIds)).toEqual([]);
    expect(sortNfts([], "default", listedIds)).toEqual([]);
  });

  it("handles single-item input", () => {
    expect(sortNfts([a], "listed-first", listedIds)).toEqual([a]);
    expect(sortNfts([b], "listed-first", listedIds)).toEqual([b]);
  });

  it("composes with filtering/search: sorts only what survived them", () => {
    const revealedOnly = filterNfts(all, "revealed", me); // all are sealed here
    expect(sortNfts(revealedOnly, "listed-first", listedIds)).toEqual([]);
  });
});
