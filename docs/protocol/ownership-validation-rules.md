# Gate 0.2 — Ownership Validation Rules and Verification Matrix

**Status:** PROPOSED, linked to ADR-0001; not tested or approved.  
**Baseline:** hNFT main @ c9fb294e88b4dd958a3a7f4bd588ca9a17d52673.  
**Scope:** curated collection, protocol schema v1 in docs/adr/0001-ownership-transfer-finality.md.

## A. Validation outcomes and implementation contract

- **Valid:** all required typed Create records and a complete contiguous author-chain prefix prove the action legal under immutable DNA properties.
- **Invalid:** deterministic proof of wrong author, schema, NFT lineage, prior terminal state, bad sequence, replay, self-transfer or deadline condition.
- **UnresolvedDependencies:** required immutable action, validated record or exact source-chain slice unavailable. Never substitute current get_links/get_details, current_wall_clock, local wallet links, remote signals or an incomplete DHT read inside validate.
- **Coordinator PendingNetwork/Conflict:** UI/API must separate incomplete evidence from an invalid operation. Only Finalization transfers title.
- Enforce security rules on **CreateRecord** as well as relevant CreateEntry/AgentActivity paths. must_get_valid_record only reflects CreateRecord validation, so do not put required checks exclusively on a link or separate DHT op.
- For fixed-slice source-chain checks: candidate.action.prev_action is chain_top; retrieve the ordered contiguous prefix with must_get_agent_activity. Read only the transfer entries needed; compare by original Create ActionHash, not EntryHash. Fail closed on unknown 0.7.1 response variants until explicitly handled.

## B. Rule catalog

| ID | Action | Rule | Expected outcome on violation |
| --- | --- | --- | --- |
| VAL-001 | All | Correct DNA, public immutable entry type, version=1, checked arithmetic, reference type, canonical ActionHash | Invalid |
| VAL-002 | Mint | Mint author allowed by collection DNA; mint cap consistent with origin minter chain | Invalid |
| VAL-003 | Offer | NFT hash points to original valid mint Create | Invalid / UnresolvedDependencies |
| VAL-004 | Offer | prev_owner_state is mint or a valid Finalization for same NFT | Invalid / UnresolvedDependencies |
| VAL-005 | Offer | action.author equals owner derived from prev_owner_state | Invalid |
| VAL-006 | Offer | sequence = previous sequence + 1, no overflow, to_agent != seller | Invalid |
| VAL-007 | Offer | seller chain contains no other unresolved offer for same state | Invalid / UnresolvedDependencies |
| VAL-008 | Offer | seller has never already Finalized/spent prev_owner_state | Invalid |
| VAL-009 | Acceptance | offer valid; acceptance author equals offer.to_agent | Invalid / UnresolvedDependencies |
| VAL-010 | Acceptance | action timestamp at or before accept_by and not before offer timestamp | Invalid |
| VAL-011 | Acceptance | buyer's prior chain has no previous Acceptance for that exact offer | Invalid / UnresolvedDependencies |
| VAL-012 | Cancellation | action author equals offer author; offer on seller chain; action.timestamp <= accept_by | Invalid |
| VAL-013 | Cancellation | seller chain has no terminal Cancellation/Expiry/Finalization for offer and no prior Finalization spending same state | Invalid / UnresolvedDependencies |
| VAL-014 | Expiry | action author equals offer author; offer on seller chain; action.timestamp > finalize_by | Invalid |
| VAL-015 | Expiry | no earlier seller-chain terminal event for offer/state | Invalid / UnresolvedDependencies |
| VAL-016 | Finalization | action author equals offer author, owner of referenced state | Invalid |
| VAL-017 | Finalization | named buyer authored valid referenced Acceptance for SAME offer, on-time | Invalid / UnresolvedDependencies |
| VAL-018 | Finalization | action.timestamp <= finalize_by | Invalid |
| VAL-019 | Finalization | offer appears earlier in seller's own chain; no prior terminal action for same offer | Invalid / UnresolvedDependencies |
| VAL-020 | Finalization | seller's chain has no earlier Finalization consuming same prev_owner_state, even via another offer | Invalid |
| VAL-021 | New owner state | owner = offer.to_agent, sequence = offer.sequence; next-hop ref exactly current finalization ActionHash | Invalid |
| VAL-022 | All close actions | source-chain action authorship and ordered history are checked; no cross-chain absence claims | Invalid / UnresolvedDependencies |
| VAL-023 | Links | Valid base, target, tagged evidence and permitted link author; links cannot act as ownership proof | Invalid |
| VAL-024 | Read API | Every hop verifies prev_owner_state, NFT, authors, sequence and finalization, not just link ordering | PendingNetwork / Conflict |
| VAL-025 | Read API | Agent fork or known warrant causes Conflict/Unverified and disables owner-authorizing action | Conflict/Unverified |
| VAL-026 | Time | accept_by/finalize_by derived by checked arithmetic from offer action and immutable DNA properties | Invalid |
| VAL-027 | Migration | Legacy receipt is never treated as a new protocol Finalization by a validator or reader | Invalid / Unknown |

