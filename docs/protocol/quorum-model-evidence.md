# Fixed-committee model: evidence and limitations

Run date: 11 November 2026. Status: bounded abstract exploration only.

## Reproduce

From the repository root, with Python 3 and its standard library:

```sh
python3 tools/modelcheck_ownership.py
```

The script exits unsuccessfully if the declared safe case fails, an intentionally unsafe case does not produce a conflicting-certificate trace, or the all-honest case lacks a stalled split. Its JSON output includes concrete action traces. The recorded output is [quorum-model-results.json](quorum-model-results.json).

No packages, Holochain conductor or signing keys are required. This script does not replace Sweettests, canonical-format tests or a reviewed protocol specification.

## Results

| Model | Result | States discovered |
| --- | --- | ---: |
| 3-of-4, at most one Byzantine, durable locks | Exhausted reachable states; no two certificates | 3,087 |
| 3-of-4, four honest witnesses | Exhausted reachable states; safe, but a fully delivered 2/2 split stalls | 2,401 |
| Signature emitted then crash before persistence | Conflicting-certificate counterexample found | 150,700 |
| Lock erased on timeout or stale restore | Conflicting-certificate counterexample found | 124,825 |
| 3-of-4 with two Byzantine witnesses | Counterexample outside declared fault bound | 3,969 |
| 2-of-4 with one Byzantine witness | Counterexample to insufficient quorum intersection | 2,925 |

For failing variants, exploration stops at the first conflicting-certificate state; the counts include discovered states still queued. They are not counts of all possible executions, runtime throughput measurements or independent test cases.

## What is actually modeled

One already-authorized input, two otherwise-valid conflicting candidates A and B, four fixed witness identities, and an unforgeable abstract signature per witness/candidate pair. The first f witnesses may sign both candidates. Honest witnesses persist one lock before signing. Messages can be delivered in any order or remain undelivered; emitted signatures are never erased from the adversary's knowledge. Certification means three distinct delivered signatures on a candidate. Valid signed evidence cannot be revoked by losing a local copy.

An honest crash preserving durable state, repeated requests, duplicate deliveries and dropped messages are state-preserving steps and are omitted from reachable-state enumeration. This abstraction assumes the storage property it needs; the negative cases show why violating that assumption matters. It does not demonstrate that any real database or Holochain conductor implements the property.

The unsafe sign-before-persist transition models an execution where a signature escapes and a crash occurs before any durable lock. The unsafe erase transition models a timeout unlock or a restore of old voting state. Neither behavior is proposed for implementation.

The all-honest stalled split is stronger than temporary message delay: all votes are delivered, every witness is locked, neither candidate has three signatures, and the one-shot rules offer no decision-changing step. This is a counterexample to unconditional progress, not a formal check of a temporal liveness specification under fairness.

## What remains unverified

- State transitions across mint, offer, acceptance, cancellation, expiry and reoffer.
- Owner/recipient authorization, signatures, canonical serialization and hash domains.
- Real clocks, expiration eligibility and fairness between terminal decisions.
- Recovery/view changes, committee rotation, accumulated key compromise and read freshness.
- Network topology, availability of complete certificate bodies and media.
- Real crash durability, disk corruption, cloned signers and concurrent service requests.
- Holochain 0.7.1 builds, integrity validation, Sweettests, UI behavior and migration.

The next model must cover the chosen full lifecycle and recovery protocol, including intentionally unsafe variants. A finite exploration result is not an unbounded theorem; a mathematical protocol argument is not automatically an implementation proof.
