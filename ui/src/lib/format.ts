import { decodeHashFromBase64, encodeHashToBase64 } from "@holochain/client";

export const b64 = (h: Uint8Array): string => encodeHashToBase64(h);
export const unb64 = (s: string): Uint8Array => decodeHashFromBase64(s);

export function shortHash(h: Uint8Array, head = 10): string {
  const s = b64(h);
  return `${s.slice(0, head)}…${s.slice(-4)}`;
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
