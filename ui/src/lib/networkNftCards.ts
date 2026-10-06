import { decodeNftManifest, type NetworkNftReference } from "@p2p-art-marketplace/client";
import { b64, nftTitle } from "./format";

/**
 * Phase J.4 — pure display-card building for the bounded "network items
 * preview" (see `discoverNetworkNfts`). Deliberately NOT a full `NftView`:
 * this only decodes the raw mint record `discoverNetworkNfts` already
 * fetched — no owner/reveal-chain resolution, no per-item extra calls beyond
 * what discovery already did. See the J.4 report §10 for why a discovery
 * reference must never masquerade as fully-resolved NFT state.
 */
export interface NetworkNftCardData {
  key: string;
  nftIdB64: string;
  title: string;
  sealed: boolean;
  collectionName: string;
  collectionCloneId: string;
}

/** `null` if the record can't be decoded as an `NftManifest` at all — dropped,
 * not fatal to the rest of a preview list (same "never fabricate" rule as
 * `parseDiscoveredCollection`). */
export function buildNetworkNftCard(ref: NetworkNftReference): NetworkNftCardData | null {
  let manifest;
  try {
    manifest = decodeNftManifest(ref.record);
  } catch {
    return null;
  }
  const nftIdB64 = b64(ref.nft);
  return {
    key: `${ref.collectionDnaHashB64}:${nftIdB64}`,
    nftIdB64,
    title: nftTitle(manifest),
    sealed: manifest.payload.status === "sealed",
    collectionName: ref.collectionName,
    collectionCloneId: ref.collectionCloneId,
  };
}

export function buildNetworkNftCards(refs: NetworkNftReference[]): NetworkNftCardData[] {
  const out: NetworkNftCardData[] = [];
  for (const ref of refs) {
    const card = buildNetworkNftCard(ref);
    if (card) out.push(card);
  }
  return out;
}
