# Aiken AST & Validator Context for Audit

## Status

Proposed implementation spec for extending `trix audit` with:
- **Phase 1**: on-demand Aiken AST generation
- **Phase 2**: `ValidatorContextMap` extraction from AST

---

## Goals

1. Ensure `trix audit` can obtain a **fresh structural view** of Aiken code without relying on pre-existing artifacts.
2. Build a deterministic `ValidatorContextMap` that can be injected into audit prompts.
3. Persist enough metadata in state to make runs reproducible and diagnosable.

---

## Scope

- New AST generation flow in `audit` execution path.
- New model contract for validator context.
- Prompt template/data-path extension to include validator context.
- State JSON extension to include AST/context metadata.
- Failure semantics for AST generation/parsing.
- Unit/e2e acceptance coverage for phase behavior.

---

## High-Level Flow (Phase 1 + 2)

Before skill loop execution:

1. Discover `.ak` source files (existing behavior).
2. Generate Aiken AST on-demand (new behavior).
3. Parse AST into normalized internal structures.
4. Build `ValidatorContextMap` (validator-centric mapping).
5. Add this context to:
   - initial prompt rendering payload
   - persisted analysis state
6. Run existing skill loop unchanged, except prompts now include validator context block.

---

## CLI Surface Changes

No mandatory user-facing flags are required for baseline phase 1–2.

Optional (recommended) additions:
- `--ast-out <path>` (default: `.tx3/audit/aiken-ast.json`)
- `--no-ast-cache` (default: false)

If optional flags are deferred, runtime should still write AST snapshot to default path.

---

## Data Contracts

## `AnalysisStateJson` extension

Add fields:

```json
{
  "ast": {
    "path": ".tx3/audit/aiken-ast.json",
    "fingerprint": "sha256:...",
    "generated_at": "2026-02-26T12:00:00Z",
    "tool": {
      "name": "aiken",
      "version": "vX.Y.Z"
    }
  },
  "validator_context": {
    "validators": [ ... ]
  }
}
```

### `AstMetadata`

- `path`: persisted AST snapshot path (workspace-relative in state)
- `fingerprint`: deterministic digest of AST content (or source-set digest)
- `generated_at`: RFC3339 UTC timestamp
- `tool.name`: fixed string `aiken`
- `tool.version`: resolved from CLI runtime

### `ValidatorContextMap`

```json
{
  "validators": [
    {
      "id": "vesting.hello_world",
      "module": "validators/vesting.ak",
      "source_file": "onchain/validators/vesting.ak",
      "source_span": {
        "start_line": 13,
        "end_line": 31
      },
      "handlers": [
        {
          "name": "spend",
          "parameters": [
            { "name": "datum", "type": "Option<Datum>" },
            { "name": "redeemer", "type": "Redeemer" },
            { "name": "_own_ref", "type": "OutputReference" },
            { "name": "self", "type": "Transaction" }
          ]
        },
        {
          "name": "else",
          "parameters": [
            { "name": "_", "type": "Unknown" }
          ]
        }
      ]
    }
  ]
}
```

Normalization rules:
- `validators` MUST be sorted deterministically by `id` then `source_file`.
- `handlers` MUST preserve source order when available.
- `parameters` MUST preserve declared order.
- If precise type text is unavailable, set type to `"Unknown"` (do not omit parameter).
- If source span is unavailable, omit `source_span`.

---

## AST Generation Contract (Phase 1)

`audit` MUST execute an on-demand AST generation step before skill analysis.

Requirements:
- MUST run within current project root.
- MUST fail the audit run if AST generation fails.
- MUST persist raw AST output to `.tx3/audit/aiken-ast.json` (or configured path).
- MUST record Aiken tool version in state metadata.
- SHOULD avoid repeated generation in same run once AST is available.

Failure behavior:
- Return explicit error category:
  - Aiken CLI missing
  - Aiken command failed
  - AST output unreadable/invalid JSON

No fallback behavior is defined in this phase.

---

## Validator Context Extraction (Phase 2)

Parser must transform AST into `ValidatorContextMap`.

Extraction requirements:
- MUST enumerate all validator definitions in analyzed source set.
- MUST extract handler names and ordered parameter lists.
- MUST include best-effort type display for each parameter.
- MUST include source file path linkage for each validator.
- SHOULD include source spans when present in AST.

Validation requirements:
- If AST is valid but yields no validators, run continues with empty validator list.
- If AST schema is incompatible, fail with parse-contract error.

---

## Prompt Integration

Template update target:
- `templates/aiken/audit_agent_initial_user_prompt.md`

Add new section after source references:

```markdown
Validator context map:
{{VALIDATOR_CONTEXT_MAP}}
```

Rendering rules:
- Use concise markdown bullets (not raw JSON dump) for readability.
- Include:
  - validator id
  - source file
  - handlers and parameter signatures
- If empty: render `- (none)`.

Provider integration:
- Existing providers (`openai`, `anthropic`, `ollama`, `scaffold`) receive the same expanded prompt content via shared builder.

---

## Implementation Notes (Code Placement)

Likely code touchpoints:
- `src/commands/audit/mod.rs`
  - orchestration: AST generation + context extraction prior to skill loop
  - state population
- `src/commands/audit/model.rs`
  - add `AstMetadata`, `ValidatorContextMap`, related structs
- `src/commands/audit/providers/shared.rs`
  - extend `build_initial_user_prompt(...)`
  - renderer for validator context markdown block
- `templates/aiken/audit_agent_initial_user_prompt.md`
  - add `{{VALIDATOR_CONTEXT_MAP}}` placeholder

Recommended internal modules:
- `src/commands/audit/ast.rs`
  - command execution + AST load
  - schema adapter/parser into internal normalized models

---

## Determinism & Caching

Minimum deterministic guarantees:
- Stable sort ordering for validator map.
- Stable markdown rendering order.
- State includes fingerprint for traceability.

Caching (optional in phase 1–2, but recommended):
- Reuse AST file if fingerprint of relevant sources unchanged.
- `--no-ast-cache` bypasses reuse.

---

## Security & Permissions

- AST generation is local and non-interactive.
- No additional AI read permissions are introduced by this phase.
- Generated AST artifact remains inside project `.tx3/` output scope.

---

## Acceptance Criteria

Phase 1 accepted when:
- `trix audit` generates AST snapshot on each run (or cache-hit behavior if enabled).
- Run fails clearly when Aiken CLI/AST generation fails.
- State JSON includes AST metadata block.

Phase 2 accepted when:
- Validator context map is extracted and persisted in state.
- Initial provider prompt includes rendered validator context map.
- Map includes validator handlers and ordered parameter signatures.
- Deterministic ordering verified by tests.

---

## Testing Plan

Unit tests:
- AST parse adapter:
  - parses validators/handlers/parameters
  - handles missing type info with `Unknown`
  - deterministic sorting
- Prompt renderer:
  - renders non-empty context map
  - renders `- (none)` for empty map

Integration/e2e tests:
- `audit` produces `.tx3/audit/aiken-ast.json`.
- `state.json` contains `ast` and `validator_context` blocks.
- Prompt-building path includes `Validator context map:` section.

Negative tests:
- Missing Aiken binary => explicit failure.
- Invalid AST JSON => explicit failure.

---

## Open Questions

1. Which exact Aiken command/output format is canonical for AST export in current supported versions?
2. Should type rendering preserve Aiken syntax verbatim or use normalized aliases?
3. Should `source_span` include columns now or lines only?

These questions must be resolved before implementation starts, but do not change the phase scope.
