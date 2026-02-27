# Audit Command Implementation Spec

## Status

This document captures the **currently implemented behavior** of `trix audit` as an implementation-spec companion to [003-ai-aiken-vulnerability-scaffolding.md](003-ai-aiken-vulnerability-scaffolding.md).

## Scope

In-scope:
- Full CLI contract currently accepted by `trix audit`
- Runtime behavior of the skill loop
- State/report output contracts as implemented
- Provider behavior (`scaffold`, `openai`, `anthropic`, `ollama`)
- Local read-tool permission and scope enforcement
- Current test-backed acceptance behavior

Out-of-scope:
- Future UX redesigns
- Non-Aiken source language support

## Command Surface

`trix audit` is a **scoped command** (requires `trix.toml` in cwd).

It is **hidden + unstable-gated**:
- Hidden in clap command listing (`#[command(hide = true)]`)
- Returns an error unless compiled with `--features unstable`

### CLI Arguments (current)

```bash
trix audit \
  [--state-out <path>] \
  [--report-out <path>] \
  [--skills-dir <path>] \
  [--provider <scaffold|openai|anthropic|ollama>] \
  [--endpoint <url>] \
  [--model <name>] \
  [--api-key-env <ENV_VAR>] \
  [--ai-logs] \
  [--read-scope <workspace|strict>] \
  [--interactive-permissions]
```

Defaults:
- `--state-out`: `.tx3/audit/state.json`
- `--report-out`: `.tx3/audit/vulnerabilities.md`
- `--skills-dir`: `skills/vulnerabilities`
- `--provider`: `scaffold`
- `--read-scope`: `workspace`
- `--ai-logs`: `false`
- `--interactive-permissions`: `false`

### Provider arguments (required behavior)

The following arguments are interpreted with provider-specific defaults:

- `--provider`
  - Supported values: `scaffold`, `openai`, `anthropic`, `ollama`
  - Any other value must fail with an unsupported provider error

- `--endpoint`
  - Optional override for provider API URL
  - Default when omitted:
    - `openai`: `https://api.openai.com/v1/chat/completions`
    - `anthropic`: `https://api.anthropic.com/v1/messages`
    - `ollama`: `http://localhost:11434/v1/chat/completions`
    - `scaffold`: not used

- `--model`
  - Optional model override
  - Default when omitted:
    - `openai`: `gpt-4.1-mini`
    - `anthropic`: `claude-3-5-haiku-latest`
    - `ollama`: `llama3.1`
    - `scaffold`: not used

- `--api-key-env`
  - Optional environment-variable name override for API credentials
  - Default when omitted:
    - `openai`: `OPENAI_API_KEY`
    - `anthropic`: `ANTHROPIC_API_KEY`
    - `ollama`: not required (fixed placeholder token is used)
    - `scaffold`: not required
  - Runtime behavior:
    - `openai` and `anthropic` must fail early if the resolved env var is not set
    - `ollama` does not read env credentials and uses `ollama` as a fixed API key string

- `--ai-logs`
  - When enabled, prints iterative model/tool progress logs to stderr
  - Logs include step counts, requested local actions, and (truncated) model/tool output

Examples:

```bash
# OpenAI with defaults
trix audit --provider openai

# OpenAI with endpoint/model/api key env overrides
trix audit --provider openai \
  --endpoint https://example.com/v1/chat/completions \
  --model gpt-4.1 \
  --api-key-env MY_OPENAI_KEY

# Anthropic default endpoint + model
trix audit --provider anthropic

# Ollama local runtime
trix audit --provider ollama --ai-logs
```

## High-Level Execution Flow

1. Build provider from args.
2. Determine `project_root = current_dir`.
3. Discover source files recursively under project root:
   - Include: `*.ak`
   - Skip directories: `.git`, `target`, `.tx3`, `build`
4. If no `.ak` files were found, fallback to `config.protocol.main` as a source reference.
5. Build `PermissionPromptSpec` based on `read_scope` and `interactive_permissions`.
6. Load skills from `--skills-dir`.
   - If directory is missing and arg is default `skills/vulnerabilities`, load embedded seed skills.
   - If directory is missing and arg is custom, fail.
7. Initialize `AnalysisStateJson` with empty iterations and write it immediately.
8. For each skill in sorted order:
   - Compose mini-prompt from skill metadata/body.
   - Call provider `analyze_skill(...)`.
   - Append iteration to state.
   - Persist full state JSON after each skill.
9. Build aggregated report from all findings.
10. Render markdown via template and write report file.
11. Print completion summary to stdout.

## Data Contracts (Implemented)

Defined in `src/commands/audit/model.rs`.

### `VulnerabilitySkill`
Required semantic fields:
- `id`, `name`, `severity`, `description`, `prompt_fragment`

Optional/collection fields (default empty if missing):
- `examples`, `false_positives`, `references`, `tags`
- `confidence_hint` optional string
- `guidance_markdown` from markdown body (post-frontmatter)

