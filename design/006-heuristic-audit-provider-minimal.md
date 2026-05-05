# Heuristic Audit Provider (No-LLM) — Minimal Milestone 2 Spec

## Status

Draft (spec-first).

This document defines the minimal implementation scope to add a local, deterministic heuristic analysis provider for `trix audit` without using LLMs.

## Goals

- Provide a functioning heuristic analysis engine for common Aiken vulnerability patterns.
- Expose heuristic scanning via the existing `trix audit` CLI flow.
- Reuse current state/report contracts and avoid breaking compatibility.
- Keep implementation minimal and focused on Milestone 2 outputs.

## Scope

In-scope:
- New `heuristic` provider in the existing provider selector.
- Rule-based detection for the 3 currently embedded vulnerability skills:
  - `strict-value-equality-001`
  - `missing-address-validation-002`
  - `unvalidated-datum-003`
- Deterministic, local-only analysis (no network calls, no LLM/tool loop).
- Continued use of current output files:
  - `.tx3/audit/state.json`
  - `.tx3/audit/vulnerabilities.md`

Out-of-scope:
- Generic interpretation of arbitrary custom skills.
- Replacing existing LLM providers (`openai`, `anthropic`, `ollama`).
- New output formats or schema changes.
- Type-checked semantic analysis beyond untyped AST (future work).

## Current Architecture Anchors

- Audit orchestration and skill loop: `src/commands/audit/mod.rs`
- Provider abstraction and factory: `src/commands/audit/providers/mod.rs`
- Heuristic provider adapter: `src/commands/audit/providers/heuristic.rs`
- Heuristic detector engine (AST-first): `src/commands/audit/providers/heuristic_detectors.rs`
- AST/cache and validator context: `src/commands/audit/ast.rs`
- Analysis/report data contracts: `src/commands/audit/model.rs`
- Existing seed skills:
  - `skills/vulnerabilities/001-strict-value-equality.md`
  - `skills/vulnerabilities/002-missing-address-validation.md`
  - `skills/vulnerabilities/003-unvalidated-datum.md`

## CLI Contract Delta

`--provider` MUST accept `heuristic`.

Defaults remain unchanged:
- Default provider stays `scaffold`.
- `heuristic` does not require `--endpoint`, `--model`, or `--api-key-env`.

## High-Level Execution Flow (heuristic mode)

1. Build provider from CLI args (`heuristic`).
2. Discover source files and load/reuse AST cache.
3. Load vulnerability skills from `--skills-dir` (or embedded seeds fallback).
4. For each skill, run deterministic local rule evaluation.
5. Persist incremental state after each skill.
6. Render final report with existing markdown template.

## Heuristic Provider Requirements

### Functional requirements

- MUST implement `AnalysisProvider` and return `SkillIterationResult` for each skill.
- MUST run without network/API keys.
- MUST be deterministic in findings ordering and status values.
- MUST support only the 3 known embedded skill IDs in this milestone.
- MUST continue processing when a skill is not supported.

### Unsupported skills

If a skill ID is not supported by the heuristic provider:
- `status` MUST be `unsupported-skill`.
- `findings` MUST be empty.
- `next_prompt` MUST be `None`.
- Audit execution MUST continue.

## Detection Strategy (M2 minimal)

The provider uses an **AST-first** approach:
- Parse each `.ak` source into Aiken `UntypedModule` (`aiken_lang` parser).
- Traverse validator handlers/fallback expressions and patterns (`UntypedExpr`, `UntypedPattern`).
- Apply deterministic rule checks from AST structure and operators.
- Use text matching only as fallback when AST parsing fails for a file.

This keeps detection deterministic, local-only, and less fragile than string-only scanning.

### Rule 1: strict-value-equality-001

Report when AST `BinOp::Eq` compares expressions that include ADA/value signals.

Do NOT report when clear safe patterns are detected, e.g.:
- `without_lovelace(...)`
- minimum checks (`>=`) for lovelace/value constraints

### Rule 2: missing-address-validation-002

Report when AST patterns extract script credentials from output addresses (e.g. `Script(hash_var)`) but no later equality/inequality validation references that extracted variable.

Do NOT report when explicit address checks are present.

### Rule 3: unvalidated-datum-003

Report when inline datum is extracted from output (e.g. `InlineDatum(x)`) but is not semantically validated, or is validated only partially (e.g. spread pattern `Datum { ..., .. }`).

Do NOT report when evidence suggests complete datum extraction/validation.

## Data Contract Compatibility

- `AnalysisStateJson` schema remains unchanged.
- `VulnerabilityFinding` schema remains unchanged.
- Report rendering remains unchanged.
- Provider metadata SHOULD identify `heuristic` clearly in `state.json`.

## Caching / Memory Requirements

- The provider MUST reuse AST/context built by existing audit flow.
- Existing AST cache in `.tx3/audit/aiken-ast.json` remains the inter-run memory mechanism.
- `--no-ast-cache` MUST still force regeneration.
- Heuristic rule execution MUST be AST-first even when cache is present (parsing source modules directly for rule traversal).

## Security and Isolation

- No outbound requests.
- No AI tool-loop execution path.
- Only local workspace file reads under existing audit orchestration.

## Acceptance Criteria (Milestone 2 minimal)

- A1: `trix audit --provider heuristic` produces a structured vulnerability report.
- B1: Rule behavior is consistent with the 3 public skill definition files.
- C1: Users can execute heuristic scans locally end-to-end from CLI.
- D1: Running against known vulnerable scripts yields non-zero findings.

## Testing Plan

- Unit tests for each heuristic rule:
  - positive and negative scenarios
  - unsupported-skill behavior
- E2E audit test for `--provider heuristic` in initialized project.
- Keep existing audit smoke/edge coverage passing.

## Requirement-to-Test Traceability (initial)

- Provider selection supports `heuristic` → audit provider validation tests.
- End-to-end execution and artifacts → `tests/e2e/happy_path.rs`.
- Unsupported skill non-fatal handling → heuristic provider unit test.
- Contract compatibility (`state.json`, report rendering) → existing audit happy-path assertions + heuristic additions.

## Open Questions (deferred)

- Should heuristic become default provider in a later milestone?
- Should custom external skills be supported beyond known IDs?
- Should future versions parse semantic expressions from typed AST for lower false positives?
