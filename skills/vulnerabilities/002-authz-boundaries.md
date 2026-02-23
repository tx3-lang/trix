---
id: authz-boundaries-002
name: Authorization boundary bypass
severity: high
description: Validate signer and role checks for every sensitive branch.
prompt_fragment: Find code paths where authorization assumptions are implicit or can be bypassed.
examples:
	- Sensitive branch checks datum fields but does not verify signer identity.
	- A role check exists only in one constructor case and not in another.
false_positives:
	- Purely read-only branches that cannot trigger state/value changes.
references:
	- https://plutus.cardano.intersectmbo.org/
tags:
	- authz
	- signers
confidence_hint: medium
---

# When to use

Use this skill for any validator path that can move value, mutate state, or grant privileges.

# Detection instructions

1. List all privileged operations and their entry branches.
2. Verify signer checks and role assertions are explicit in each branch.
3. Identify implicit assumptions (e.g., relying on script purpose without signer validation).
4. Ensure negative paths cannot reach privileged effects.

# Reporting guidance

Include the exact branch/function where authz is missing and a realistic abuse scenario.