### `AnalysisStateJson`
```json
{
  "version": "1",
  "source_files": ["..."],
  "provider": {
    "name": "...",
    "model": "... or null",
    "notes": "..."
  },
  "permission_prompt": {
    "shell": "bash",
    "allowed_commands": ["grep", "cat", "find", "ls"],
    "scope_rules": ["..."],
    "read_scope": "workspace|strict",
    "interactive_permissions": false,
    "allowed_paths": ["..."]
  },
  "iterations": [
    {
      "skill_id": "...",
      "status": "completed|scaffolded|...",
      "findings": [
        {
          "title": "...",
          "severity": "...",
          "summary": "...",
          "evidence": ["..."],
          "recommendation": "...",
          "file": "optional",
          "line": 42
        }
      ],
      "next_prompt": {
        "skill_id": "...",
        "text": "..."
      }
    }
  ]
}
```

### `VulnerabilityReportSpec`
- `title`
- `generated_at` (UTC RFC3339)
- `findings` (flattened from all iterations)

## Skill File Contract (Implemented Parser)

Each skill file must be markdown with YAML frontmatter delimited by `---`.

Rules:
- Missing frontmatter delimiters => error
- Unknown frontmatter fields => error (`deny_unknown_fields`)
- Required string fields must be non-empty after trim
- `severity` must be one of: `low|medium|high|critical` (case-normalized to lowercase)
- Tabs in frontmatter are normalized to two spaces before YAML parse
- Markdown body after frontmatter is stored in `guidance_markdown`

## Prompt Construction

Per skill, a mini-prompt is composed from:
- `Skill ID`
- `Name`
- `Severity`
- `Description`
- `Prompt Fragment`
- Optional sections for tags/hint/examples/false positives/references/guidance markdown

Provider initial prompt includes:
- Mini-prompt text
- Referenced source files list
- Allowed commands + scope rules from `PermissionPromptSpec`

## Permission Model and Local Tooling

Allowed tool actions requested by model:
- `read_file`
- `grep`
- `list_dir`
- `find_files`
- `final`

Mapped local commands:
- `read_file` -> `cat`
- `grep` -> `grep -n -C <N> -- <pattern> <path>`
- `list_dir` -> `ls -la <path>`
- `find_files` -> `find <path> -type f [-name <glob>]`

Global safeguards:
- Requested path must canonicalize successfully
- Canonical path must remain under project root
- Command must be in `allowed_commands`
- Output truncation at 30,000 chars

### Read scope modes

`workspace`:
- Reads/searches over any path under project root

`strict`:
- Denies `list_dir` and `find_files`
- Allows reads/searches only on regular files listed in `permission_prompt.allowed_paths`
- `allowed_paths` is populated from discovered source files (displayed relative paths)

### Interactive permissions

If enabled:
- Each local read request prompts `Allow this request? [y/N]:`
- Non-yes response denies request with an explicit error

## Providers (Current)

### `scaffold`
- No network calls
- Returns one iteration with:
  - `status = scaffolded`
  - empty findings
  - placeholder `next_prompt`

### `openai`
- Provider spec:
  - `name = openai-compatible`
  - `notes = Endpoint: <endpoint>`
- Defaults:
  - endpoint: `https://api.openai.com/v1/chat/completions`
  - model: `gpt-4.1-mini`
  - api key env: `OPENAI_API_KEY`
- Request shape:
  - `model`, `messages`, `response_format: { type: json_object }`
  - auth: Bearer API key
- Response extraction:
  - `/choices/0/message/content` (string JSON)
- Iterative loop:
  - max 25 steps (`MAX_AGENT_STEPS`)
  - parse model output as action (`read request` or `final`)
  - execute local read request and feed output back as user message

### `anthropic`
- Provider spec:
  - `name = anthropic`
  - `notes = Endpoint: <endpoint>`
- Defaults:
  - endpoint: `https://api.anthropic.com/v1/messages`
  - model: `claude-3-5-haiku-latest`
  - api key env: `ANTHROPIC_API_KEY`
  - version header: `2023-06-01`
- Request shape:
  - `model`, `max_tokens`, `system`, `messages`
  - headers: `x-api-key`, `anthropic-version`
- Response extraction:
  - `/content/0/text` (string JSON)
- Same 25-step interactive read loop as `openai`

### `ollama`
- Implemented via `OpenAiProvider` compatibility
- Defaults:
  - endpoint: `http://localhost:11434/v1/chat/completions`
  - model: `llama3.1`
  - api key literal: `ollama`

## Parsing of AI Output

Accepted model output forms:
- Raw JSON object
- JSON inside fenced blocks (```json ... ``` or ``` ... ```)

Action interpretation:
- If `action` missing but payload has `findings` or `status` => treated as `final`
- `final` payload is converted into `SkillIterationResult`
- `findings[*].line` can be number or numeric string
- Also supports nested fallback location fields:
  - `location.file`
  - `location.line`

