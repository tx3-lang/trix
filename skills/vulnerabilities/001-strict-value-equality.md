---
id: strict-value-equality-001
name: strict-value-equality
severity: high
description: Vulnerabilities related to strict value equality in the protocol.
prompt_fragment: Read validator scripts and flag strict equality checks on ADA or full output values; treat comparisons using without_lovelace() as acceptable and not strict ADA equality.
confidence_hint: medium
---

# strict-value-equality

Validators could become unsatisfiable when enforcing exact equality on ADA or full output values.
Exact value equality is almost always incorrect for ADA in Plutus V2. Validators should enforce minimums, not exact amounts, unless there is a very strong invariant requiring exact equality.

## When to use

Every time a search for vulnerabilities related to strict value equality in the protocol is explicitly requested by the user.

## Instructions

1. Read the validator scripts of the protocol and identify any instances where strict value equality is enforced on ADA or full output values.
2. Take into account that values compared using `without_lovelace()` are not considered strict equality, as they ignore the ADA component.

## Reporting guidance

- Include the exact equality expression and where it appears.
- Explain why it can make the validator unsatisfiable in realistic transaction construction.
- Recommend replacing strict equality on ADA with a minimum-bound check when possible.