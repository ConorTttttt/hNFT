//! G1: deterministic generative-trait foundation.
//!
//! Pure functions and plain data only — no HDK calls, no clock, no OS/browser
//! randomness, no network. `generate(config, seed)` is a function of its two
//! arguments and nothing else, so the same config + version + seed always
//! yields the same traits, on any peer, in any language that reimplements it.
//!
//! This module is deliberately ignorant of where the seed came from (QRNG,
//! oracle, ...) and of how traits become artwork (external storage, or a
//! future on-chain renderer). It provides determinism, NOT unpredictability:
//! the security of a generative mint depends entirely on the mechanism that
//! produces the [`GenerationSeed`].
//!
//! # Generation v1 rules (frozen once shipped — changes need a new version)
//!
//! For each category, in config order:
//! 1. `roll = u64_le(blake2b_256(DOMAIN_V1 ‖ seed ‖ len(category.id) as u32 le ‖ category.id)[0..8])`
//! 2. `point = roll % total_weight` (`total_weight` = sum of that category's weights)
//! 3. pick the first trait whose cumulative weight exceeds `point`
//!    (trait `i` owns `[sum(w[..i]), sum(w[..=i]))`).
//!
//! The roll depends on the category *id*, not its position, so reordering
//! categories does not change what each category picks. Modulo bias is at most
//! `total_weight / 2^64` — negligible for u32 weights.

use crate::nft_manifest::{commit_hash, NftAttribute};
use hdi::prelude::*;

/// The only generation version this build understands.
pub const GENERATION_VERSION_1: u32 = 1;

const DOMAIN_V1: &[u8] = b"hnft.generation.v1";
const MAX_ID_LEN: usize = 64;
/// G5a integrity limits on a generation config.
pub const MAX_CATEGORIES: usize = 32;
pub const MAX_TRAITS_PER_CATEGORY: usize = 256;
pub const MAX_DISPLAY_NAME_LEN: usize = 64;

/// Opaque 32-byte seed. Produced by a future randomness layer; the generator
/// only requires that it be supplied, never how it was made.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct GenerationSeed(pub [u8; 32]);

/// One selectable trait. `id` is the stable logical identity; `display_name`
/// may change without changing what a generated NFT *is*.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TraitOption {
    pub id: String,
    pub display_name: String,
    /// Relative weight (not a percentage). `0` = never selected.
    pub weight: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TraitCategory {
    pub id: String,
    pub display_name: String,
    pub traits: Vec<TraitOption>,
}

/// A collection's trait catalogue plus the rules version that interprets it.
///
/// Future compatibility rules (e.g. "laser_eyes excludes sunglasses") can be
/// added as new `#[serde(default)]` fields without breaking this shape.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct GenerationConfig {
    pub generation_version: u32,
    pub categories: Vec<TraitCategory>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct GeneratedTrait {
    pub category_id: String,
    pub trait_id: String,
}

/// Generator output. Always carries the version that produced it so old NFTs
/// stay reproducible. Contains no artwork/media reference by design.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct GeneratedTraits {
    pub generation_version: u32,
    pub traits: Vec<GeneratedTrait>,
}

impl GeneratedTraits {
    /// Represent the result with the existing manifest attribute type
    /// (`trait_type` = category id, `value` = trait id), so it slots into the
    /// current reveal payload without changing `NftManifest`.
    pub fn to_attributes(&self) -> Vec<NftAttribute> {
        self.traits
            .iter()
            .map(|t| NftAttribute {
                trait_type: t.category_id.clone(),
                value: t.trait_id.clone(),
            })
            .collect()
    }
}

fn validate_id(kind: &str, id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err(format!("{kind} id must not be empty"));
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!("{kind} id '{id}' exceeds {MAX_ID_LEN} bytes"));
    }
    Ok(())
}

fn validate_display_name(kind: &str, name: &str) -> Result<(), String> {
    if name.len() > MAX_DISPLAY_NAME_LEN {
        return Err(format!(
            "{kind} display_name exceeds {MAX_DISPLAY_NAME_LEN} bytes"
        ));
    }
    Ok(())
}

