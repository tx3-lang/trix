---
id: state-transition-001
name: Unsafe state transition validation
severity: high
description: Ensure transitions are fully guarded by explicit preconditions and invariants.
prompt_fragment: Review all state transition paths and identify missing or bypassable validation checks.
examples:
	- A transition branch updates state without validating current state/version.
	- A fallback branch bypasses checks that are present in the main path.
false_positives:
	- Branches that are unreachable due to upstream exhaustive pattern matching.
references:
	- https://plutus.cardano.intersectmbo.org/
tags:
	- state-machine
	- invariants
confidence_hint: medium
---

# When to use

Use this skill when auditing validators or state machines that evolve datum/state across transactions.

# Detection instructions

1. Enumerate every possible transition branch.
2. Verify explicit preconditions for each branch (state shape, signer set, timing/value gates).
3. Check for bypasses where validation exists in one branch but not in another.
4. Confirm invariants are preserved before and after transitions.

# Reporting guidance

Prefer findings with concrete branch/path evidence and explain why a transition can be bypassed or made inconsistent.
