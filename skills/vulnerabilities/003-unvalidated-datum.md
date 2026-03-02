---
id: unvalidated-datum-003
name: unvalidated-datum
severity: high
description: Detects missing validation of datum when creating or validating outputs at script addresses.
prompt_fragment: Detect outputs at script addresses where datum is missing, partially validated, or extracted but unused.
---

# unvalidated-datum

Validators must validate the datum of outputs created or continued at script addresses.

If a validator creates or enforces an output to a script address but does not validate its datum, attackers may:

- Inject arbitrary datum data
- Corrupt protocol state
- Create unspendable UTxOs
- Break protocol invariants

---

## Detection Logic

Report if some of the following hold:

1. The validator DOES NOT check on EVERY FIELD of the OUTPUT datum (first must check what is the type of the datum and what fields it has, if it is a structured datum, and then check that all fields are checked). For example:
   - The validator only checks a subset of the datum fields
   - The validator only checks the datum type but not its fields

2. The validator DOES NOT:
   - Extract the datum from the output
   - Decode it into the expected datum type
   - Validate it (whether by checking equality on fields or using them in other functions)
   - Or enforce equality with an expected datum

3. The datum is extracted but discarded as wildcard match, or the datum is extracted but not used at all in the validation

---

## Do NOT Report If

- The output datum is extracted, decoded, and fully validated
- The output is to a pubkey address
- The output carries no datum by design

---

## Description

Explain:

- Which output lacks validation and what is the datum schema expected for that output
- If there are certain fields of the datum that are not validated, explain which ones and what is the expected value for those fields