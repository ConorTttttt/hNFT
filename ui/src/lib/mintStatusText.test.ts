import { describe, expect, it } from "vitest";
import { describeMintUnavailability } from "./mintStatusText";
import type { MintStatus } from "@p2p-art-marketplace/client";

function status(overrides: Partial<MintStatus>): MintStatus {
  return { policy: "public", eligible: true, minted_by_me: 0, my_allowance: null, ...overrides };
}

describe("describeMintUnavailability", () => {
  it("is null when status hasn't loaded yet", () => {
    expect(describeMintUnavailability(null)).toBeNull();
  });

  it("is null whenever eligible, regardless of policy", () => {
    for (const policy of ["public", "creator_only", "whitelist"] as const) {
      expect(describeMintUnavailability(status({ policy, eligible: true }))).toBeNull();
    }
  });

  it("says 'Not whitelisted' for a whitelist collection with no personal allowance", () => {
    expect(
      describeMintUnavailability(status({ policy: "whitelist", eligible: false, my_allowance: null })),
    ).toBe("Not whitelisted");
  });

  it("says 'Allowance exhausted' for a whitelisted agent who reached their cap", () => {
    expect(
      describeMintUnavailability(
        status({ policy: "whitelist", eligible: false, my_allowance: 5, minted_by_me: 5 }),
      ),
    ).toBe("Allowance exhausted");
  });

  it("says 'Creator-only minting' for a non-creator under CREATOR_ONLY", () => {
    expect(describeMintUnavailability(status({ policy: "creator_only", eligible: false }))).toBe(
      "Creator-only minting",
    );
  });

  it("says 'Mint unavailable' as the PUBLIC fallback (should not normally happen — PUBLIC is always eligible)", () => {
    expect(describeMintUnavailability(status({ policy: "public", eligible: false }))).toBe(
      "Mint unavailable",
    );
  });
});
