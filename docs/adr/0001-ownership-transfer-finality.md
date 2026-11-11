# ADR-0001 — hNFT Ownership, Transfer Finality and Conflict Resolution

**Status:** PROPOSED — architecture review required; NOT APPROVED  
**Date:** 2026-11-11  
**Gate:** 0.2 / S0-02 (S0-02A–G)  
**Repository baseline:** ConorTttttt/hNFT, main @ c9fb294e88b4dd958a3a7f4bd588ca9a17d52673  
**Target:** curated hNFT v0.1 collection; future open-collection migration deliberately out of scope  
**Depends on:** Gate 0.1 Holochain 0.7.1 API/validation verification

## 1. Context and current-state audit (S0-02A)

The current implementation in dnas/nft/zomes/integrity/nft/src/transfer.rs models seller-authored TransferCommitment, buyer-authored TransferReceipt and seller-authored TransferCancellation. The coordinator exposes initiate_transfer, accept_transfer, cancel_transfer and drop_stale_ownership_link. TransferReceipt immediately changes ownership; current_owner and get_provenance read receipt links (NftToReceipt), sort by sequence, and use highest sequence as owner. AgentToNft links are a derived wallet index. Sweettests cover single and two-hop transfers, double acceptance, named recipient, expiry window and some stale-index cases.

**Critical defect A — acceptance and cancellation coexist.** Existing receipt validation allows an acceptance action timestamp no later than the offer deadline; cancellation validation allows a separate seller action timestamp later than the same deadline. Both may be valid simultaneously. Disjoint timestamp ranges do NOT make the actions mutually exclusive. The cancellation validator does not establish that the offer was unaccepted. A cancellation after acceptance then allows seller_has_open_duplicate to treat the old ownership state as re-offerable, even though receipt-based provenance may already assign it away.

**Critical defect B — no ownership-changing finalization.** drop_stale_ownership_link only removes a derived link; it is not a finalization record. There is no seller-chain event that exclusively chooses between completion and release of an ownership state.

**Critical defect C — incomplete or stale reads.** current_owner derives a head from independently visible NftToReceipt links; a missing link can make an obsolete owner look current. The function checks sequence contiguity but does not independently verify each hop's prev_state reference. Wallet links can legitimately be stale.

**Important limitation D — untrusted clocks and network convergence.** Action timestamps are self-reported (subject to source-chain continuity checks), not a global trusted clock. A local DHT query cannot prove absence of later, hidden or delayed spends. Agent forks/warrants are eventually discovered, not globally synchronously known.

The README reports 13/13 prior Sweettests passing on Holochain 0.7.0; this audit inspected source and did **not** rerun tests or verify 0.7.1 compatibility.

## 2. Decision (S0-02B–F)

**Adopt an append-only, seller-chain-serialized transfer protocol.** A buyer's acceptance is an acknowledgement of a proposed transfer, NOT an ownership grant. Ownership changes ONLY when the current owner commits a valid TransferFinalization action referencing a valid TransferAcceptance. Every terminal choice for a particular ownership state is authored by its current owner and ordered by that owner's source chain. This is a bounded, agent-centric finality model, not a global atomic transaction or blockchain-style settlement.

Use the stock Holochain integrity/coordinator split, with no required countersigning. Do not infer ownership from UI links, signals, acceptance, elapsed wall-clock time or a maximum sequence in a non-complete DHT query.

A verified valid finalization establishes a **positive transfer-of-title proof** relative to the protocol lineage. A client cannot cryptographically prove that this title has never since been spent without seeing a sufficiently current view of relevant chains. Expose the distinction between *verified lineage* and *current observed head* explicitly.

## 3. Ownership authority and proof

