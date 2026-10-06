import type { MintStatus } from "@p2p-art-marketplace/client";

/**
 * A short, honest reason the connected agent currently can't mint — or
 * `null` when they can (or status hasn't loaded yet). Purely a UI-facing
 * summary of the authoritative `MintStatus` (Phase J.2); never a second
 * authorization decision — the integrity validator reaches `eligible`'s
 * answer independently at mint time, and remains the source of truth if a
 * race makes this text stale for a moment.
 */
export function describeMintUnavailability(status: MintStatus | null): string | null {
  if (!status || status.eligible) return null;
  switch (status.policy) {
    case "whitelist":
      return status.my_allowance == null ? "Not whitelisted" : "Allowance exhausted";
    case "creator_only":
      return "Creator-only minting";
    case "public":
      return "Mint unavailable";
  }
}
