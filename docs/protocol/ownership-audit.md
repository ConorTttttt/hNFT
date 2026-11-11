# Gate 0.2 — Existing hNFT Ownership/Transfer Audit

**Date:** 2026-11-11  
**Status:** source reviewed; changes not implemented; no runtime tests executed  
**Baseline commit:** c9fb294e88b4dd958a3a7f4bd588ca9a17d52673 (main)  
**Companion artifacts:** docs/adr/0001-ownership-transfer-finality.md; docs/protocol/ownership-validation-rules.md

## What exists today

| Area / current path | Existing behavior | Architectural treatment |
| --- | --- | --- |
| integrity/nft/src/transfer.rs | TransferCommitment binds seller to prev_state, buyer key, sequence | Retain immutable references; rename/version Offer and change prior state to minted/finalized |
| integrity/nft/src/transfer.rs | TransferReceipt buyer-authored; becomes ownership state immediately | Change to provisional Acceptance only |
| integrity/nft/src/transfer.rs | TransferCancellation by offerer after transfer window | Split pre-deadline Cancellation and post-settlement Expiry; terminal choice serialized on seller chain |
| coordinator/nft/src/transfer.rs | initiate_transfer, accept_transfer, cancel_transfer, drop_stale_ownership_link | Add finalize_transfer and expire_transfer; cleanup never equals settlement |
| coordinator/nft/src/query.rs | current_owner uses highest-sequence NftToReceipt link, provenance is sorted link collection | Follow Finalization immutable prev_owner_state lineage, return visibility/conflict status |
| integrity/nft/src/links.rs | NftToReceipt and AgentToNft links checked against receipt; links drive UI | Create finalization provenance index; links are discovery hints, not settlement evidence |
| integrity/nft/src/dna_properties.rs | transfer_window_secs included in immutable DNA properties | Add settlement_grace_secs and explicit wire version |
| client/src/types.ts / client.ts | TransferReceipt types and accept => ownership | Extend types/API for Accept, Finalize, Cancel, Expire and status |
| ui/src/lib/appState.svelte.ts | On accepting an offer, toast says 'Offer accepted — NFT is yours' | Change to 'Acceptance recorded — awaiting finalization'; show Conflict/PendingNetwork |
| coordinator/nft/tests/flows.rs / edge.rs | 13 Sweettests reported passing on 0.7.0 by README | Preserve baseline; add adversarial tests per validation matrix |

## Critical vulnerability: acceptance plus later cancellation

Illustrative sequence that the *current validators* admit at the record level:

1. Alice mints NFT N, owns state N.
2. Alice creates commitment O to Bob for N, valid until timestamp T.
3. Bob creates receipt R for O at timestamp <= T. It is independently valid, and get_provenance now treats Bob as owner if receipt index link is visible.
4. Alice creates cancellation C referencing O at timestamp > T. It is independently valid on Alice's chain because validation checks only author and timestamp, not R or a seller-exclusive terminal state.
5. seller_has_open_duplicate treats O as closed based on C and will permit a new commitment consuming N if evaluated on Alice's chain. Receipt R can remain valid and indexed.

**Result:** the old protocol cannot establish exclusive transfer vs cancellation. The specific coordinator's current_owner preflight may reject Alice on a fully converged network, but a coordinator preflight is neither deterministic DHT validation nor guaranteed to see Bob's receipt promptly. Holochain's eventual publication makes this a security boundary defect, not merely a UI bug.

## Additional findings

- **Incomplete reads:** resolve_hops sorts links and checks sequence counts, but does not check every commitment.prev_state against the preceding receipt in its lineage. Missing links can yield an apparently valid shorter chain; sorted links do not constitute full ownership proof.
- **Stale indices:** a seller's AgentToNft link can remain after acceptance. This is correctly described as derived, but get_nfts_for_agent returns linked records without authoritative ownership filtering. Current tests explicitly show the stale link remains until removed.
- **Timestamp trust:** transfer.rs compares action timestamp to offer deadline, which is reproducible but not a trusted real-world timer. Existing README states that accept-vs-cancel is deterministic; this assertion does not establish exclusivity because valid actions are on separate chains.
- **Partial test coverage:** flows.rs and edge.rs check common paths and some expiry limits, but do not assert that acceptance and cancellation cannot both be valid or that a seller cannot re-transfer after a cancellation concurrent with a receipt.
- **Compatibility:** Cargo/README target HDK 0.7.0 and HDI 0.8.0. 0.7.1 API, partial history handling, host-function behavior and runtime build remain Gate 0.1 verification items. No builds or tests were executed during this source review.

## Classification

| Finding | Severity | Impact | Required mitigation |
| --- | --- | --- | --- |
| Acceptance and cancellation valid simultaneously | Critical | conflicting title/reoffer | seller-authored Finalization as exclusive title transfer |
| Ownership change with no explicit finalization | Critical | no common terminal order | add Finalization and seller-chain double-spend checks |
| Provenance lookup depends on partial mutable DHT link collection | High | stale/incorrect owner results | lineage proof plus explicit unknown/conflict status |
| Stale AgentToNft index displayed as holdings | Medium | misleading wallet | verify owner state before showing transferable ownership |
| Self-reported expiry clocks | High | timing fairness not cryptographically enforceable | explicitly limit v0.1 promises, optional future notary/clock design |
| No adversarial accept/cancel or partition tests | Critical | defect undetected | add OWN-007 to OWN-022 scenarios |
| Holochain 0.7.1 yet unverified | Blocker for approval | API/runtime uncertainty | close Gate 0.1 |

## Source review limits and next step

This is a **static audit** of the default branch at the stated commit, not a full adversarial code audit, runtime proof, or approved architecture. Review proposed ADR and trade-offs, verify Holochain 0.7.1, and approve protocol *before* modifying transfer zomes.

Holochain context:
- https://developer.holochain.org/build/validation/
- https://developer.holochain.org/build/must-get-host-functions/
- https://developer.holochain.org/build/getting-an-agents-status/