- NFT identity: original Create action hash for NftManifest. The mint's action author is the genesis owner. Production collections must have a configured single mint authority per DNA properties; open-mint mode is dev-only.
- OwnershipState = either **MintState(nft_action_hash, owner=minter, sequence=0)** or **FinalizedState(finalization_action_hash, owner=offer.to_agent, sequence=offer.sequence)**. This is a virtual derived type; no mutable owner field or globally writable register.
- Authority to spend a state: only the AgentPubKey identified as owner in the referenced valid state, proved through action authorship and immutable lineage. A seller's *own authored action* creates finalization.
- Ownership proof: DNA hash + mint create + ordered chain of (offer, buyer acceptance, seller finalization) records + validation of every hash, signature, type, author, referenced previous state and sequence. The latest known valid Finalization creates the next OwnerState; no TransferAcceptance alone does.
- A reviewer must also check seller source-chain activity/status, discovered warrants/forks, and DHT availability. This is a risk-based confirmation step in the coordinator/client, not a deterministic integrity-validator dependency.
- Wallet/discovery/provenance links are rebuildable hints. They MUST NOT grant rights or change owner state. A missing link does not invalidate an otherwise provable finalization.

## 4. Immutable entries (schema v1; proposed, not yet implemented)

Use public entry Create actions, immutable and non-deletable; references are ActionHash, never EntryHash. Existing structure names may be retained through a deliberately versioned migration, but **do not reuse old TransferReceipt as an owner state**.

~~~rust
struct TransferOffer {
    nft: ActionHash,              // original NftManifest Create
    prev_owner_state: ActionHash, // mint or prior TransferFinalization
    to_agent: AgentPubKey,
    sequence: u32,                // previous state + 1; checked overflow
    schema_version: u8,          // 1
}
struct TransferAcceptance { offer: ActionHash, schema_version: u8 }
struct TransferCancellation { offer: ActionHash, schema_version: u8 }
struct TransferExpiry { offer: ActionHash, schema_version: u8 }
struct TransferFinalization {
    offer: ActionHash,
    acceptance: ActionHash,
    schema_version: u8,
}
~~~

The seller is the offer/finalization/cancellation/expiry action author; the buyer is the acceptance action author. Each offer's identity is its Create ActionHash. No separately mutable status field exists. All validation must fetch and check exactly the referenced Create records. The finalized state hash is the TransferFinalization Create ActionHash.

**Time constants:** accept_by = offer.action.timestamp + DNA.transfer_window_secs; finalize_by = accept_by + DNA.settlement_grace_secs. The latter is a new DNA property, strictly positive, chosen for v0.1 deployment; any deadline arithmetic overflow is invalid. DNA-level configuration ensures identical rule inputs for all validators. The actual durations and timestamp tolerance require Gate 0.1 test verification. These are logical timestamp deadlines, not a trusted wall-clock oracle.

## 5. State machine and transitions (S0-02C)

~~~mermaid
stateDiagram-v2
    [*] --> Owned
    Owned --> Offered: seller creates valid Offer
    Offered --> Accepted: named buyer creates Acceptance (provisional)
    Accepted --> Finalized: seller creates valid Finalization referencing Acceptance
    Finalized --> Owned: buyer receives new OwnershipState
    Offered --> Cancelled: seller creates Cancellation (before accept_by)
    Offered --> Expired: seller creates Expiry (after finalize_by)
    Accepted --> Cancelled: seller cancels before finalization; acceptance orphaned
    Accepted --> Expired: no finalization before close; acceptance orphaned
    Cancelled --> Owned: original owner retains state
    Expired --> Owned: original owner retains state
~~~

Important: the Accepted state describes observable buyer intent; it is NOT a globally serialized, ownership-changing state. A contemporaneous cancellation and acceptance may both be structurally valid because they live on separate chains; Cancellation is authoritative if recorded on the seller chain before finalization. A finalization and cancellation or expiry may NOT both be valid for the same ownership state.

**Offer:** only owner of prev_owner_state; recipient must differ from seller; no currently open offer or already finalized spend for that state on seller's prior chain. Offer is the sole active attempt for that state. Receiver sees it through best-effort inbox link/signal plus on-demand record validation.

