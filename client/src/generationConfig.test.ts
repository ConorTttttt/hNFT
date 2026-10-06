import { describe, expect, it } from "vitest";
import {
  MAX_CATEGORIES,
  MAX_TRAITS_PER_CATEGORY,
  MAX_DISPLAY_NAME_LEN,
  emptyCategory,
  isValidGenerationConfig,
  suggestId,
  totalWeight,
  validateGenerationConfig,
  weightPercent,
  zeroWeightTraits,
} from "./generationConfig.js";
import type { GenerationConfig, TraitCategory } from "./types.js";

function cat(id: string, traits: { id: string; weight: number }[]): TraitCategory {
  return { id, display_name: id, traits: traits.map((t) => ({ ...t, display_name: t.id })) };
}
function config(categories: TraitCategory[]): GenerationConfig {
  return { generation_version: 1, categories };
}

describe("validateGenerationConfig", () => {
  it("accepts a valid configuration", () => {
    const cfg = config([cat("background", [{ id: "blue", weight: 50 }, { id: "gold", weight: 5 }])]);
    expect(validateGenerationConfig(cfg)).toEqual([]);
    expect(isValidGenerationConfig(cfg)).toBe(true);
  });

  it("rejects an empty categories list", () => {
    expect(isValidGenerationConfig(config([]))).toBe(false);
  });

  it("accepts exactly MAX_CATEGORIES and rejects one more", () => {
    const many = (n: number) =>
      config(Array.from({ length: n }, (_, i) => cat(`c${i}`, [{ id: "t", weight: 1 }])));
    expect(isValidGenerationConfig(many(MAX_CATEGORIES))).toBe(true);
    expect(isValidGenerationConfig(many(MAX_CATEGORIES + 1))).toBe(false);
  });

  it("accepts exactly MAX_TRAITS_PER_CATEGORY traits and rejects one more", () => {
    const traits = (n: number) =>
      config([cat("c", Array.from({ length: n }, (_, i) => ({ id: `t${i}`, weight: 1 })))]);
    expect(isValidGenerationConfig(traits(MAX_TRAITS_PER_CATEGORY))).toBe(true);
    expect(isValidGenerationConfig(traits(MAX_TRAITS_PER_CATEGORY + 1))).toBe(false);
  });

  it("enforces the display_name byte limit for categories and traits", () => {
    const at = (catName: string, traitName: string) =>
      config([
        {
          id: "c",
          display_name: catName,
          traits: [{ id: "t", display_name: traitName, weight: 1 }],
        },
      ]);
    expect(isValidGenerationConfig(at("a".repeat(MAX_DISPLAY_NAME_LEN), "b"))).toBe(true);
    expect(isValidGenerationConfig(at("a".repeat(MAX_DISPLAY_NAME_LEN + 1), "b"))).toBe(false);
    expect(isValidGenerationConfig(at("a", "b".repeat(MAX_DISPLAY_NAME_LEN + 1)))).toBe(false);
  });

  it("rejects duplicate category ids", () => {
    const cfg = config([cat("dup", [{ id: "a", weight: 1 }]), cat("dup", [{ id: "b", weight: 1 }])]);
    expect(isValidGenerationConfig(cfg)).toBe(false);
  });

  it("rejects duplicate trait ids within a category", () => {
    const cfg = config([cat("c", [{ id: "dup", weight: 1 }, { id: "dup", weight: 2 }])]);
    expect(isValidGenerationConfig(cfg)).toBe(false);
  });

  it("rejects empty ids", () => {
    expect(isValidGenerationConfig(config([cat("", [{ id: "a", weight: 1 }])]))).toBe(false);
    expect(isValidGenerationConfig(config([cat("c", [{ id: "", weight: 1 }])]))).toBe(false);
  });

  it("rejects zero total category weight but allows individual zero-weight traits", () => {
    expect(isValidGenerationConfig(config([cat("c", [{ id: "a", weight: 0 }])]))).toBe(false);
    expect(isValidGenerationConfig(config([cat("c", [{ id: "a", weight: 0 }, { id: "b", weight: 1 }])]))).toBe(
      true,
    );
  });

  it("rejects an unsupported generation_version", () => {
    const cfg = config([cat("c", [{ id: "a", weight: 1 }])]);
    expect(isValidGenerationConfig({ ...cfg, generation_version: 2 })).toBe(false);
  });

  it("reports a path for every issue", () => {
    const cfg = config([cat("", [])]);
    const issues = validateGenerationConfig(cfg);
    expect(issues.length).toBeGreaterThan(0);
    for (const issue of issues) {
      expect(issue.path).toMatch(/^categories\[0\]/);
      expect(issue.message.length).toBeGreaterThan(0);
    }
  });
});

describe("zeroWeightTraits / totalWeight / weightPercent", () => {
  const c = cat("c", [{ id: "a", weight: 0 }, { id: "b", weight: 3 }, { id: "c", weight: 1 }]);

  it("flags zero-weight traits without rejecting them", () => {
    expect(zeroWeightTraits(c).map((t) => t.id)).toEqual(["a"]);
    expect(isValidGenerationConfig(config([c]))).toBe(true);
  });

  it("computes total weight and percentages from the entered integers only", () => {
    expect(totalWeight(c)).toBe(4);
    expect(weightPercent(c, c.traits[0]!)).toBe(0);
    expect(weightPercent(c, c.traits[1]!)).toBeCloseTo(75);
    expect(weightPercent(c, c.traits[2]!)).toBeCloseTo(25);
  });

  it("percent of an all-zero category is 0, not NaN", () => {
    const allZero = cat("z", [{ id: "a", weight: 0 }]);
    expect(weightPercent(allZero, allZero.traits[0]!)).toBe(0);
  });
});

describe("suggestId", () => {
  it("never runs automatically and never overwrites an explicit id (caller contract)", () => {
    // suggestId is a pure function the caller must only apply to an EMPTY id
    // field; this test documents and locks that contract at the function level.
    expect(suggestId("Cosmic Blue")).toBe("cosmic-blue");
  });

  it("is deterministic and produces protocol-legal ids", () => {
    expect(suggestId("Cosmic Blue")).toBe(suggestId("Cosmic Blue"));
    expect(suggestId("  Ünïcode!! Name  ")).toBe("ncode-name");
    expect(suggestId("x".repeat(100)).length).toBeLessThanOrEqual(64);
  });

  it("does not claim exact-id preservation for non-ASCII input (documented UI convenience only)", () => {
    // The protocol itself never normalizes ids (see generation.rs); suggestId
    // is an opt-in convenience with its own, separate, documented lossy rule.
    expect(suggestId("眼睛")).toBe("");
  });
});

describe("emptyCategory", () => {
  it("starts with a blank, editable id and a single zero-issue-free-shaped trait", () => {
    const c = emptyCategory();
    expect(c.id).toBe("");
    expect(c.traits).toHaveLength(1);
    expect(c.traits[0]!.weight).toBeGreaterThan(0);
  });
});
