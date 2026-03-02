---
id: missing-address-validation-002
name: missing-address-validation
severity: high
description: Detects validators and minting policies that fail to explicitly validate output destination addresses.
prompt_fragment: Identify output-selection logic that validates datum/token/value but never validates destination address.
---

# missing-address-validation

Validators and minting policies must explicitly validate the destination address of:

- Continuing outputs
- Newly created outputs
- Outputs receiving newly minted tokens

Selecting outputs only by:

- Datum presence
- Token presence
- Value shape

without checking the destination address can allow attackers to redirect funds or protocol state to unintended addresses.

---

## Detection Scope

Focus on logic that:

- Filters or selects outputs
- Asserts the existence of a “continuing” or “target” output
- Checks token presence, datum presence, or value constraints

---

## Detection Logic

Report a vulnerability if **ALL** of the following hold:

1. The validator or policy:
   - Selects one or more outputs (`transaction.outputs`)
   - Or asserts existence of a continuing output

2. Output selection or validation is based on some of the following:
   - Datum presence (`output.datum`)
   - Token presence
   - Value shape
   - Minted value containment
   - Reference script presence

3. **No explicit validation** of the output address is performed, such as:
   - `output.address == expected_address`
   - Matching on `ScriptCredential` / `PubKeyCredential`

4. No indirect address validation is performed via:
   - Comparison with input address
   - Comparison with a known script address

---

### Description

Explain:

- How outputs are selected
- That destination address is never checked
- How an attacker can redirect funds or state