**Accept:** only named recipient; timestamp <= accept_by, timestamp >= offer timestamp where enforceable; at most one acceptance by buyer on its prior source chain for an offer. Acceptance can race with seller cancellation or expiry and never implies title. A buyer MUST wait for finalized proof before listing or forwarding the NFT.

**Cancel:** seller only, before or at accept_by, and only if the seller's prior chain has no Cancellation, Expiry or Finalization for the same offer/state. Because the seller cannot deterministically inspect every remote acceptance, cancellation is permitted even if a remote acceptance exists; it takes effect ONLY if no earlier valid finalization is on the seller chain. This is a deliberate v0.1 fairness/availability trade-off; do not market offers as irrevocable.

**Expire:** seller only, after finalize_by, if no prior terminal action for the offer/state on seller's chain. No remote observer may author an authoritative Expiry for another agent. An elapsed timer produces a non-authoritative UI label, ExpiryEligible, until the seller creates TransferExpiry. No party changes ownership on mere timeout.

**Finalize:** seller only; references same offer and a valid acceptance by its named buyer, accepted no later than accept_by; finalization timestamp <= finalize_by. Offer must be in the seller's own source-chain history and not preceded there by Cancellation, Expiry or any other Finalization consuming prev_owner_state. A finalization consumes that ownership state forever. The new owner is exactly offer.to_agent. Acceptance and Finalization are separate, never claimed to be atomically committed across chains.

**After close:** Cancellation and Expiry release the original owner state for a NEW offer with a NEW offer action hash; a stale acceptance cannot finalize a closed offer. After Finalization the seller cannot create another offer or finalization consuming that state; buyer's future offers reference the finalization hash as prev_owner_state.

## 6. Deterministic conflict rules (S0-02D/E)

**Authoritative order:** for each owner and ownership state, inspect the bounded prefix of that agent's valid source chain up to the action being validated. Terminal actions on that chain are totally ordered by source-chain action sequence, not action timestamp or DHT arrival order. The first valid terminal spend/release action decides; later conflicting terminal events are Invalid. Expiry is an explicit seller event, not a global negative assertion. A seller cannot bypass the state history by publishing an alternative state chain that is valid and unforked.

**Concurrently authored acceptance vs cancellation:** a buyer acceptance can be valid but remain orphaned if the seller cancels before finalization. It cannot move title. Return status Cancelled with an explanation, never the buyer as owner.

**Two offers:** a second open offer for the same owner state is Invalid; after a valid Cancellation/Expiry a fresh offer is possible. Never reopen after Finalization. Multiple buyer acceptances for the same offer are Invalid on the named buyer's own linear chain.

**Two finalizations or finalize-vs-cancel/expire:** all are seller-authored; compare the seller's preceding history. One terminal event can survive on a valid linear chain. In a genuine source-chain fork, Holochain's fork/warrant handling applies; surface Conflict/Unverified and suspend transfers, do not apply a lexicographic/action-hash tie-break that fabricates global consensus.

**DHT partitions/missing dependencies:** if parent action/receipt/source-chain slice is not available to integrity validation, return UnresolvedDependencies via must_get_* and retry, NOT Valid and NOT Invalid solely due to absence. Coordinator reads return PendingNetwork/Unverified, never a guessed owner. Do not select a 'winner' from currently available NftToReceipt links.

**Replayed, malformed, unauthorized events:** Invalid; coordinator rejects locally where possible, but integrity validation is the security boundary.

## 7. Validation responsibilities and algorithm (S0-02D)

Every security-critical constraint belongs in **integrity zome validate**, including the CreateRecord op path (because must_get_valid_record checks CreateRecord validity). Mirror applicable checks for AgentActivity ops; neither a UI guard nor a coordinator guard suffices. Deterministic dependency reads only: must_get_action, must_get_valid_record, must_get_entry, must_get_agent_activity with a fixed, known chain top. The source-chain scan MUST end at the candidate action's prev_action (never query an unbounded 'latest' head or mutable links); validate a complete contiguous prefix and handle 0.7.1 response variants. For performance, only expand transfer-type actions and bound scans to the ownership epoch if a verified checkpoint can be defined; **do not assume such an optimization before proving it safe**.

