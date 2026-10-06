import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  forgetCollection,
  listRegisteredCollections,
  registerCollection,
} from "./collectionRegistry";

/** Minimal in-memory `localStorage` — avoids pulling in a DOM test environment
 * just for this one Web Storage API. */
function makeMemoryStorage(): Storage {
  const store = new Map<string, string>();
  return {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => void store.set(k, v),
    removeItem: (k) => void store.delete(k),
    clear: () => store.clear(),
    key: (i) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  } as Storage;
}

beforeEach(() => {
  vi.stubGlobal("localStorage", makeMemoryStorage());
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe("collectionRegistry", () => {
  it("starts empty", () => {
    expect(listRegisteredCollections()).toEqual([]);
  });

  it("registers and lists a collection", () => {
    registerCollection({ cloneId: "nft.0", name: "Drop One", collectionId: null, createdAt: 1 });
    expect(listRegisteredCollections()).toEqual([
      { cloneId: "nft.0", name: "Drop One", collectionId: null, createdAt: 1 },
    ]);
  });

  it("updates in place by cloneId rather than duplicating", () => {
    registerCollection({ cloneId: "nft.0", name: "Old Name", collectionId: null, createdAt: 1 });
    registerCollection({ cloneId: "nft.0", name: "New Name", collectionId: "ab".repeat(32), createdAt: 1 });
    const list = listRegisteredCollections();
    expect(list).toHaveLength(1);
    expect(list[0]!.name).toBe("New Name");
  });

  it("forgets a collection without touching others", () => {
    registerCollection({ cloneId: "nft.0", name: "A", collectionId: null, createdAt: 1 });
    registerCollection({ cloneId: "nft.1", name: "B", collectionId: null, createdAt: 2 });
    forgetCollection("nft.0");
    expect(listRegisteredCollections().map((c) => c.cloneId)).toEqual(["nft.1"]);
  });

  it("survives corrupted storage by returning an empty list", () => {
    localStorage.setItem("hnft.nft-collections.v1", "{not json");
    expect(listRegisteredCollections()).toEqual([]);
  });

  it("ignores non-array or malformed entries in storage", () => {
    localStorage.setItem("hnft.nft-collections.v1", JSON.stringify({ not: "an array" }));
    expect(listRegisteredCollections()).toEqual([]);
    localStorage.setItem("hnft.nft-collections.v1", JSON.stringify([{ noCloneId: true }, null, 5]));
    expect(listRegisteredCollections()).toEqual([]);
  });
});
