import { blake2b } from "blakejs";
import type { NftAttribute } from "./types.js";

/**
 * Commit-reveal helpers. These MUST reproduce `nft_integrity`'s
 * `reveal_preimage` + `commit_hash` byte-for-byte, or `reveal_nft` validation
 * rejects the reveal.
 *
 * Preimage layout (every length is a little-endian u32):
 *
 *     salt[32]
 *     attributes.len()
 *     for each attribute:  trait_type.len() ‖ trait_type (utf-8)
 *                          value.len()      ‖ value (utf-8)
 *     image_uri.len() ‖ image_uri (utf-8)
 *
 * Hash: BLAKE2b, 32-byte digest, no key/salt/personalization
 * (matches `holo_hash::blake2b_256`).
 */

export const SALT_BYTES = 32;

function getCrypto(): Crypto {
  const c = (globalThis as { crypto?: Crypto }).crypto;
  if (!c?.getRandomValues) {
    throw new Error(
      "No Web Crypto available. In older Node, `import { webcrypto } from 'node:crypto'` and assign it to globalThis.crypto.",
    );
  }
  return c;
}

/** A fresh 32-byte salt. Keep it secret until reveal; you need it again to reveal. */
export function randomSalt(): Uint8Array {
  const salt = new Uint8Array(SALT_BYTES);
  getCrypto().getRandomValues(salt);
  return salt;
}

function u32le(n: number): Uint8Array {
  if (!Number.isInteger(n) || n < 0 || n > 0xffff_ffff) {
    throw new RangeError(`length out of u32 range: ${n}`);
  }
  const b = new Uint8Array(4);
  new DataView(b.buffer).setUint32(0, n, true);
  return b;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.length;
  }
  return out;
}

export function revealPreimage(
  salt: Uint8Array,
  attributes: NftAttribute[],
  imageUri: string,
): Uint8Array {
  if (salt.length !== SALT_BYTES) {
    throw new Error(`salt must be ${SALT_BYTES} bytes, got ${salt.length}`);
  }
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [salt, u32le(attributes.length)];
  const put = (bytes: Uint8Array) => parts.push(u32le(bytes.length), bytes);
  for (const a of attributes) {
    put(enc.encode(a.trait_type));
    put(enc.encode(a.value));
  }
  put(enc.encode(imageUri));
  return concat(parts);
}

/** The 32-byte `metadata_hash` to pass to `mint_nft`. */
export function commitmentHash(
  salt: Uint8Array,
  attributes: NftAttribute[],
  imageUri: string,
): Uint8Array {
  return blake2b(revealPreimage(salt, attributes, imageUri), undefined, 32);
}

/** Convenience: everything you need to mint now and reveal later. */
export interface Commitment {
  salt: Uint8Array;
  attributes: NftAttribute[];
  imageUri: string;
  metadataHash: Uint8Array;
}

export function prepareCommitment(
  attributes: NftAttribute[],
  imageUri: string,
  salt: Uint8Array = randomSalt(),
): Commitment {
  return {
    salt,
    attributes,
    imageUri,
    metadataHash: commitmentHash(salt, attributes, imageUri),
  };
}
