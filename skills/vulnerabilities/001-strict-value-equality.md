---
id: strict-value-equality-001
name: strict-value-equality
severity: high
description: Detects vulnerabilities caused by enforcing exact equality on ADA or full output values in Aiken validators.
prompt_fragment: Read validator scripts and flag strict equality checks on ADA or full output values; treat comparisons using without_lovelace() as acceptable and not strict ADA equality.
confidence_hint: medium
---

# strict-value-equality

Validators may become unsatisfiable when enforcing exact equality on ADA or full output values.
Exact value equality is almost always incorrect for ADA.

Strict equality can break due to:

- minUTxO changes
- Datum size changes
- Reference script additions
- Token additions that increase UTxO size

Validators should enforce minimum values rather than exact equality, unless there is a strong invariant requiring exact equality.

---

## When to use

Every time a search for vulnerabilities related to strict value equality is required.

This skill MUST be applied only to on-chain code when value comparisons are detected.

---

## Detection Logic

Identify cases where **strict equality** is enforced on:

### 1. ADA values

Examples:

- Comparing lovelace amounts using exact equality:
  - `lovelace_of(tx_out.value) == expectedLovelace`
  - `tx_out.value == from_lovelace(expectedLovelace)`

### 2. Full output values

- Exact equality between `Value` objects:
  - `tx_out.value == expectedValue`

---

## Allowed / Safe Cases (Do NOT report)

Do NOT report findings if:

- ADA is explicitly excluded from the comparison:
  - `value.without_lovelace() == expectedValue`
- Comparison enforces a **minimum**:
  - `lovelaceValueOf(tx_out.value) >= minLovelace`

If a strong invariant is present, do NOT report unless it is violated or undocumented.

---

## Description

Explain:

- Where the strict equality is enforced
- Why it can cause the validator to become unsatisfiable
- Under which ledger conditions this may fail (minUTxO, datum growth, etc.)