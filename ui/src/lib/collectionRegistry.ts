/**
 * Client-side record of the NFT-collection clone cells this device has
 * created/joined (G5b). Holochain itself remembers that an installed clone
 * cell exists across restarts; this registry only remembers WHICH clone ids
 * exist and their display names, so the UI can offer a collection switcher
 * without asking the conductor to enumerate cells on every launch.
 *
 * Not a global/on-chain registry: this is per-device `localStorage`, exactly
 * like `saltStore`'s reveal commitments and the hidden-NFT list. A collection
 * created on one device will not appear on another device's switcher —
 * cross-device discovery is explicitly out of scope (see the G5b design report).
 */

import type { MintPolicy } from "@p2p-art-marketplace/client";

const STORAGE_KEY = "hnft.nft-collections.v1";

/** Visual/behavioral grouping for the Collections browser — see `collectionCards.ts`. */
export type RegisteredCollectionKind = "normal" | "generative" | "blind";

export interface RegisteredCollection {
  /** The clone id (e.g. `"nft.0"`), used as `NftClient`'s `roleName`. */
  cloneId: string;
  name: string;
  /** 64-hex `collection_id`, if this collection has one (generative/blind). */
  collectionId: string | null;
  createdAt: number;
  /** Known only for collections created through this device's Create-Collection
   * flow (`AppState.createNftCollection`), which has the properties on hand at
   * creation time. `undefined` for anything registered without them — the
   * Collections browser must not fabricate these, only omit them. */
  symbol?: string;
  kind?: RegisteredCollectionKind;
  maxSupply?: number;
  /** Known only for collections created through this device's Create-Collection
   * flow, same caveat as `symbol`/`kind`/`maxSupply` above. Superseded by
   * `mintPolicy` (Phase J.2) for anything created after it existed — kept for
   * older registry entries and back-compat with existing card fallbacks. */
  restrictedMinting?: boolean;
  /** This collection's mint policy at creation time (Phase J.2). `undefined`
   * for entries registered before this field existed — the browser falls
   * back to `restrictedMinting` for those (see `collectionCards.ts`). */
  mintPolicy?: MintPolicy;
}

function safeParse(raw: string | null): RegisteredCollection[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (c): c is RegisteredCollection =>
        !!c && typeof c === "object" && typeof (c as RegisteredCollection).cloneId === "string",
    );
  } catch {
    return [];
  }
}

/** Every registered collection, oldest first. Never throws (returns `[]` on any storage error). */
export function listRegisteredCollections(): RegisteredCollection[] {
  try {
    return safeParse(localStorage.getItem(STORAGE_KEY));
  } catch {
    return [];
  }
}

function save(list: RegisteredCollection[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  } catch {
    // Best-effort only — e.g. a private window with storage disabled. The
    // collection itself already exists on the conductor regardless.
  }
}

/** Add or update a collection by `cloneId`. */
export function registerCollection(entry: RegisteredCollection): void {
  const list = listRegisteredCollections().filter((c) => c.cloneId !== entry.cloneId);
  list.push(entry);
  save(list);
}

/** Remove a collection from the local switcher list (does not touch the cell itself). */
export function forgetCollection(cloneId: string): void {
  save(listRegisteredCollections().filter((c) => c.cloneId !== cloneId));
}