| Entry | Required integrity checks |
| --- | --- |
| Mint | valid curated minter, type and DNA properties; unique NFT ID is create action; existing mint-cap rules retained |
| Offer | author = holder of valid prev_owner_state; NFT match; sequence increment; recipient != author; prior seller chain has no outstanding offer for state or finalized spend; version valid |
| Acceptance | valid offer; action author = to_agent; action timestamp within offer window; no duplicate acceptance on buyer's prior chain; version valid |
| Cancellation | action author = offer author; timestamp <= accept_by; prior seller chain contains this offer and no terminal for offer/state; version valid |
| Expiry | action author = offer author; timestamp > finalize_by; prior seller chain contains this offer and no terminal for offer/state; version valid |
| Finalization | author = offer author = owner of prev_owner_state; valid matching acceptance by recipient; acceptance <= accept_by; finalization <= finalize_by; previous source-chain prefix has no cancellation/expiry/finalization for this offer and NO earlier finalization consuming this owner state; version valid |
| Provenance / wallet / inbox links | type/base/target/author checks; links are append-only derived discoverability and cannot settle title; missing links do not authorize inferred ownership |

Check deadline ordering, checked arithmetic, impossible sequence overflow, DNA mismatch and self-transfer. Do not read current time, get_links, get_details or 'latest DHT' inside validate. The coordinator MAY use dynamic reads for UX/visibility, but must report uncertainty.

**Read API:** replace current_owner: AgentPubKey with an explicit resolution result { last_verified_owner, owner_state_hash, sequence, observed_at, status: VerifiedObserved | PendingDependencies | Conflict | Unknown, proof_hashes }. 'VerifiedObserved' means the observed lineage is valid, not proof that no later valid finalization exists. get_provenance must follow prev_owner_state hashes, verify each linked record and report gaps/forks rather than trusting sorted receipt links. Use a new API version; update client and UI copy to avoid misleading statements like 'NFT is yours' immediately after accept.

## 8. Threat model, safety and availability limits

1. **Safety, conditional:** On an unforked seller chain with the correct validation code, no two valid TransferFinalization actions can spend one ownership state. An owner holding different keys cannot grant title without following an authorized previous-state chain.
2. **Not synchronous global finality:** DHT visibility is eventually consistent. A fresh finalized record is proof of a transfer, but a peer may not yet know of that record or another later one. Consumers must verify provenance and relevant chain status; risk tolerance must be specified for public launches.
3. **Forks:** Malicious/compromised conductors and restored stale source chains can produce contradictory temporarily visible histories. Reject known forks/warrants; no canonical winner is asserted during an unresolved fork.
4. **Time:** Timestamps are self-reported; deadline comparisons are deterministic with fixed inputs, NOT guarantees of real-world expiry. No payments/escrow, immediate delivery promises or irreversible exchange semantics in v0.1. If hard clock fairness is a requirement, this ADR is insufficient and needs a new notary/consensus design.
5. **Liveness:** Seller must come online to finalize or expire. After an unfinalized acceptance, buyer has no title; the NFT remains seller-owned. After an offline seller misses finalize_by, no finalization can be valid and seller can later post Expiry and re-offer.
6. **Keys and API:** Agent key loss, malicious owners and protocol-version migrations are outside this protocol. A separate recovery/guardianship ADR would be needed.
7. **Countersigning:** deliberately not selected for v0.1; it is a session coordination feature, not a standalone double-spend solution. Reconsider for higher-assurance exchanges only after capability verification and separate ADR.

## 9. Required implementation changes (not performed in this gate)

