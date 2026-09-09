import type { NftAttribute } from "@p2p-art-marketplace/client";
import { b64 } from "./format";

/**
 * Reveal-commitment persistence.
 *
 * A sealed NFT can only ever be revealed by whoever holds the original `salt`
 * (plus the exact attributes + image URI). That data never goes on-chain before
 * reveal, so if this store is lost the NFT is permanently sealed. Treat it as
 * important user data — back it up / export it in a real deployment.
 *
 * Keyed by the NFT's stable id (its mint `ActionHash`, base64).
 */

const DB_NAME = "p2p-art-marketplace";
const STORE = "reveal-commitments";
const DB_VERSION = 1;

export interface StoredCommitment {
  nftB64: string;
  /** 32 bytes. */
  salt: number[];
  attributes: NftAttribute[];
  imageUri: string;
  createdAt: number;
  revealedAt: number | null;
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(STORE)) {
        db.createObjectStore(STORE, { keyPath: "nftB64" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function run<T>(
  mode: IDBTransactionMode,
  op: (store: IDBObjectStore) => IDBRequest,
): Promise<T> {
  const db = await openDb();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const req = op(tx.objectStore(STORE));
      req.onsuccess = () => resolve(req.result as T);
      req.onerror = () => reject(req.error);
    });
  } finally {
    db.close();
  }
}

export async function saveCommitment(
  nft: Uint8Array,
  c: { salt: Uint8Array; attributes: NftAttribute[]; imageUri: string },
): Promise<void> {
  const rec: StoredCommitment = {
    nftB64: b64(nft),
    salt: Array.from(c.salt),
    attributes: c.attributes,
    imageUri: c.imageUri,
    createdAt: Date.now(),
    revealedAt: null,
  };
  await run("readwrite", (s) => s.put(rec));
}

export async function loadCommitment(
  nft: Uint8Array,
): Promise<StoredCommitment | null> {
  const rec = await run<StoredCommitment | undefined>("readonly", (s) =>
    s.get(b64(nft)),
  );
  return rec ?? null;
}

export async function hasCommitment(nft: Uint8Array): Promise<boolean> {
  return (await loadCommitment(nft)) !== null;
}

export async function markRevealed(nft: Uint8Array): Promise<void> {
  const rec = await loadCommitment(nft);
  if (rec) await run("readwrite", (s) => s.put({ ...rec, revealedAt: Date.now() }));
}

export async function deleteCommitment(nft: Uint8Array): Promise<void> {
  await run("readwrite", (s) => s.delete(b64(nft)));
}

export async function listCommitments(): Promise<StoredCommitment[]> {
  return (await run<StoredCommitment[]>("readonly", (s) => s.getAll())) ?? [];
}