Defaults when missing:
- iteration status: `completed`
- finding title: `Untitled finding`
- finding severity: skill severity
- other finding text fields default to empty string

## Output Rendering

Report template: `templates/aiken/report.md`

Findings markdown rendering:
- Empty findings => `- *(none)*`
- Per finding include title, severity, summary, recommendation
- Include `Location` line when `file` and/or `line` available

Permission template file exists (`templates/aiken/permission_prompt.md`) but current runtime behavior constructs prompt data directly from `PermissionPromptSpec` and does not render this template for provider calls.

## Embedded Seed Skills

When using default `--skills-dir` and path is absent, embedded content is loaded from:
- `skills/vulnerabilities/001-strict-value-equality.md`

## Current Acceptance Signals (Tests)

E2E tests assert:
- `audit --help` works with unstable feature
- `audit` fails without `trix.toml` (scoped command requirement)
- `audit` fails for missing custom skills dir
- `audit` succeeds after `init --yes`
- Outputs are created:
  - `.tx3/audit/state.json`
  - `.tx3/audit/vulnerabilities.md`
- State contract basics:
  - `version == "1"`
  - `iterations.len() == 3` for seed skills

Unit tests assert:
- Skill parser behavior and validation errors
- Source discovery recursion and ignored directories
- Strict read scope allows known file and rejects directory listing
- Report markdown includes location formatting

## Specification Evolution Notes

The following items represent milestone evolution from initial scaffolding to current implementation:

1. **Real provider integrations now exist** (`openai`, `anthropic`, `ollama`), not contract-only.
2. **Interactive read tool loop is implemented** with bounded local command execution.
3. **Additional CLI controls exist** (`endpoint`, `model`, `api_key_env`, `ai_logs`, `read_scope`, `interactive_permissions`).
4. **Strict/workspace read scopes are enforced in code**.
5. **Seed skill fallback is embedded** when default skills directory is not found.
6. **Permission prompt template is currently not part of runtime rendering path**.

## Spec-Driven Viability Assessment

Using this document for spec-driven development of the current `audit` behavior is **viable**.

This section upgrades the contract into strict spec-first form via:
- normative requirement levels (`MUST`/`SHOULD`)
- requirement-to-test traceability
- canonical golden fixtures

## Normative Requirements

### MUST (behavior compatibility)

- Same CLI flags, defaults, and unstable gating behavior.
- Same provider selection and provider-specific defaults/env handling.
- Same `.ak` discovery semantics and skipped directories.
- Same skills parsing rules (frontmatter, required fields, severity enum, unknown-field rejection).
- Same iterative per-skill persistence to state JSON.
- Same read-request action schema and local command mapping.
- Same path confinement and strict/workspace enforcement.
- Same max step guard (`25`) and command output truncation (`30_000` chars).
- Same report generation shape and findings rendering.
- Same seed-skill fallback behavior and baseline test outcomes.

### SHOULD (implementation quality)

- Keep provider/network and local-tooling boundaries separated behind provider adapter interfaces.
- Preserve deterministic ordering where current implementation sorts inputs/paths.
- Preserve error messages close to current wording when feasible, to reduce e2e churn.
- Keep state/report writes atomic at logical checkpoints (initial state + post-iteration).

## Requirement-to-Test Traceability

| Requirement | Test anchors |
|---|---|
| CLI visibility and unstable behavior | `tests/e2e/smoke.rs::audit_help_runs_without_error`, `tests/e2e/smoke.rs::audit_help_displays_provider_options` |
| Scoped command requirement (`trix.toml`) | `tests/e2e/edge_cases.rs::aiken_audit_fails_without_trix_config` |
| Missing custom skills dir failure | `tests/e2e/edge_cases.rs::aiken_audit_fails_with_missing_skills_dir` |
| Baseline success path + output artifacts | `tests/e2e/happy_path.rs::aiken_audit_runs_in_initialized_project` |
| State shape baseline (`version`, seed iterations) | `tests/e2e/happy_path.rs::aiken_audit_runs_in_initialized_project` |
| Skill parser frontmatter/body behavior | `src/commands/audit/mod.rs::parse_skill_content_reads_frontmatter_and_guidance` |
| Skill parser validation failures | `src/commands/audit/mod.rs::parse_skill_content_requires_frontmatter`, `src/commands/audit/mod.rs::parse_skill_content_rejects_invalid_severity` |
| Source discovery recursion and filtering | `src/commands/audit/mod.rs::discover_source_files_finds_ak_files_recursively`, `src/commands/audit/mod.rs::discover_source_files_skips_target_tx3_and_build_dirs` |
| Strict read-scope allows known file | `src/commands/audit/providers/shared.rs::execute_read_request_strict_allows_known_file` |
| Strict read-scope denies directory listing | `src/commands/audit/providers/shared.rs::execute_read_request_strict_rejects_list_dir` |
| Report location rendering contract | `src/commands/audit/mod.rs::render_findings_markdown_includes_location_when_available` |