impl GenerationConfig {
    /// Structural validation. `generate` calls this, so an invalid config can
    /// never silently produce output.
    pub fn validate(&self) -> Result<(), String> {
        if self.generation_version != GENERATION_VERSION_1 {
            return Err(format!(
                "unsupported generation_version {}",
                self.generation_version
            ));
        }
        if self.categories.is_empty() {
            return Err("generation config has no categories".into());
        }
        if self.categories.len() > MAX_CATEGORIES {
            return Err(format!("more than {MAX_CATEGORIES} categories"));
        }
        let mut seen_categories = std::collections::BTreeSet::new();
        for cat in &self.categories {
            validate_id("category", &cat.id)?;
            validate_display_name("category", &cat.display_name)?;
            if !seen_categories.insert(cat.id.as_str()) {
                return Err(format!("duplicate category id '{}'", cat.id));
            }
            if cat.traits.is_empty() {
                return Err(format!("category '{}' has no traits", cat.id));
            }
            if cat.traits.len() > MAX_TRAITS_PER_CATEGORY {
                return Err(format!(
                    "category '{}' has more than {MAX_TRAITS_PER_CATEGORY} traits",
                    cat.id
                ));
            }
            let mut seen_traits = std::collections::BTreeSet::new();
            let mut total: u64 = 0;
            for t in &cat.traits {
                validate_id("trait", &t.id)?;
                validate_display_name("trait", &t.display_name)?;
                if !seen_traits.insert(t.id.as_str()) {
                    return Err(format!("duplicate trait id '{}' in '{}'", t.id, cat.id));
                }
                total += t.weight as u64;
            }
            if total == 0 {
                return Err(format!("category '{}' has zero total weight", cat.id));
            }
        }
        Ok(())
    }
}

/// The seed-derived roll for one category (rule 1). Exposed so tests and
/// future validators can inspect it.
pub fn category_roll(seed: &GenerationSeed, category_id: &str) -> u64 {
    let mut buf = Vec::with_capacity(DOMAIN_V1.len() + 32 + 4 + category_id.len());
    buf.extend_from_slice(DOMAIN_V1);
    buf.extend_from_slice(&seed.0);
    buf.extend_from_slice(&(category_id.len() as u32).to_le_bytes());
    buf.extend_from_slice(category_id.as_bytes());
    let h = commit_hash(&buf);
    u64::from_le_bytes(h[..8].try_into().expect("8 bytes"))
}

/// Rules 2–3: map a roll into the weighted ranges. Returns `None` only if the
/// total weight is zero (callers validate first).
pub fn select_index(weights: &[u32], roll: u64) -> Option<usize> {
    let total: u64 = weights.iter().map(|w| *w as u64).sum();
    if total == 0 {
        return None;
    }
    let point = roll % total;
    let mut acc: u64 = 0;
    for (i, w) in weights.iter().enumerate() {
        acc += *w as u64;
        if point < acc {
            return Some(i);
        }
    }
    None
}

/// Deterministically generate one trait per category.
pub fn generate(
    config: &GenerationConfig,
    seed: &GenerationSeed,
) -> Result<GeneratedTraits, String> {
    generate_with_rolls(config, |cat| category_roll(seed, cat))
}

