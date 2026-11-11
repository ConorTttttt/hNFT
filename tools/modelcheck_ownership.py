#!/usr/bin/env python3
"""Finite exploration of one input, two candidates, and a fixed witness set.

No cryptography, Holochain runtime, time, membership changes, or recovery protocol
is implemented. Signatures are unforgeable abstract witness/candidate pairs.
Run from the repository root: python3 tools/modelcheck_ownership.py
"""
from collections import deque
import json


def replace(values, index, value):
    return values[:index] + (value,) + values[index + 1:]


def explore(n=4, q=3, faults=1, mode="durable"):
    # State = durable locks, emitted signatures, delivered signatures.
    # 0 = empty; candidates A/B = bits 1/2. A Byzantine witness can emit both.
    initial = ((0,) * n, (0,) * n, (0,) * n)
    parents = {initial: None}
    queue = deque([initial])
    split = None

    def trace(state):
        actions = []
        while parents[state] is not None:
            state, action = parents[state]
            actions.append(action)
        return list(reversed(actions))

    def certificates(delivered):
        return [c for c in (1, 2) if sum(bool(x & c) for x in delivered) >= q]

    while queue:
        state = queue.popleft()
        locks, signed, delivered = state
        certs = certificates(delivered)
        if len(certs) == 2:
            return {"safe": False, "visited_states": len(parents),
                    "counterexample": trace(state)}
        # Fully delivered 2/2 split with four honest locked witnesses:
        # no further sign or lock action can make progress, even with delivery.
        if faults == 0 and all(locks) and signed == delivered and not certs:
            if all(signed[i] == locks[i] for i in range(n)):
                split = state

        successors = []
        for i in range(n):
            for c, name in ((1, "A"), (2, "B")):
                if i < faults and not signed[i] & c:
                    successors.append(((locks, replace(signed, i, signed[i] | c), delivered),
                                       f"Byzantine W{i} signs {name}"))
                if i >= faults:
                    if locks[i] == 0:
                        successors.append(((replace(locks, i, c), signed, delivered),
                                           f"W{i} persists lock {name}"))
                        if mode == "sign_before_persist" and not signed[i] & c:
                            successors.append(((locks, replace(signed, i, signed[i] | c), delivered),
                                               f"W{i} emits {name}, crashes before persistence"))
                    if locks[i] == c and not signed[i] & c:
                        successors.append(((locks, replace(signed, i, signed[i] | c), delivered),
                                           f"W{i} signs locked {name}"))
                if signed[i] & c and not delivered[i] & c:
                    successors.append(((locks, signed, replace(delivered, i, delivered[i] | c)),
                                       f"Deliver W{i}/{name}"))
            if mode == "erase_lock" and i >= faults and locks[i]:
                successors.append(((replace(locks, i, 0), signed, delivered),
                                   f"W{i} unsafely clears lock (timeout or stale restore)"))
        # Honest crash/restart in durable mode, retries, drops, and duplicates
        # are state-preserving steps. Omitting these self-loops preserves the
        # reachable-state safety check; it makes no fairness/liveness claim.
        for next_state, action in successors:
            if next_state not in parents:
                parents[next_state] = (state, action)
                queue.append(next_state)
    result = {"safe": True, "visited_states": len(parents)}
    if split is not None:
        result["stalled_split_trace"] = trace(split)
    return result


def main():
    cases = {
        "durable_3_of_4_one_byzantine": ({}, True),
        "honest_split_can_stall": ({"faults": 0}, True),
        "unsafe_sign_before_persist": ({"mode": "sign_before_persist"}, False),
        "unsafe_timeout_or_restore_unlock": ({"mode": "erase_lock"}, False),
        "outside_fault_bound_two_byzantine": ({"faults": 2}, False),
        "unsafe_2_of_4_threshold": ({"q": 2}, False),
    }
    results = {}
    for name, (options, expected_safe) in cases.items():
        result = explore(**options)
        if result["safe"] != expected_safe:
            raise AssertionError(f"Unexpected model outcome: {name}: {result}")
        if name == "honest_split_can_stall" and "stalled_split_trace" not in result:
            raise AssertionError("Expected an honest 2/2 split counterexample to progress")
        results[name] = result
    print(json.dumps({"scope": "finite abstract model; not production verification",
                      "results": results}, indent=2))


if __name__ == "__main__":
    main()
