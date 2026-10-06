import type { GenerationConfig, TraitCategory, TraitOption } from "./types.js";

/**
 * Client-side pre-validation for a `GenerationConfig`, mirroring the frozen
 * integrity rules in `dnas/nft/zomes/integrity/nft/src/generation.rs` exactly
 * (limits, not behaviour). This is a UX convenience only — it is NOT
 * authoritative. The real check is `GenerationConfig::validate()` in Rust,
 * enforced by `genesis_self_check` when the collection is created and again at
 * every randomness/record/reveal. A config that passes here can still be
 * rejected on-chain (e.g. by a stricter future integrity rule); a config that
 * fails here is guaranteed to be rejected on-chain today.
 *
 * IDs are never generated, normalized, or otherwise mutated by this module —
 * only checked. See `suggestId` for the one OPT-IN convenience that fills an
 * empty id field; it never overwrites a non-empty one.
 */

export const MAX_CATEGORIES = 32;
export const MAX_TRAITS_PER_CATEGORY = 256;
export const MAX_DISPLAY_NAME_LEN = 64;
export const MAX_ID_LEN = 64;
export const GENERATION_VERSION_1 = 1;

export interface ValidationIssue {
  /** Dot-path to the offending field, e.g. `categories[1].traits[0].weight`. */
  path: string;
  message: string;
}

function idIssues(kind: string, path: string, id: string): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  if (id.length === 0) issues.push({ path, message: `${kind} id must not be empty` });
  else if (id.length > MAX_ID_LEN) {
    issues.push({ path, message: `${kind} id exceeds ${MAX_ID_LEN} bytes` });
  }
  return issues;
}

function displayNameIssues(kind: string, path: string, name: string): ValidationIssue[] {
  if (name.length > MAX_DISPLAY_NAME_LEN) {
    return [{ path, message: `${kind} display_name exceeds ${MAX_DISPLAY_NAME_LEN} bytes` }];
  }
  return [];
}

/** Every problem found, or `[]` if the config would pass the on-chain checks. */
export function validateGenerationConfig(cfg: GenerationConfig): ValidationIssue[] {
  const issues: ValidationIssue[] = [];

  if (cfg.generation_version !== GENERATION_VERSION_1) {
    issues.push({
      path: "generation_version",
      message: `unsupported generation_version ${cfg.generation_version}`,
    });
  }
  if (cfg.categories.length === 0) {
    issues.push({ path: "categories", message: "at least one category is required" });
  }
  if (cfg.categories.length > MAX_CATEGORIES) {
    issues.push({ path: "categories", message: `more than ${MAX_CATEGORIES} categories` });
  }

  const seenCategoryIds = new Set<string>();
  cfg.categories.forEach((cat, ci) => {
    const catPath = `categories[${ci}]`;
    issues.push(...idIssues("category", `${catPath}.id`, cat.id));
    issues.push(...displayNameIssues("category", `${catPath}.display_name`, cat.display_name));
    if (cat.id.length > 0) {
      if (seenCategoryIds.has(cat.id)) {
        issues.push({ path: `${catPath}.id`, message: `duplicate category id '${cat.id}'` });
      }
      seenCategoryIds.add(cat.id);
    }
    if (cat.traits.length === 0) {
      issues.push({ path: `${catPath}.traits`, message: `category '${cat.id}' has no traits` });
    }
    if (cat.traits.length > MAX_TRAITS_PER_CATEGORY) {
      issues.push({
        path: `${catPath}.traits`,
        message: `category '${cat.id}' has more than ${MAX_TRAITS_PER_CATEGORY} traits`,
      });
    }
    const seenTraitIds = new Set<string>();
    let total = 0;
    cat.traits.forEach((t, ti) => {
      const tPath = `${catPath}.traits[${ti}]`;
      issues.push(...idIssues("trait", `${tPath}.id`, t.id));
      issues.push(...displayNameIssues("trait", `${tPath}.display_name`, t.display_name));
      if (t.id.length > 0) {
        if (seenTraitIds.has(t.id)) {
          issues.push({ path: `${tPath}.id`, message: `duplicate trait id '${t.id}' in '${cat.id}'` });
        }
        seenTraitIds.add(t.id);
      }
      if (!Number.isInteger(t.weight) || t.weight < 0) {
        issues.push({ path: `${tPath}.weight`, message: "weight must be a non-negative integer" });
      } else {
        total += t.weight;
      }
    });
    if (total === 0) {
      issues.push({
        path: `${catPath}.traits`,
        message: `category '${cat.id}' has zero total weight`,
      });
    }
  });

  return issues;
}

export function isValidGenerationConfig(cfg: GenerationConfig): boolean {
  return validateGenerationConfig(cfg).length === 0;
}

/** Traits with weight `0` in a category (never selected; a warning, not an error). */
export function zeroWeightTraits(cat: TraitCategory): TraitOption[] {
  return cat.traits.filter((t) => t.weight === 0);
}

/** Sum of a category's trait weights (display only — never fed back into the config). */
export function totalWeight(cat: TraitCategory): number {
  return cat.traits.reduce((sum, t) => sum + (Number.isFinite(t.weight) ? t.weight : 0), 0);
}

/** `trait's share of totalWeight(cat)`, as a percentage for display only. */
export function weightPercent(cat: TraitCategory, trait: TraitOption): number {
  const total = totalWeight(cat);
  return total === 0 ? 0 : (trait.weight / total) * 100;
}

/**
 * An OPT-IN suggestion for an id from a display name — lowercased, spaces to
 * `-`, non-alphanumeric (besides `-`/`_`) stripped. This NEVER runs
 * automatically and NEVER overwrites a non-empty id: callers must only use
 * this to fill an id field the user has left blank, and must still let them
 * edit the result freely afterward. The protocol itself imposes no such rule
 * (ids are exact byte strings) — this is a UI convenience only.
 */
export function suggestId(displayName: string): string {
  return displayName
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9_-]/g, "")
    .slice(0, MAX_ID_LEN);
}

export function emptyTrait(): TraitOption {
  return { id: "", display_name: "", weight: 1 };
}

export function emptyCategory(): TraitCategory {
  return { id: "", display_name: "", traits: [emptyTrait()] };
}
