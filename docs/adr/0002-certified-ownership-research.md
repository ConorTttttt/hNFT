# ADR-0002 — Certified ownership for the public hNFT release

Status: PROPOSED; not approved, implemented, or production-verified.  
Date: 11 November 2026. Gate: 0.2 / S0-02.  
Baseline: `ConorTttttt/hNFT` main `c9fb294e88b4dd958a3a7f4bd588ca9a17d52673`.  
Input: supplied “hNFT Research Report: Cryptography, Blockchains, DAGs and Distributed Mathematics”, dated 11 November 2026.

## Decision proposed

Make single-use, certified ownership states the leading candidate for public transfers. Keep Holochain for immutable records, validation, discovery and provenance. Add an explicitly governed uniqueness service that certifies one successor of each input state. Sender and recipient consent establishes authorization; a certificate establishes the exclusive decision under the stated witness assumptions.

Begin with a fixed four-witness, three-signature experiment using ordinary independent signatures. It is a safety prototype, not a complete production agreement protocol. Recovery, cancellation, expiry, publication availability and operational independence remain release gates.

This is an alternative to the seller-chain design in [draft PR #2](https://github.com/ConorTttttt/hNFT/pull/2), head `b6931d03bd08e5eb8cce56efe4db6554549bed53`. It does not silently approve or supersede that draft. Before implementation, choose one authority model and reconcile ADR-0001, its diagram and validation matrix. Preserve its source audit and provisional-acceptance insight. Seller-only finalization can be evaluated for a restricted demonstration, but does not meet the stronger requirement of prevention against a seller that forks its source chain.

## Evidence from the actual project

The following are static source observations, not runtime exploit results:

| Location | Observed behavior | Required improvement |
| --- | --- | --- |
| `dnas/nft/zomes/integrity/nft/src/transfer.rs` | Receipt accepted at/before the deadline; cancellation accepted after it on another chain. Both conditions can hold for different actions. | Acceptance must not grant title; conflicting terminal decisions must share one exclusivity mechanism. |
| Same file, `seller_has_open_duplicate` | A cancellation allows the same `prev_state` to be offered again. | A closed certified state must never be resurrected. |
| `dnas/nft/zomes/coordinator/nft/src/query.rs` | `current_owner` selects the last receipt after sorting observed links; `resolve_hops` checks sequence contiguity but not exact predecessor lineage. | Verify explicit parent hashes and certificates; return observed-state and missing-evidence status. |
| Same file, `get_nfts_for_agent` | Wallet holdings come from derived links. | Links remain discovery hints, not spend authorization. |
| `README.md` | Identity is the original mint Create ActionHash; blind mint uses commit/reveal. | Preserve token identity; add certified state identity and issuance-slot uniqueness separately. |
| Draft PR #1 | Gate 0.1 is NO-GO pending lockfiles, builds and Holochain 0.7.1 verification. | Keep toolchain verification independent; do not count this research model as Holochain test evidence. |

## Scope and trust contract

The curated v0.1 scope is collection issuance, gifting/transfers and independently verifiable provenance. Payment settlement, open witness admission, royalties enforcement, key recovery and cross-DNA asset portability are separate designs.

For the prototype, membership, threshold and policy are immutable for the collection network. An authoritative collection configuration pins the committee hash and four distinct eligible keys. The proposal cannot select its own witnesses. Four keys on one administrator's restored machines are not four independent failure domains.

Assume at most one of the four witnesses equivocates or loses its durable voting history. Honest witnesses serialize competing requests, preserve locks across crashes, and never sign two decisions for the same input. A key copied to another signer or restored with stale state can violate that assumption. Compromise accumulated over the lifetime of this fixed committee counts toward the bound; “one at a time” is insufficient if old signing keys remain usable.

Signature unforgeability, collision-resistant hashes, correct canonical parsing and correct certificate verification are additional assumptions. Publication and retrieval availability are not consequences of signature validity.

## Ownership identity and commitments

Retain the original mint Create ActionHash as `token_id`. Do not rename existing identity semantics to an arbitrary integer. The new protocol needs a different identifier for each certified state:

```
StateBody = {
  protocol_version, dna_hash, token_id, state_version,
  owner_key, parent_state_id_or_genesis, policy_hash, committee_hash
}
state_id = H(domain_state || canonical(StateBody))

DecisionBody = {
  protocol_version, dna_hash, kind,
  input_state_id_or_issuance_slot, output_state_id,
  offer_ref, recipient_key_or_none,
  policy_hash, agreement_code_hash, committee_hash, epoch, nonce
}
decision_id = H(domain_decision || canonical(DecisionBody))
```

These are schematic data definitions, not a frozen wire format or compilable Rust. Domain labels, hash function, typed byte lengths, option tags, integer encoding, ordering and rejection of noncanonical encodings must be fixed in Gate 0.2-C before implementation. Use separate signing contexts for offer, acceptance, witness decision and status responses. Publish the offer intent first, without the later decision hash; construct the decision using that immutable offer reference, then obtain owner and recipient authorizations over the decision ID. This avoids a second cycle between an offer's ActionHash and the decision it authorizes. An acceptance is by the specified recipient and commits to the exact decision, never to a mutable link.

**Avoid circular hashes:** the state body excludes its certificate and the decision hash that certifies it. The decision commits to the output state hash. A `CertifiedState` pairs the body with evidence. Alternate quorum subsets and duplicate DHT publications of the same decision must identify the same logical state. Holochain ActionHashes locate published records; they are not interchangeable with state/decision digests.

Each transfer consumes one certified input and creates one output with the same token, checked version increment and the named recipient as owner. Validate its full ancestry to an authorized issuance. A legitimate owner may reacquire a token at a later state; that does not revive earlier spent states.

## Finalization and quorum argument

Each honest witness first validates the candidate and referenced evidence, then atomically persists `input_key -> decision_id`, and only then emits a signature. Same-input/same-decision retries reproduce the authorization; same-input/different-decision retries fail. The signing path must not bypass this store. An implementation must prove the persistence/acknowledgement ordering across actual crashes, disk failures, process concurrency and restore procedures.

A certificate has at least three **distinct authorized** witness signatures over the same canonical decision. Verify committee membership from the input's authenticated policy, not from candidate-supplied membership. Reject unknown members, duplicate signers, mixed decisions, unsupported versions and wrong-network evidence. Embed or resolve all required bodies and signatures deterministically.

For n witnesses, quorum q and at most f Byzantine witnesses, two quorums intersect in at least `2q - n`. Safety requires `2q - n > f`. With n=4, q=3, f=1, intersection is at least two, including an honest witness. Two conflicting certificates would require an honest witness to violate its persistent lock. This is a conditional argument for one fixed conflict domain; it does not supply a recovery protocol, membership handover or software correctness proof.

Treat a valid certificate as authoritative even if its publication was delayed. A local timeout or missing DHT link cannot revoke it. Relays may publish complete certificates without being the old owner; the evidence determines authority. Distinguish certification from local observation and durable public availability.

## Lifecycle and the cancellation trap

Product lifecycle remains `Owned -> Offered -> Accepted -> Finalized`. Offered and Accepted are provisional observations: different peers may see different stages. No wallet rights change before the certificate verifies.

All irreversible decisions concerning a given input must use the same lock namespace. In particular, cancellation or expiry must not clear witness locks. A nonce, round, offer ID or committee epoch must not create a second independent lock namespace for the same spendable input.

Proposed terminal-decision semantics to model next:

| Decision | Evidence required | Certified output |
| --- | --- | --- |
| TRANSFER | Owner authorization and named recipient acceptance of the exact decision | New state, recipient owns |
| CANCEL | Owner-authorized closure, with eligibility fixed in the reviewed policy | New state, same owner |
| EXPIRE | Expiry evidence under an explicitly reviewed clock/ordering policy | New state, same owner |

Cancellation and expiry consume the old state too. Reoffering uses the newly certified state and a new offer. This fences delayed acceptances and certificates for the original state. A UI withdrawal without a closure certificate is only a request, never proof that an already authorized transfer cannot finish.

This formulation changes ADR-0001's reuse of the original state after closure. It avoids unsafe resurrection but introduces competing decisions that can split votes. The safety-only prototype must leave such states pending; it has no safe unlock/retry rule. Product fairness also needs a decision: under owner-authorized cancellation, a recipient acceptance may lose to a certified cancellation. Do not promise irrevocable offers before defining a reservation protocol.

**Expiry remains unresolved for production.** Action timestamps do not prove real time. A complete policy must define clock bounds or certified ordering, eligibility at the signing point, delayed certificate handling, and why a late-arriving transfer certificate cannot conflict with expiry. A timer can display “deadline reached; settlement pending” but cannot free the input. Do not use current wall-clock reads inside integrity validation.

## Recovery, freshness and governance

Permanent locks prevent double certification but can freeze an asset. Even four honest witnesses split 2/2 cannot finish a 3-of-4 decision without a more capable protocol. Retrying the same candidate helps missing-message recovery; retrying a new candidate does not resolve split locks safely.

Before public transfers, select and specify a reviewed agreement/recovery protocol with conditional liveness, including view changes and lock justification where needed. Borrowing the term HotStuff or FastPay does not transfer either protocol's proof. Do not add a home-grown timeout unlock. Keep rotation disabled until old/new committee handover is modeled; changing keys or DNA configuration is not a safe recovery shortcut.

Offline verification proves a historical certified state, not that it is unspent now. The read API should return token/state IDs, version, owner at that state, proof status, observed-at metadata and freshness status. Use `HistoricalVerified`, `PendingEvidence`, `PendingSettlement`, `Conflict` and a separately defined freshness indicator. Invalid spam is rejected; reserve Conflict for contradictory authenticated evidence or an unresolved lineage conflict. A local timestamp is not a freshness certificate. Independent signed reads are not automatically linearizable.

Spending always enters the uniqueness protocol against an exact input; a stale cached owner result cannot authorize it. A live “current owner” promise requires a separately specified read protocol with a stated observation point. Display “owner at verified state” until then.

## Issuance, provenance and media

Pin a signed collection manifest (or its hash) in collection configuration before issuance. Define immutable slots such as `(collection_id, artwork_id, edition_number)` independently of a mutable manifest revision. Certify one mint allocation for each slot using the same uniqueness principle. Otherwise two different mint ActionHashes can both claim the same edition. Validate slot bounds, authorized minter, initial owner, metadata/media commitments and one authorized root. A second signed root does not silently expand supply.

A full signed manifest is sufficient for a small first collection; Merkle inclusion proofs are an optimization. Inclusion alone proves neither exclusive issuance nor completeness. Append-only transparency logs, consistency proofs and equivocation monitoring can follow. Preserve blind-mint secrecy: pre-reveal manifests commit to sealed content; they must not publish the hidden metadata preimage. A later reveal must satisfy the original commitment and cannot change ownership.

Ownership has one certified path. Creator attestations, licensing descriptions and metadata observations can form a branching provenance graph. Those records never override ownership. Content hashes establish integrity, not continued availability; release testing must restore media and proofs from independently maintained copies.

## Holochain and Unyt integration boundaries

Integrity validation verifies typed immutable dependencies, authorization, canonical hashes, certificates, policy and lineage. Missing dependencies defer validation; they are not proof of no spend. Enforce critical rules in applicable CreateRecord and entry validation paths. Reject updates/deletes of authority records. Coordinator functions orchestrate and retry; links, signals and UI guards cannot establish finality.

The witness durable store is an explicit service boundary. Ordinary DHT “read then write if absent” is not atomic exclusivity. Choose and test a native transactional witness store/signing service or another proven durable implementation after Gate 0.1. This proposal does not claim that a WASM zome has access to a suitable transactional database or that source-chain publication by itself fences cloned signers.

Use a small fixed deterministic transfer policy first. Keep agreement code/version hashes in commitments for later extensibility. Unyt concepts are useful inspiration, but its current API, security guarantees and 0.7.1 compatibility were not audited here; no Unyt integration is implemented or required for this prototype. Atomic NFT-for-payment requires a separate joint commit/abort and timeout design across both systems; sequential successful calls are insufficient.

## Verification and approval

The accompanying Python explorer checks only the one-input, two-candidate fixed-committee abstraction and deliberately unsafe variants. It does not model offer eligibility, clocks, closure/reoffer, multiple states, cryptography, Holochain validation, disk I/O, membership changes or fairness. Passing finite exploration is not an unbounded proof or an implementation audit.

Extend the model in TLA+ (or a comparably reviewable specification) before runtime work: issuance, owner authorization, accept/cancel/expire races, state versions, publication, stale reads, crash recovery and chosen agreement rounds. Record bounded model sizes, invariants, assumptions and counterexamples. Prove/refine critical properties beyond finite exploration where required.

Approval remains pending: protocol owner chooses authority and terminal semantics; security reviewer accepts the fault model and recovery protocol; operations demonstrates independent witnesses and safe restore; release lead confirms Gate 0.1 and adversarial integration evidence. Public transferable NFTs are NO-GO until those conditions are met.

## Sources and limits

Primary sources checked on 11 November 2026. The hNFT design above is our proposed adaptation, not a claim that these sources prove hNFT safe.

- [Holochain countersigning](https://developer.holochain.org/concepts/10_countersigning/): agreement and source-chain fork detection do not by themselves prevent scarce-asset double spending; countersigning is documented as unstable/default-disabled.
- [Corda consensus](https://docs.r3.com/en/platform/corda/4.14/community/key-concepts-consensus.html): separates validity from uniqueness.
- [FastPay](https://arxiv.org/abs/2003.11506): Byzantine consistent broadcast is a useful settlement precedent; its protocol and assumptions are not this prototype.
- [Coordination Avoidance in Database Systems](https://arxiv.org/abs/1402.2237): invariant confluence motivates coordinating operations whose independently valid effects cannot safely merge. Applying it to exclusive ownership is our design inference.
- [HotStuff](https://arxiv.org/abs/1803.05069): a primary reference for partially synchronous BFT agreement; used as further reading for recovery, not implemented here.
- [RFC 9162](https://www.rfc-editor.org/rfc/rfc9162.html): distinguishes inclusion and consistency proof machinery for transparency.
- [TLA+ tools](https://lamport.org/tla/tools.html): TLC model checking and TLAPS proof checking are different kinds of evidence.

The supplied report's broader claims about Sui, Unyt, DAG systems and threshold cryptography were not independently re-audited here. They are not release dependencies or claimed performance evidence.
