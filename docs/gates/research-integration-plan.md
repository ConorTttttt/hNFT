# hNFT research integration plan

Date: 11 November 2026. Status: proposed backlog; no gate is approved by this document.

## Release direction

Preserve the curated-first-collection goal and the path to an open platform. Make certified ownership, collection issuance integrity and honest uncertainty in the UI the core of v0.1. Defer marketplace payments, threshold-signature optimization, zero-knowledge transfers, dynamic committees and a general-purpose DAG consensus network.

This plan builds on the existing Gate 0.1 toolchain work and Gate 0.2 architecture work. It does not renumber S0-03 CI, S0-04 environment configuration or S0-05 release process. Proposed ticket suffixes below are planning identifiers, not GitHub issues already created.

## Foundation gates

| Work | Concrete change | Acceptance evidence | Depends on |
| --- | --- | --- | --- |
| S0-01 / Gate 0.1 | Keep Holochain 0.7.1 capability/build verification in [PR #1](https://github.com/ConorTttttt/hNFT/pull/1). Include signature-verification host APIs and deterministic dependency behavior needed by certificates. | Locked clean build, runtime/API evidence and actual Sweettest logs. Historical 0.7.0 results do not close the gate. | Existing toolchain tasks |
| S0-02 / Gate 0.2-A | Compare [PR #2](https://github.com/ConorTttttt/hNFT/pull/2) seller-chain finalization with ADR-0002's stronger witness authority. Preserve the existing audit. | One recorded authority decision, threat model and consistent ADR/diagram/validation matrix; no mixed finalization rules. | Source audit; final approval accounts for Gate 0.1 |
| S0-02 / Gate 0.2-B | Specify and model exclusive issuance, transfer, certified cancellation/expiry and recovery. | Reviewed safety argument, bounded counterexample search, declared fault/timing/fairness assumptions and explicit liveness limits. | 0.2-A |
| S0-02 / Gate 0.2-C | Freeze state/decision identity and canonical signing formats. | Rust/TypeScript golden bytes, digest/signature vectors, negative vectors for wrong DNA, key, policy, recipient and duplicate signers. | 0.2-A; final schema follows 0.2-B |
| S0-03 / CI | Run the abstract explorer now; later add canonical vectors, model checking and adversarial Sweettests. | Unsafe variants must produce counterexamples; safe variants must satisfy their declared bounds. Archive results with source SHA. | Existing CI setup; integration tests after toolchain passes |
| S0-04 / Environments | Separate DNA identities, manifests, committee keys and signing domains for development/staging/production. | Cross-network replay rejection; no shared production signer keys in tests; deployment configuration audit. | 0.2-C |
| S0-05 / Release process | Add explicit finality, restore and evidence requirements. | No “approved”, “current owner”, “double-spend prevented” or “atomic sale” claim without matching evidence. | All relevant release gates |

## Implementation sequence

| Proposed ticket | Deliverable | Done when |
| --- | --- | --- |
| OWN-01 | Pure verifier and immutable state/certificate schema | Exact lineage, token, version, authorization, domains and distinct quorum signatures validated; alternate certificates for one decision give one state ID. |
| OWN-02 | Fixed-committee durable witness prototype | Concurrent requests cannot produce two certificates under f<=1; kill/restart at every persist/sign boundary preserves authorization; duplicate requests are idempotent. |
| OWN-03 | Agreement recovery, cancellation and expiry | Chosen recovery protocol handles split votes under stated liveness assumptions; delayed certificates cannot race certified closure; time policy independently reviewed. No timeout unlock. |
| OWN-04 | Holochain integration | Integrity tests reject handcrafted invalid records independently of coordinator guards; missing dependencies defer; partitions, duplicate publications and forked sellers cannot bypass quorum rules. |
| OWN-05 | Proof-based queries, client and UI | Accept does not display ownership; wallet links do not authorize spends; historical proof is separate from freshness; verified conflicting certificates halt the affected asset. |
| COL-01 | Certified collection issuance | Same edition slot cannot obtain two accepted genesis allocations under assumptions; unapproved roots/minters rejected; sealed metadata remains hidden until valid reveal. |
| PROV-01 | Portable provenance/media evidence | A separate verifier validates genesis and every hop from an export; missing/modified content fails clearly; restore from independent media copies succeeds. |
| OPS-01 | Witness operations and security review | Four independent operators/failure domains or an explicitly revised trust claim; durable restore and duplicate-signer fencing demonstrated; compromise response does not silently reset locks or rotate membership. |

Dependency order: 0.2-A/B/C -> OWN-01 and COL-01; Gate 0.1 + OWN-01 -> OWN-02 -> OWN-03 -> OWN-04 -> OWN-05. PROV-01 follows the state schema. OPS-01 starts during witness design and must finish before public transfers. Independent metadata/gallery work can proceed alongside this sequence.

## Required adversarial evidence

| Case | Expected result |
| --- | --- |
| Owner forks its source chain and authorizes Bob and Carol | Only one certificate can be valid under fixed membership and f<=1. |
| 2/2 witness split, all honest and online | Safety-only prototype stalls; production recovery must have an explicit tested resolution under its assumptions. |
| Sign-before-persist, stale restore, copied key, timeout lock deletion | Negative tests demonstrate the hazard; production path prevents these behaviors or counts the signer faulty. |
| Acceptance races cancellation/expiry | One certified terminal decision consumes the input; acceptance alone never moves title. |
| Old certificate arrives after closure/reoffer attempt | No reuse of old input; no local timeout invalidates a legitimate certificate. |
| Same decision published with different signer subsets/action hashes | Same logical state, no duplicated ownership branch. |
| Recipient/policy/network altered; duplicate or ineligible signers | Certificate rejected before ownership is granted. |
| Duplicate edition using two mint actions or a new manifest revision | Shared slot conflict domain blocks second issuance. |
| Missing links, missing ancestors, offline cached certificate | Historical/pending status; never infer absence of a newer spend. |
| Two valid conflicting certificates or >f compromised witnesses | Fault assumption violation surfaced; affected asset stopped and evidence retained; no timestamp/hash winner. |

## Public release decision

Public transfers require all of: approved authority model; Gate 0.1 passed; independently reviewed safety and conditional liveness; actual persistence/recovery tests; collection issuance checks; Holochain adversarial tests; honest UI semantics; and operational/security sign-off. The one-input Python result alone closes none of these gates.

If those are incomplete, a gallery/provenance preview with transfer controls disabled is a narrower possible milestone. Real certified issuance still requires COL-01. Do not turn an unresolved ownership protocol into a public NFT transfer feature to preserve a schedule.

## Relationship to research R1–R9

| Report gate | Project home |
| --- | --- |
| R1 Ownership model | Gate 0.2-A; ADR-0002 |
| R2 Formal model | Gate 0.2-B; explorer is initial bounded evidence only |
| R3 Cryptographic format | Gate 0.2-C; OWN-01 |
| R4 Witness prototype | OWN-02 |
| R5 Recovery and liveness | OWN-03; OPS-01 |
| R6 Provenance graph | PROV-01; OWN-05 |
| R7 Collection integrity | COL-01 |
| R8 Integration | OWN-04; optional later Unyt evaluation |
| R9 Security review | OPS-01; public release decision |
