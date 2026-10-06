import { CellType, encodeHashToBase64 } from "@holochain/client";
import type { AgentPubKey, AppWebsocket, DnaHash } from "@holochain/client";
import { decode } from "@msgpack/msgpack";
import { NFT_COLLECTION_ROLE } from "./collections.js";
import type { SocialClient, NftCollectionDiscoveryView } from "./social.js";
import type { CollectionInfo } from "./types.js";

/**
 * Phase J.3 — network-wide NFT-collection discovery, client side.
 *
 * The discovery record itself (`social`'s `NftCollectionRegistration`) lives
 * one layer below this: this module is what turns "the DHT has this record"
 * into "the local conductor now has a usable clone cell", and back. See
 * `dnas/social/zomes/integrity/social/src/nft_collection_registry.rs` for
 * the full protocol/authorization-boundary writeup.
 */

/** A discovery record, resolved into something the UI can render or open —
 * `null` from `parseDiscoveredCollection` means "malformed, drop it", never
 * a thrown error that would take down the whole discovery list. */
export interface DiscoveredCollectionCard {
  registration: import("@holochain/client").ActionHash;
  dnaHash: DnaHash;
  networkSeed: string;
  properties: CollectionInfo;
  registeredBy: AgentPubKey;
  registeredAtMs: number;
}

function looksLikeCollectionInfo(v: unknown): v is CollectionInfo {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return typeof o.name === "string" && typeof o.symbol === "string" && typeof o.max_supply === "number";
}

/** Never throws — a malformed `properties_json` (or one that doesn't even
 * parse) yields `null` rather than corrupting the rest of a discovery list. */
export function parseDiscoveredCollection(view: NftCollectionDiscoveryView): DiscoveredCollectionCard | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(view.properties_json);
  } catch {
    return null;
  }
  if (!looksLikeCollectionInfo(parsed)) return null;
  return {
    registration: view.registration,
    dnaHash: view.dna_hash,
    networkSeed: view.network_seed,
    properties: parsed,
    registeredBy: view.registered_by,
    registeredAtMs: Math.floor(view.registered_at_micros / 1000),
  };
}

export function parseDiscoveredCollections(views: NftCollectionDiscoveryView[]): DiscoveredCollectionCard[] {
  const out: DiscoveredCollectionCard[] = [];
  for (const v of views) {
    const c = parseDiscoveredCollection(v);
    if (c) out.push(c);
  }
  return out;
}

/**
 * Publish (or re-publish) discovery for an `nft` clone this device ALREADY
 * has installed — `cloneId` as `App.svelte`'s collection switcher already
 * knows it. Reads `network_seed`/`properties`/`dna_hash` LIVE from the local
 * conductor's own clone-cell record (`appInfo()`), never from anything
 * remembered separately — this is also exactly what makes retroactively
 * publishing a collection created before this feature existed possible with
 * no migration step: if the clone is still installed (Holochain keeps
 * installed clones across restarts), its live modifiers are still right
 * here to read.
 */
export async function publishNftCollectionForDiscovery(
  client: AppWebsocket,
  social: SocialClient,
  cloneId: string,
): Promise<import("@holochain/client").ActionHash> {
  const appInfo = await client.appInfo();
  const cellInfos = appInfo?.cell_info[NFT_COLLECTION_ROLE] ?? [];
  const clone = cellInfos.find((c) => c.type === CellType.Cloned && c.value.clone_id === cloneId);
  if (!clone || clone.type !== CellType.Cloned) {
    throw new Error(`no installed "${NFT_COLLECTION_ROLE}" clone with id ${cloneId}`);
  }
  const { cell_id, dna_modifiers } = clone.value;
  const properties = decode(dna_modifiers.properties) as CollectionInfo;
  const record = await social.registerNftCollection({
    dnaHash: cell_id[0],
    networkSeed: dna_modifiers.network_seed,
    propertiesJson: JSON.stringify(properties),
  });
  return record.signed_action.hashed.hash;
}

/** `true` once `cloneId` has been separately registered for discovery. */
export function isRegisteredDnaHash(registrations: NftCollectionDiscoveryView[], dnaHash: DnaHash): boolean {
  const target = encodeHashToBase64(dnaHash);
  return registrations.some((r) => encodeHashToBase64(r.dna_hash) === target);
}

export interface OpenedDiscoveredCollection {
  cloneId: string;
  cellId: import("@holochain/client").ClonedCell["cell_id"];
}

/**
 * Reconstruct the IDENTICAL `nft` clone a discovered collection points to —
 * `createCloneCell` again, called with the SAME `network_seed`/`properties`
 * the creator published, which is the only mechanism this Holochain client
 * actually offers for "join a DNA another peer already created" (there is
 * no "connect by DnaHash alone" API — a clone cell is local conductor state,
 * reconstructed from its ingredients, not fetched from the network as a
 * blob). If this device already has a clone with that history, `createCloneCell`
 * will fail (duplicate); callers should check `isRegisteredDnaHash`-style
 * local state first where practical.
 *
 * Verifies the resulting `cell_id[0]` (the actual DnaHash Holochain computed)
 * equals `card.dnaHash` before returning — this is the ONLY point in the
 * whole pipeline that can actually confirm addressing information wasn't
 * corrupted or mismatched, since neither `social`'s validators nor this
 * client can independently compute a DnaHash without asking the conductor to
 * do exactly this. A mismatch throws rather than silently binding to the
 * wrong collection.
 */
export async function openDiscoveredCollection(
  client: AppWebsocket,
  card: DiscoveredCollectionCard,
): Promise<OpenedDiscoveredCollection> {
  const clone = await client.createCloneCell({
    role_name: NFT_COLLECTION_ROLE,
    modifiers: {
      network_seed: card.networkSeed,
      properties: card.properties as unknown as Record<string, unknown>,
    },
    name: card.properties.name,
  });
  const gotDnaHash = encodeHashToBase64(clone.cell_id[0]);
  const wantDnaHash = encodeHashToBase64(card.dnaHash);
  if (gotDnaHash !== wantDnaHash) {
    throw new Error(
      "reconstructed clone's DnaHash does not match the discovery record — refusing to bind to it " +
        "(the registration's network_seed/properties may be corrupted or forged)",
    );
  }
  return { cloneId: clone.clone_id, cellId: clone.cell_id };
}
