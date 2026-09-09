import { decodeHashFromBase64, encodeHashToBase64 } from "@holochain/client";
import type { DecodedNftManifest } from "@p2p-art-marketplace/client";

export const b64 = (h: Uint8Array): string => encodeHashToBase64(h);
export const unb64 = (s: string): Uint8Array => decodeHashFromBase64(s);

export function shortHash(h: Uint8Array, head = 6): string {
  const s = b64(h);
  return `${s.slice(0, head)}…${s.slice(-4)}`;
}

/** A human-facing title: a `name` attribute if the NFT is revealed, else a fallback. */
export function nftTitle(m: DecodedNftManifest): string {
  if (m.payload.status === "sealed") return "Sealed NFT";
  const name = m.payload.attributes
    .find((a) => a.trait_type.trim().toLowerCase() === "name")
    ?.value.trim();
  return name || "Untitled";
}

/** "just now" / "5m ago" / "3h ago" / "2d ago". */
export function relativeTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 45_000) return "just now";
  const mins = Math.round(diff / 60_000);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.round(hrs / 24)}d ago`;
}

export function eqHash(a?: Uint8Array, b?: Uint8Array): boolean {
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** Holochain `Timestamp` is microseconds since the Unix epoch. */
export const tsToMs = (micros: number): number => Math.floor(micros / 1000);

export function formatCountdown(msRemaining: number): string {
  if (msRemaining <= 0) return "0:00";
  const total = Math.ceil(msRemaining / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