Time policy: accept_by = offer.ts + transfer_window_secs; finalize_by = accept_by + settlement_grace_secs. Cancellation is allowed <= accept_by, Expiry only > finalize_by. The interval after accept_by through finalize_by is reserved for settlement; an offer remains locked until valid Finalization or Expiry. All timestamp rules are deterministic comparisons of published action data, NOT proofs of accurate real-world time.

## C. Seller-chain terminal invariant: required pseudocode

~~~text
function seller_history_before(candidate):
    return must_get_agent_activity(
        candidate.author,
        fixed_chain_filter(chain_top=candidate.prev_action)
    )
    // Must be complete, contiguous, excludes candidate, handles 0.7.1 responses.

function validate_offer(o, author, prior):
    state = valid_owner_state(o.prev_owner_state)
    require state.nft == o.nft
    require author == state.owner
    require o.sequence == state.sequence + 1
    require o.to_agent != author
    require prior contains no Finalization spending o.prev_owner_state
    // Historical offers permitted only if each earlier attempt has exactly one
    // valid Cancellation or Expiry, and none has a Finalization.
    require no unclosed prior Offer for o.prev_owner_state

function validate_finalize(f, author, prior):
    offer = valid_offer(f.offer)
    acceptance = valid_acceptance(f.acceptance)
    require author == offer.action.author
    require acceptance.offer == f.offer
    require acceptance.action.author == offer.to_agent
    require acceptance.action.timestamp <= accept_by(offer)
    require f.action.timestamp <= finalize_by(offer)
    require prior contains offer action
    require prior contains no valid terminal for f.offer
    require prior contains no Finalization spending offer.prev_owner_state
    // This action is now the single owner-state successor of prev_owner_state.

function validate_cancel_or_expire(close, author, prior):
    offer = valid_offer(close.offer)
    require author == offer.action.author
    require prior contains offer and no terminal for offer
    require prior contains no Finalization consuming offer.prev_owner_state
    if cancel: require close.timestamp <= accept_by(offer)
    if expire: require close.timestamp > finalize_by(offer)
    // May orphan a remote Acceptance; it never granted title.
~~~

**Do not add a mutable 'is-spent' link query to validate.** The definitive check is the immutable seller source-chain prefix. A visible pending acceptance may be ignored in deciding Cancellation because it is authored by the buyer and cannot be proven absent network-wide.

## D. State reconstruction / query API

Inputs: NFT Create ActionHash, optional expected owner_state hash. Outputs:

~~~text
OwnershipResolution {
  nft, last_verified_owner, owner_state_hash, sequence,
  proof_action_hashes: [mint, offer_1, acceptance_1, finalization_1, ...],
  status: VerifiedObserved | PendingDependencies | Conflict | Unknown,
  checked_at: local UI timestamp // informational only
}
~~~

Reconstruct by referencing immutable Finalization entries linked/discovered through DHT. Validate each hop recursively back to mint. Confirm successor.offer.prev_owner_state == previous Finalization; confirm author and sequence. Multiple *apparently* valid children, missing provenance, known fork/warrant, mismatched lineage, or stale network data => Conflict/PendingDependencies. No arbitrary 'highest timestamp/hash' tiebreak.

**Limit:** VerifiedObserved is a snapshot assertion, not proof of a globally latest head. For consequential use, coordinator should check agent activity and warrants on the relevant owners (non-deterministic read), and the product must specify confidence/confirmation policy.

## E. Attack and regression tests — required after architecture approval