- Add TransferFinalization and TransferExpiry integrity entry types plus validators; replace receipt-as-ownership with acceptance-as-provisional and a new finalized owner-state reference.
- Change offer validation and seller source-chain scan to reject previously finalized spends independently of cancellation; terminal events require exclusive seller-chain history.
- Rewrite current_owner, next_transfer_state and get_provenance to reconstruct the immutable ownership lineage with uncertainty statuses and cross-links as hints only.
- Add coordinator finalize_transfer and expire_transfer; retain explicit cancel_transfer with changed pre-deadline semantics; revise inbox and stale-link cleanup flows.
- Update TS types, client calls, UI statuses and copy. Track old data as incompatible: **development DNA reset/new version** is required unless a separately reviewed migration preserves semantics.
- Ensure validation rules are enforced in CreateRecord as well as other applicable op paths; run and profile full history lookups.

## 10. Verification plan and acceptance evidence

See docs/protocol/ownership-validation-rules.md for matrix and test IDs; state diagram is docs/protocol/ownership-transfer-state-machine.mmd.

Gate 0.1 must prove 0.7.1 HDI/HDK signatures, ChainFilter/must_get_agent_activity behavior, validation op routing, timestamp APIs and all existing tests. Gate 0.2 review must approve the safety/liveness limitations, especially seller discretion, clock trust and conditional observed finality. Then implementation gate must prove single/two-hop history, acceptance-before-cancellation race, valid acceptance followed by Expiry, two concurrent offers, two finalizations, replay, missing links, partition, fork/warrant and deadline-boundary cases.

## 11. Alternatives and consequences

- **Retain receipt-immediate ownership:** rejected because remote acceptance and seller cancellation cannot be exclusively ordered by timestamps. Existing code makes contradictory records admissible.
- **Choose 'highest timestamp/hash wins' on the DHT:** rejected; depends on an incomplete mutable view and can reverse after gossip.
- **Mandatory two-party countersigning:** deferred; increases online/session constraints and does not independently prevent double spends.
- **Trusted collection registrar/notary:** deferred. Could improve high-value confirmation by serializing authorizations through a collection authority, but adds central availability/trust and changes the open-platform path.
- **Selected seller-chain finalization:** adds a seller settlement step and provisional acceptance. It prioritizes explainable safety over buyer liveness/fairness, appropriate only for the initial curated, non-escrow v0.1 scope.

## 12. Gate decision and review record (S0-02G)

**Gate decision: NO-GO / pending approval.** This is a detailed proposed architecture, not an approved or tested protocol. Do not modify ownership zomes or claim release-grade finality solely on this ADR.

Reviewer checklist:
- [ ] Protocol/product owner accepts seller-controlled settlement and non-binding provisional acceptance.
- [ ] Holochain engineer confirms 0.7.1 integrity and source-chain APIs and security guarantees (Gate 0.1).
- [ ] Security reviewer accepts documented fork, clock, and partial-DHT limitations or requests a stricter model.
- [ ] Implementation owner accepts schema/version reset and test matrix.
- [ ] Approver records name, date and decision below.

| Role | Approver | Date | Decision |
| --- | --- | --- | --- |
| Product/protocol | Pending | — | Pending |
| Holochain/security | Pending | — | Pending |
| Release lead | Pending | — | Pending |

## References

- Local implementation: dnas/nft/zomes/integrity/nft/src/{transfer.rs,links.rs,lib.rs}, dnas/nft/zomes/coordinator/nft/src/{transfer.rs,query.rs}; README.md; coordinator Sweettests.
- Holochain validation: https://developer.holochain.org/build/validation/
- Deterministic dependency reads: https://developer.holochain.org/build/must-get-host-functions/
- Agent status / scarcity / forks: https://developer.holochain.org/build/getting-an-agents-status/
- Holochain 0.7 upgrade: https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.7/
- Countersigning limits: https://developer.holochain.org/concepts/10_countersigning/
