import { describe, expect, it } from "vitest";
import { canReveal, deriveGenerativeStatus } from "./generativeStatus";

describe("deriveGenerativeStatus", () => {
  it("is revealed once the manifest is revealed, regardless of other flags", () => {
    expect(
      deriveGenerativeStatus({
        manifestStatus: "revealed",
        hasRandomness: false,
        hasRecord: false,
      }),
    ).toBe("revealed");
  });

  it("is unrevealed with no randomness and nothing pending", () => {
    expect(
      deriveGenerativeStatus({ manifestStatus: "sealed", hasRandomness: false, hasRecord: false }),
    ).toBe("unrevealed");
  });

  it("is randomness-pending while a create call is in flight and no record exists yet", () => {
    expect(
      deriveGenerativeStatus({
        manifestStatus: "sealed",
        hasRandomness: false,
        hasRecord: false,
        actionPending: true,
      }),
    ).toBe("randomness-pending");
  });

  it("is randomness-verified once randomness exists but no record yet", () => {
    expect(
      deriveGenerativeStatus({ manifestStatus: "sealed", hasRandomness: true, hasRecord: false }),
    ).toBe("randomness-verified");
  });

  it("is ready-to-reveal once a GenerationRecord exists", () => {
    expect(
      deriveGenerativeStatus({ manifestStatus: "sealed", hasRandomness: true, hasRecord: true }),
    ).toBe("ready-to-reveal");
  });

  it("a record takes priority over a stale pending flag", () => {
    expect(
      deriveGenerativeStatus({
        manifestStatus: "sealed",
        hasRandomness: true,
        hasRecord: true,
        actionPending: true,
      }),
    ).toBe("ready-to-reveal");
  });
});

describe("canReveal", () => {
  it("only ready-to-reveal and traits-generated permit reveal", () => {
    expect(canReveal("ready-to-reveal")).toBe(true);
    expect(canReveal("traits-generated")).toBe(true);
    for (const s of ["unrevealed", "randomness-pending", "randomness-verified", "revealed"] as const) {
      expect(canReveal(s)).toBe(false);
    }
  });
});