| Test | Setup | Expected result |
| --- | --- | --- |
| OWN-001 | Mint with authorized curated key | MintState owner is minter |
| OWN-002 | Authorized offer, named buyer accepts, seller finalizes | buyer owns only after Finalization |
| OWN-003 | A -> B -> C finalized twice | proof chain has exact prev_state and sequences 1, 2 |
| OWN-004 | Third party attempts offer or finalization | Invalid |
| OWN-005 | Buyer A accepts offer intended for B | Invalid |
| OWN-006 | Buyer accepts same offer twice | second Invalid |
| OWN-007 | Seller opens two simultaneous offers for one state | second Invalid |
| OWN-008 | Seller finalizes same state twice, using different offers or acceptances | second Invalid, no double spend |
| OWN-009 | Seller cancels before acceptance, buyer later accepts within nominal window | acceptance may exist; finalization Invalid; seller remains owner |
| OWN-010 | Buyer accepts before deadline, seller cancels later before accept_by | Cancellation wins if seller closes first; buyer acceptance orphaned; no title |
| OWN-011 | Buyer accepts before accept_by, seller fails to finalize, seller Expiry after finalize_by | Expiry valid, seller still owner; buyer never owner |
| OWN-012 | Seller Finalizes then tries to Cancel or Expire | Invalid close; buyer is owner |
| OWN-013 | Seller Cancels then tries to Finalize | Invalid Finalization |
| OWN-014 | Seller Expires then creates new Offer referencing same state | legal; prior acceptance cannot authorize old offer |
| OWN-015 | Acceptance action timestamp = accept_by | Valid; > accept_by Invalid |
| OWN-016 | Finalization timestamp = finalize_by | Valid; > finalize_by Invalid |
| OWN-017 | Cancellation timestamp = accept_by | Valid; > accept_by Invalid |
| OWN-018 | Expiry timestamp = finalize_by | Invalid; > finalize_by Valid |
| OWN-019 | Lost DHT link to valid Finalization | reader reports PendingDependencies / queries authoritative proof; no owner guess |
| OWN-020 | Only receipt and wallet link visible, no Finalization | original owner unchanged |
| OWN-021 | Missing parent or incomplete must_get_agent_activity prefix | UnresolvedDependencies and retried |
| OWN-022 | Known agent fork/warrant reported | Conflict/Unverified; no silent tiebreak |
| OWN-023 | Malformed ActionHash, wrong entry type, forged link, wrong DNA/version | Invalid |
| OWN-024 | Buyer disconnects after Acceptance | no title transition until Seller Finalization; eventual Expiry |
| OWN-025 | Owner loses key / is offline after offer | availability risk documented; no third-party unilateral finalization |
| OWN-026 | Mint, offer, accept, finalize under 0.7.1 Sweettest with two conductors | build/runtime verified |
| OWN-027 | Legacy TransferReceipt from 0.7.0 is read under new schema | not silently treated as finalization; migration/reset enforced |
| OWN-028 | Seller attempts to backdate/forward-date action | test source-chain monotonicity constraints, document non-trusted clock limitations |
| OWN-029 | Concurrent finalization requests on same local conductor | precisely one valid terminal action |
| OWN-030 | Receipt/link DHT propagation delayed in partition test | no finality inferred from partial view |
| OWN-031 | Owner reacquires same NFT in a later state, offers it again | allowed against new state; cannot resurrect old spent state |
| OWN-032 | Post-accept finalized proof missing from UI but authored on seller | UI cannot show false owner; PendingNetwork/Unverified |

Tests OWN-009 through OWN-014 directly cover the current protocol's untested accept/cancel and double-transfer failures. A legacy Sweettest green result is not sufficient evidence for these properties.

## F. Static review and performance checklist

- [ ] Confirm 0.7.1 HDI/hdk versions and entry type derivation.
- [ ] Confirm exact must_get_agent_activity return structure, fork handling, filter semantics, and partial-history behavior.
- [ ] Put essential checks in CreateRecord path; test independently with coordinator zome calls and DHT publishing.
- [ ] Profile historical chain scans with 1, 100, 1,000 transfers and unrelated actions; never silently truncate a security check.
- [ ] Verify no OpenWorld 'negative evidence' is interpreted as proof of absence.
- [ ] Test delayed gossip and known fork/warrants with separate conductors; check actual test harness capability.
- [ ] Verify new DNA properties and reset/migration strategy; never accept mixed protocol evidence.
- [ ] Update frontend, including removing 'Offer accepted — NFT is yours' before Finalization.
- [ ] Record approval of seller liveness, buyer fairness and non-trusted timestamp trade-offs.

**Gate disposition:** ready for review as a design specification; NOT APPROVED; Gate 0.2 remains NO-GO.

References: docs/adr/0001-ownership-transfer-finality.md; https://developer.holochain.org/build/must-get-host-functions/; https://developer.holochain.org/build/getting-an-agents-status/; https://developer.holochain.org/resources/upgrade/upgrade-holochain-0.7/ .
