import type { AppWebsocket } from "@holochain/client";
import type { ClonedCell } from "@holochain/client";
import { decodeHashFromBase64 } from "@holochain/client";
import type { CollectionInfo, GenerationConfig, MintPolicy, WhitelistEntry } from "./types.js";

/**
 * Runtime NFT-collection creation (G5b), via Holochain's own `createCloneCell` —
 * NOT a new/exported `dna.yaml`. See `docs/generative-collections.md` and
 * `docs/blind-box-collections.md` for the protocol these properties configure.
 *
 * The DNA-hash-affecting `network_seed` is generated here and never surfaced to
 * the caller: two collections with identical `properties` would otherwise
 * collide on the same DNA hash (and therefore the same DHT).
 */

/** The role this app clones per NFT collection (`workdir/happ.yaml`). */
export const NFT_COLLECTION_ROLE = "nft";

export interface CreateNftCollectionInput {
  name: string;
  symbol: string;
  /** `0` = unlimited (only enforceable with a fixed `minter_pubkey`). */
  maxSupply: number;
  transferWindowSecs: number;
  /** `0` disables the (UI-only) blind-purchase-abandoned signal. */
  blindResolutionWindowSecs?: number;
  /**
   * Base64 `AgentPubKey` of the sole minter, or `undefined` for open (dev-only)
   * minting. Required in practice for a generative or blind collection, since
   * `collection_id`/`RandomnessSpec` alone don't gate authorship — see
   * `dna_properties.rs`. Ignored once `whitelist` is non-empty (Phase J.1/J.2):
   * WHITELIST mode always takes priority over `minterPubkey`.
   */
  minterPubkey?: string;
  /**
   * Per-agent mint whitelist (Phase J.1/J.2). Omit/empty for PUBLIC or
   * CREATOR_ONLY (governed by `minterPubkey` as before). Non-empty switches
   * this collection to WHITELIST mode for its entire lifetime — there is no
   * post-creation whitelist editing (see `validateWhitelistEntries` for the
   * same invariants the protocol enforces authoritatively at mint time).
   */
  whitelist?: WhitelistEntry[];
  /** Present (and `Some`) only for a generative collection. */
  generation?: GenerationConfig;
  /**
   * Present whenever the collection needs G3a blind batches or G5a generative
   * randomness. `provider_id` is currently always `dev-insecure` — see
   * `randomnessWarning`.
   */
  randomness?: { providerId: string; verifierParams?: string };
}

/** 32 random bytes as 64 lowercase hex chars — this collection's `collection_id`. */
export function generateCollectionId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** A fresh, hidden DNA `network_seed` — never shown to or entered by the user. */
function generateNetworkSeed(): string {
  return crypto.randomUUID();
}

function toProperties(input: CreateNftCollectionInput, collectionId: string): CollectionInfo {
  return {
    name: input.name,
    symbol: input.symbol,
    max_supply: input.maxSupply,
    minter_pubkey: input.minterPubkey ?? null,
    royalty_bps: 0,
    transfer_window_secs: input.transferWindowSecs,
    blind_resolution_window_secs: input.blindResolutionWindowSecs ?? 0,
    collection_id: input.generation || input.randomness ? collectionId : null,
    randomness: input.randomness
      ? { provider_id: input.randomness.providerId, verifier_params: input.randomness.verifierParams ?? "" }
      : null,
    generation: input.generation ?? null,
    whitelist: input.whitelist && input.whitelist.length > 0 ? input.whitelist : null,
  };
}

/**
 * This collection's mint-authorization mode, mirroring the protocol's own
 * `CollectionProperties::mint_mode()` resolution exactly: WHITELIST (non-empty
 * `whitelist`) takes priority over `minter_pubkey`, otherwise a set
 * `minter_pubkey` means CREATOR_ONLY, otherwise PUBLIC. Display/UI-gating
 * only — the integrity validator resolves this independently at mint time.
 */
export function mintPolicyOf(
  info: Pick<CollectionInfo, "minter_pubkey" | "whitelist"> | null | undefined,
): MintPolicy {
  if (!info) return "public";
  if (info.whitelist && info.whitelist.length > 0) return "whitelist";
  return info.minter_pubkey ? "creator_only" : "public";
}

/**
 * Client-side USABILITY validation for a whitelist editor — mirrors the same
 * invariants `CollectionProperties::mint_mode` enforces authoritatively
 * (`dna_properties.rs`), so a creator gets immediate feedback instead of a
 * rejected `createCloneCell` call. This is not a security boundary: the
 * protocol re-validates independently regardless of what this returns.
 * Returns a list of human-readable issues; empty means no issues found.
 */
export function validateWhitelistEntries(entries: WhitelistEntry[], maxSupply: number): string[] {
  const issues: string[] = [];
  const seen = new Set<string>();
  let total = 0;
  for (const e of entries) {
    const agent = e.agent.trim();
    if (!agent) {
      issues.push("Every whitelist entry needs an agent public key.");
      continue;
    }
    // `decodeHashFromBase64` itself is lenient (plain base64 decode, no
    // format check) — check the decoded shape ourselves: 39 bytes, with the
    // 3-byte AgentPubKey type prefix. Usability only; not a security check.
    let bytes: Uint8Array;
    try {
      bytes = decodeHashFromBase64(agent);
    } catch {
      bytes = new Uint8Array(0);
    }
    if (bytes.length !== 39 || bytes[0] !== 0x84 || bytes[1] !== 0x20 || bytes[2] !== 0x24) {
      issues.push(`"${agent}" is not a valid agent public key.`);
      continue;
    }
    if (!Number.isInteger(e.allowance) || e.allowance < 1) {
      issues.push(`${agent}: allowance must be a positive whole number (omit the entry instead of 0).`);
      continue;
    }
    if (seen.has(agent)) {
      issues.push(`${agent} appears more than once in the whitelist.`);
      continue;
    }
    seen.add(agent);
    total += e.allowance;
  }
  if (maxSupply > 0 && total > maxSupply) {
    issues.push(
      `Allocated allowance (${total}) exceeds the collection's max supply (${maxSupply}).`,
    );
  }
  return issues;
}

export interface CreatedNftCollection {
  cloneId: string;
  cellId: ClonedCell["cell_id"];
  collectionId: string | null;
  properties: CollectionInfo;
}

/**
 * Create a new NFT-collection clone cell. Properties are set on the clone at
 * creation time (before it does anything else), so `genesis_self_check`
 * (invalid `collection_id`/`RandomnessSpec`/`GenerationConfig`) rejects a bad
 * configuration by rejecting this call — nothing partially exists.
 */
export async function createNftCollection(
  client: AppWebsocket,
  input: CreateNftCollectionInput,
): Promise<CreatedNftCollection> {
  const collectionId = generateCollectionId();
  const properties = toProperties(input, collectionId);
  const clone = await client.createCloneCell({
    role_name: NFT_COLLECTION_ROLE,
    modifiers: {
      network_seed: generateNetworkSeed(),
      properties: properties as unknown as Record<string, unknown>,
    },
    name: input.name,
  });
  return {
    cloneId: clone.clone_id,
    cellId: clone.cell_id,
    collectionId: properties.collection_id ?? null,
    properties,
  };
}

/** User-facing warning for the only randomness provider that currently exists. */
export const DEV_INSECURE_WARNING =
  "This randomness source is for development/testing only and is forgeable. It must not be used for production collections.";