/// Same as [`generate`] but with the per-category roll supplied by the caller,
/// so weighted boundaries can be tested precisely.
pub fn generate_with_rolls(
    config: &GenerationConfig,
    roll_for: impl Fn(&str) -> u64,
) -> Result<GeneratedTraits, String> {
    config.validate()?;
    let mut traits = Vec::with_capacity(config.categories.len());
    for cat in &config.categories {
        let weights: Vec<u32> = cat.traits.iter().map(|t| t.weight).collect();
        let idx = select_index(&weights, roll_for(&cat.id))
            .ok_or_else(|| format!("category '{}' has zero total weight", cat.id))?;
        traits.push(GeneratedTrait {
            category_id: cat.id.clone(),
            trait_id: cat.traits[idx].id.clone(),
        });
    }
    Ok(GeneratedTraits {
        generation_version: config.generation_version,
        traits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str, weight: u32) -> TraitOption {
        TraitOption {
            id: id.into(),
            display_name: id.into(),
            weight,
        }
    }
    fn cat(id: &str, traits: Vec<TraitOption>) -> TraitCategory {
        TraitCategory {
            id: id.into(),
            display_name: id.into(),
            traits,
        }
    }
    fn config() -> GenerationConfig {
        GenerationConfig {
            generation_version: 1,
            categories: vec![
                cat(
                    "background",
                    vec![
                        opt("blue", 50),
                        opt("red", 30),
                        opt("cosmic", 15),
                        opt("gold", 5),
                    ],
                ),
                cat(
                    "eyes",
                    vec![opt("normal", 70), opt("laser", 20), opt("diamond", 10)],
                ),
            ],
        }
    }
    fn seed(b: u8) -> GenerationSeed {
        GenerationSeed([b; 32])
    }

    #[test]
    fn deterministic_across_repeated_runs() {
        let first = generate(&config(), &seed(7)).unwrap();
        for _ in 0..100 {
            assert_eq!(generate(&config(), &seed(7)).unwrap(), first);
        }
    }

    #[test]
    fn different_seeds_can_differ() {
        let outs: std::collections::BTreeSet<String> = (0..=255u8)
            .map(|b| format!("{:?}", generate(&config(), &seed(b)).unwrap()))
            .collect();
        assert!(outs.len() > 1);
    }

    #[test]
    fn every_category_gets_exactly_one_trait_in_order() {
        for b in 0..=255u8 {
            let g = generate(&config(), &seed(b)).unwrap();
            let cats: Vec<&str> = g.traits.iter().map(|t| t.category_id.as_str()).collect();
            assert_eq!(cats, vec!["background", "eyes"]);
            let attrs = g.to_attributes();
            assert_eq!(attrs.len(), 2);
            assert_eq!(attrs[0].trait_type, "background");
        }
    }

    #[test]
    fn weighted_boundaries() {
        // background ranges: blue [0,50) red [50,80) cosmic [80,95) gold [95,100)
        let w = [50, 30, 15, 5];
        for (roll, want) in [
            (0, 0),
            (49, 0),
            (50, 1),
            (79, 1),
            (80, 2),
            (94, 2),
            (95, 3),
            (99, 3),
            (100, 0), // wraps modulo total
            (149, 0),
            (150, 1),
        ] {
            assert_eq!(select_index(&w, roll), Some(want), "roll {roll}");
        }
    }

    #[test]
    fn zero_weight_trait_is_never_selected() {
        let w = [0, 10, 0];
        for roll in 0..100 {
            assert_eq!(select_index(&w, roll), Some(1));
        }
    }

    #[test]
    fn injected_rolls_pick_expected_traits() {
        let g =
            generate_with_rolls(&config(), |c| if c == "background" { 95 } else { 70 }).unwrap();
        assert_eq!(g.traits[0].trait_id, "gold");
        assert_eq!(g.traits[1].trait_id, "laser");
    }

    #[test]
    fn category_order_does_not_change_picks() {
        let a = config();
        let mut b = config();
        b.categories.reverse();
        let ga = generate(&a, &seed(3)).unwrap();
        let gb = generate(&b, &seed(3)).unwrap();
        for t in &ga.traits {
            assert!(gb.traits.contains(t));
        }
    }

    #[test]
    fn version_is_retained() {
        assert_eq!(generate(&config(), &seed(1)).unwrap().generation_version, 1);
    }

    #[test]
    fn invalid_configs_are_rejected() {
        let mut c = config();
        c.generation_version = 0;
        assert!(generate(&c, &seed(1)).is_err());
        c.generation_version = 2;
        assert!(generate(&c, &seed(1)).is_err());

        let empty = GenerationConfig {
            generation_version: 1,
            categories: vec![],
        };
        assert!(empty.validate().is_err());

        let no_traits = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("a", vec![])],
        };
        assert!(no_traits.validate().is_err());

        let zero = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("a", vec![opt("x", 0)])],
        };
        assert!(zero.validate().is_err());

        let dup_cat = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("a", vec![opt("x", 1)]), cat("a", vec![opt("y", 1)])],
        };
        assert!(dup_cat.validate().is_err());

        let dup_trait = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("a", vec![opt("x", 1), opt("x", 2)])],
        };
        assert!(dup_trait.validate().is_err());

        let empty_id = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("", vec![opt("x", 1)])],
        };
        assert!(empty_id.validate().is_err());
        let empty_trait_id = GenerationConfig {
            generation_version: 1,
            categories: vec![cat("a", vec![opt("", 1)])],
        };
        assert!(empty_trait_id.validate().is_err());
    }

    #[test]
    fn malformed_serialized_config_is_rejected() {
        // negative weight is unrepresentable in u32 and must fail to deserialize
        let bad = r#"{"generation_version":1,"categories":[{"id":"a","display_name":"A","traits":[{"id":"x","display_name":"X","weight":-1}]}]}"#;
        assert!(serde_json::from_str::<GenerationConfig>(bad).is_err());
        let missing = r#"{"categories":[]}"#;
        assert!(serde_json::from_str::<GenerationConfig>(missing).is_err());
    }

    // ---- Golden vectors for generation v1 --------------------------------
    // Expected rolls were computed independently with Python's
    // `hashlib.blake2b(digest_size=32)` over
    //   b"hnft.generation.v1" + seed + u32_le(len(id)) + id.utf8
    // and read as the first 8 bytes, little-endian. They pin the exact v1
    // byte-level algorithm: a refactor that changes any of them changes the
    // traits of existing NFTs and must instead be a new generation version.

    fn pick_ids(config: &GenerationConfig, seed: &GenerationSeed) -> Vec<(String, String)> {
        generate(config, seed)
            .unwrap()
            .traits
            .into_iter()
            .map(|t| (t.category_id, t.trait_id))
            .collect()
    }

    fn seq_seed() -> GenerationSeed {
        let mut s = [0u8; 32];
        for (i, b) in s.iter_mut().enumerate() {
            *b = i as u8;
        }
        GenerationSeed(s)
    }

    #[test]
    fn golden_single_category() {
        let cfg = GenerationConfig {
            generation_version: 1,
            categories: vec![cat(
                "eyes",
                vec![opt("normal", 70), opt("laser", 20), opt("diamond", 10)],
            )],
        };
        let s = seed(0);
        assert_eq!(category_roll(&s, "eyes"), 11974938226986828654);
        // 11974938226986828654 % 100 = 54 -> normal [0,70)
        assert_eq!(pick_ids(&cfg, &s), vec![("eyes".into(), "normal".into())]);
    }

    #[test]
    fn golden_multiple_categories() {
        let s = seq_seed();
        assert_eq!(category_roll(&s, "background"), 2255182698294181220);
        assert_eq!(category_roll(&s, "eyes"), 9562791129218714794);
        // background: %100 = 20 -> blue [0,50); eyes: %100 = 94 -> diamond [90,100)
        assert_eq!(
            pick_ids(&config(), &s),
            vec![
                ("background".into(), "blue".into()),
                ("eyes".into(), "diamond".into())
            ]
        );
    }

    #[test]
    fn golden_zero_weight_and_unicode_ids() {
        // Ids are UTF-8 bytes; length prefix is the BYTE length (6 for "眼睛").
        let cfg = GenerationConfig {
            generation_version: 1,
            categories: vec![cat(
                "眼睛",
                vec![opt("無効", 0), opt("レーザー", 5), opt("普通", 3)],
            )],
        };
        let s = seed(0xAB);
        assert_eq!(category_roll(&s, "眼睛"), 13905535208956277398);
        // %8 = 6: zero-weight owns nothing, レーザー [0,5), 普通 [5,8) -> 普通
        assert_eq!(pick_ids(&cfg, &s), vec![("眼睛".into(), "普通".into())]);
    }

    #[test]
    fn golden_weighted_boundary_from_a_real_seed() {
        let s = seed(0x11);
        let roll = category_roll(&s, "background");
        assert_eq!(roll, 18216780212876247470);
        assert_eq!(roll % 1000, 470);
        let two = |a: u32, b: u32| GenerationConfig {
            generation_version: 1,
            categories: vec![cat("background", vec![opt("first", a), opt("second", b)])],
        };
        // point 470 is the first value owned by the second trait when the
        // first trait's weight is exactly 470...
        assert_eq!(pick_ids(&two(470, 530), &s)[0].1, "second");
        // ...and still belongs to the first trait when it is 471.
        assert_eq!(pick_ids(&two(471, 529), &s)[0].1, "first");
    }

    // ---- G5a integrity limits ---------------------------------------------

    fn many_categories(n: usize) -> GenerationConfig {
        GenerationConfig {
            generation_version: 1,
            categories: (0..n)
                .map(|i| cat(&format!("c{i}"), vec![opt("t", 1)]))
                .collect(),
        }
    }

    #[test]
    fn category_count_limit() {
        assert!(many_categories(32).validate().is_ok());
        assert!(many_categories(33).validate().is_err());
    }

    #[test]
    fn trait_count_limit() {
        let traits = |n: usize| -> GenerationConfig {
            GenerationConfig {
                generation_version: 1,
                categories: vec![cat("c", (0..n).map(|i| opt(&format!("t{i}"), 1)).collect())],
            }
        };
        assert!(traits(256).validate().is_ok());
        assert!(traits(257).validate().is_err());
    }

    #[test]
    fn display_name_limit_applies_to_categories_and_traits() {
        let with = |cat_name: String, trait_name: String| GenerationConfig {
            generation_version: 1,
            categories: vec![TraitCategory {
                id: "c".into(),
                display_name: cat_name,
                traits: vec![TraitOption {
                    id: "t".into(),
                    display_name: trait_name,
                    weight: 1,
                }],
            }],
        };
        assert!(with("a".repeat(64), "b".repeat(64)).validate().is_ok());
        assert!(with("a".repeat(65), "b".into()).validate().is_err());
        assert!(with("a".into(), "b".repeat(65)).validate().is_err());
    }

    #[test]
    fn ids_are_exact_bytes_and_huge_totals_do_not_overflow() {
        // Unicode ids are accepted as-is (no normalization or charset rule).
        let c = GenerationConfig {
            generation_version: 1,
            categories: vec![cat(
                "眼睛",
                vec![opt("é", u32::MAX), opt("e\u{301}", u32::MAX)],
            )],
        };
        assert!(
            c.validate().is_ok(),
            "two distinct byte strings, even if they render alike"
        );
        // 256 traits at u32::MAX fit comfortably in u64.
        let big = GenerationConfig {
            generation_version: 1,
            categories: vec![cat(
                "c",
                (0..256).map(|i| opt(&format!("t{i}"), u32::MAX)).collect(),
            )],
        };
        assert!(big.validate().is_ok());
        assert!(generate(&big, &seed(3)).is_ok());
    }
}
