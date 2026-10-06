# G5b freeze record

Documentation only. No protocol, coordinator, client, UI, schema, validation, algorithm, test, or build-configuration file was changed to produce this record.

## 1. Behavioral/protocol continuity

- No G3a, G4, or G5a protocol behavior was changed during G5b.
- No integrity schema, validation rule, generation algorithm, seed derivation, provenance rule, or frozen vector was changed.

## 2. Integrity artifact identity

The current DnaHash differs from the DnaHash recorded in the original G5a implementation report:

- G5a implementation report: `uhC0kEukRv8agXsxbkw5arpc1JRxxFxXVthttlDWDwckW8LHKdHkd`
- Current (G5b freeze): `uhC0kJ-OQBEHNrltjbTKCyVmZH2szfhdwPXGXIX3Jwj49g1STQxpu`

The difference arose during the G5a audit, when a documentation comment was added to integrity code (`generation_record.rs`, explaining that `GenerationRecord` validation trusts a cited `GenerationRandomness` transitively via `must_get_valid_record` rather than re-invoking the verifier). That comment altered compiled WASM bytes because `file!()`/`line!()` information appears in compiled panic-message strings, and adding lines shifted every subsequent line number in the file. The integrity artifact bytes therefore changed despite no behavioral or protocol change — confirmed by `cargo test -p nft_integrity` passing with the same 58/58 result both before and after that comment.

## 3. G5b coordinator continuity

The G5b coordinator change (`get_generation_randomness` returning `Option<(ActionHash, GenerationRandomness)>` instead of `Option<GenerationRandomness>`) does **not** change DnaHash.

This was verified empirically, not just asserted: the coordinator function was reverted to its old return shape, the nft DNA was rebuilt, and `hc dna hash` was run again. The result was byte-for-byte identical to the DnaHash with the change present. This matches Holochain 0.7's own source (`holochain_zome_types::dna_def::DnaDef`), whose hash-relevant content is explicitly `{modifiers, integrity_zomes}` — coordinator zomes are documented there as not affecting the DnaHash.

**Wording for the record:** the G5b coordinator change does not alter DnaHash. The current DnaHash is the same as the immediately preceding integrity artifact used for G5b verification. It differs from the DnaHash recorded in the original G5a implementation report because of a subsequent G5a audit-only comment change that altered integrity WASM bytes without changing protocol behavior.

## 4. Package identity

The built `.dna` and `.happ` file SHA-256 fingerprints may differ from earlier G5a builds because coordinator WASM bytes are packaged into both files even though coordinator code does not contribute to DnaHash. The tooling exposes no separate "hApp hash" command (`hc app` has no `hash` subcommand), so these fingerprints are the best available package-artifact identity:

- `workdir/p2p_art_marketplace.happ`: `cedf41e1bcf75eb768ba8491e9dda32009de68bc29aa7de1d74f46dd115a111e`
- `dnas/nft/workdir/nft.dna`: `35f4f39ee67eed82130e0328478842b79e0988caaa5bb82a1d341080fc7b9716`
- `dnas/social/workdir/social.dna` (untouched by G5a/G5b): `3bc2ed6b215673ba36b33c6d3e7e8ce97fea07736d4a93b695317bd073b7f990`

## Conclusion

No protocol or G5b integration blockers identified.
