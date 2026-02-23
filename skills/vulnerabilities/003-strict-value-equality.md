---
id: strict-value-equality-003
name: Strict value equality on ADA or full Value
severity: high
description: Detect unsatisfiable validator constraints caused by exact equality checks on ADA or complete output values.
prompt_fragment: Read validator scripts and flag strict equality checks on ADA or full output values; treat comparisons using without_lovelace() as acceptable and not strict ADA equality.
examples:
	- output.value == expected_value
	- output.value.lovelace == exact_amount
false_positives:
	- Comparisons using without_lovelace() to ignore ADA component.
	- Checks that enforce minimum lovelace instead of exact equality.
references:
	- https://plutus.cardano.intersectmbo.org/
tags:
	- value
	- lovelace
	- constraints
confidence_hint: medium
---

# When to use

Use this skill whenever validators compare output values or ADA amounts for equality.

# Detection instructions

1. Find equality checks on full values and lovelace amounts.
2. Flag exact equality constraints that can become unsatisfiable due to fees/min-ADA variability.
3. Accept checks using `without_lovelace()` as intentional ADA-agnostic comparisons.
4. Prefer invariants based on lower bounds for ADA, unless a strict invariant is explicitly justified.

# Reporting guidance

Include the equality expression and explain why it can fail in realistic transaction construction.