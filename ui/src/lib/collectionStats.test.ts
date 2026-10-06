import { describe, expect, it } from "vitest";
import { formatListingPrice, supplyStats } from "./collectionStats";

describe("supplyStats", () => {
  it("treats max_supply 0 as unlimited, with no remaining count", () => {
    expect(supplyStats(0, 5)).toEqual({ max: null, unlimited: true, minted: 5, remaining: null });
  });

  it("computes remaining for a finite supply", () => {
    expect(supplyStats(100, 42)).toEqual({ max: 100, unlimited: false, minted: 42, remaining: 58 });
  });

  it("never reports a negative remaining count", () => {
    expect(supplyStats(10, 12).remaining).toBe(0);
  });

  it("reports sold-out as zero remaining", () => {
    expect(supplyStats(10, 10).remaining).toBe(0);
  });

  it("leaves remaining unknown until the minted count is loaded", () => {
    expect(supplyStats(10, null)).toEqual({ max: 10, unlimited: false, minted: null, remaining: null });
  });

  it("leaves everything unknown when collection info isn't loaded", () => {
    expect(supplyStats(undefined, null)).toEqual({ max: null, unlimited: false, minted: null, remaining: null });
    expect(supplyStats(null, 3).remaining).toBeNull();
  });
});

describe("formatListingPrice", () => {
  it("shows price and currency as listed", () => {
    expect(formatListingPrice({ price: 12, currency: "HOT" })).toBe("12 HOT");
  });

  it("omits an empty currency", () => {
    expect(formatListingPrice({ price: 3, currency: "" })).toBe("3");
  });
});
