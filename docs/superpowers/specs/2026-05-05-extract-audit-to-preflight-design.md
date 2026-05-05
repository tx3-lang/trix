# Extract `trix audit` to the `preflight` binary

**Date:** 2026-05-05
**Status:** Approved (pending implementation)
**Repos touched:** `tx3-lang/trix`, `tx3-lang/toolchain`
**Repo not touched:** `tx3-lang/preflight` (already at v0.1.0, no work required)

## Context

`trix audit` lives in `src/commands/audit/` (~6.000 lines) and pulls heavy dependencies into the `trix` binary: the full `aiken-lang` compiler, `serde_yaml_ng`, plus embedded vulnerability skills (`skills/vulnerabilities/*.md`) and Aiken prompt templates (`templates/aiken/*.md`) bundled via `include_str!`.

The same code already exists as a standalone binary at [`tx3-lang/preflight`](https://github.com/tx3-lang/preflight) v0.1.0. Preflight is functionally equivalent (and slightly ahead — it has `--main-source`, `validate_anthropic_reasoning_effort` with tests, and full anthropic reasoning-effort wiring including adaptive thinking variants, none of which are in trix's current `feat/aiken-vulnerability-detection` branch).

The goal is to mirror the pattern already used for `tx3c`, `dolos`, and `cshell`: trix invokes the external binary via `Command::new(home::tool_path(name))`, and `tx3up` distributes it.

## Goals

- Remove all audit/Aiken-specific code, skills, and templates from the trix binary.
- Drop `aiken-lang` and `serde_yaml_ng` from trix's `Cargo.toml`.
- Replace `commands::audit` with a thin wrapper that spawns `preflight`.
- Add `preflight` to the `tx3up` toolchain manifests so users get it on `tx3up` refresh.
- Preserve current UX: `trix audit --help`, flags, stdout/stderr behavior, and the `unstable`/`hide` gating.

## Non-goals

- No changes to preflight (it is the source of truth and is up to date).
- No new features in audit. UX changes deferred to a separate spec.
- No revisit of the `unstable` feature gate or `hide = true` decision. Both are kept as today.
- No work on `~/.tx3/default/bin` install layout, `home::tool_path` resolution, or `tx3up` itself.

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│ tx3-lang/trix                                                    │
│                                                                  │
│  cli.rs                                                          │
│   └─ Audit(commands::audit::Args)   (hide = true, marked UNSTABLE)│
│                                                                  │
│  commands/audit.rs   (single file replacing commands/audit/ dir) │
│   └─ run(args, &RootConfig, &ProfileConfig) -> Result<()>        │
│        ├─ #[cfg(feature = "unstable")] → spawn::preflight::run   │
│        └─ otherwise → Err("requires unstable feature")            │
│                                                                  │
│  spawn/preflight.rs   (new sibling of tx3c.rs / dolos.rs)        │
│   └─ run(args, &RootConfig)                                      │
│        Command::new(home::tool_path("preflight"))                │
│          .args(forwarded flags)                                  │
│          .args(["--main-source", &config.protocol.main])          │
│          .status()  → inherits stdin/stdout/stderr               │
└──────────────────────────────────────────────────────────────────┘
                               │ spawn
                               ▼
┌──────────────────────────────────────────────────────────────────┐
│ tx3-lang/preflight  (installed by tx3up)                         │
│   ~/.tx3/default/bin/preflight                                   │
│   or $TX3_PREFLIGHT_PATH if set (per home::tool_path conventions)│
└──────────────────────────────────────────────────────────────────┘
```

The wrapper sits between `cli.rs` and `home::tool_path`. The contract with preflight is: trix forwards every public preflight flag verbatim, plus injects `--main-source` from `RootConfig.protocol.main` (which preflight already accepts as the fallback when `.ak` discovery returns empty).

## Trix-side changes

### Files deleted

```
src/commands/audit/                  (10 .rs files, ~5.993 lines)
  ├── mod.rs                  786
  ├── ast.rs                  462
  ├── model.rs                141
  └── providers/
      ├── mod.rs              124
      ├── anthropic.rs        594
      ├── heuristic.rs         59
      ├── heuristic_detectors.rs  1.919
      ├── openai.rs           854
      ├── scaffold.rs          44
      └── shared.rs          1.010

skills/vulnerabilities/              (3 markdown files; embedded in audit/mod.rs)
  ├── 001-strict-value-equality.md
  ├── 002-missing-address-validation.md
  └── 003-unvalidated-datum.md

templates/aiken/                     (5 markdown files; embedded in providers/shared.rs)
  ├── audit_agent_initial_user_prompt.md
  ├── audit_agent_system_prompt.md
  ├── audit_agent_tool_result_prompt.md
  ├── permission_prompt.md
  └── report.md
```

### Files created

**`src/commands/audit.rs`** — clap `Args` (mirror of preflight's flags) + thin dispatcher that preserves the `unstable` feature gate:

- Mirrors all preflight flags 1:1 EXCEPT `--main-source` (trix derives it from `RootConfig`).
- Keeps `ReadScopeArg` enum identical to today (so the `cli.rs` `Audit` variant signature does not change).
- `run(args, &RootConfig, &ProfileConfig)` keeps the same shape `main.rs` calls today.
- When `unstable` is on: `spawn::preflight::run(args, config)`.
- When `unstable` is off: returns the same miette error as today (`"The audit command is currently unstable and requires the `unstable` feature to be enabled."`).

**`src/spawn/preflight.rs`** — new module following the `spawn::tx3c` / `spawn::dolos` pattern:

- Resolves the binary via `home::tool_path("preflight")` (gives env-var override `TX3_PREFLIGHT_PATH` and the consistent "please run tx3up" error for free).
- Builds `Command::new(...)` and forwards each flag from `Args` to the corresponding preflight CLI flag.
- Injects `--main-source` from `config.protocol.main` (converted via `to_string_lossy()`).
- Uses `.status()` (not `.output()`) so stdin/stdout/stderr are inherited from the parent process. This preserves: the `🧭` progress logs, the experimental warning print, interactive permission prompts, and final summary lines.
- On non-zero exit: `bail!("preflight exited with non-zero status")`.

### Files modified

| File | Change |
|---|---|
| `src/spawn/mod.rs` | Add `pub mod preflight;` |
| `src/cli.rs` | No structural change. `#[command(hide = true)]` and the `(UNSTABLE - ...)` doc comment stay. |
| `src/main.rs` | No change — `audit::run(args, &config, &profile)` signature is preserved. |
| `Cargo.toml` | Remove `aiken-lang = "1.1.21"` and `serde_yaml_ng = "0.10"`. Keep `[features] unstable = []`. |

### Cargo.toml dependencies — verified retention

Other "audit-looking" dependencies stay because they have non-audit usage in the trix codebase:

- `cryptoxide` → used by `home.rs`, `wallet.rs`
- `chrono` → used by `commands/publish.rs`
- `reqwest` → used by `codegen.rs`, `codegen_legacy.rs`, `telemetry/client.rs`
- `tempfile` → used by `codegen.rs`, `codegen_legacy.rs`

Only `aiken-lang` and `serde_yaml_ng` are exclusive to the audit module.

### Tests

The existing `#[cfg(test)] mod tests` blocks under `src/commands/audit/` already exist verbatim in preflight (verified for `mod.rs`, `heuristic_detectors.rs`, `shared.rs`, `anthropic.rs`, `openai.rs`). Nothing needs to be rescued back into trix.

New trix tests:

1. **Compile-time / clap parse smoke test.** Confirms `audit::Args` parses the same flags as before (regression guard for typos in the mirrored definitions).
2. **Spawn integration test (with `assert_cmd`).** Sets `TX3_PREFLIGHT_PATH` to a small bash script that writes its argv to a file and exits 0. Runs `trix audit --provider openai --model foo` from a fixture project. Asserts the recorded argv contains all forwarded flags plus `--main-source <fixture-main>`. Standard pattern for testing spawn wrappers.

## Toolchain manifest changes (`tx3-lang/toolchain`)

A separate PR in a separate repo. Three manifests, one entry each.

**`manifest-stable.json`** — append to `tools` array:

```json
{
  "name": "preflight",
  "description": "Aiken smart contract vulnerability auditor",
  "repo_name": "preflight",
  "repo_owner": "tx3-lang",
  "version": "^0.1"
}
```

**`manifest-beta.json`** — same entry as stable (today both files are byte-identical except for self/header).

**`manifest-nightly.json`** — same entry but `"version": "^0"` (nightly always tracks latest 0.x):

```json
{
  "name": "preflight",
  "description": "Aiken smart contract vulnerability auditor",
  "repo_name": "preflight",
  "repo_owner": "tx3-lang",
  "version": "^0"
}
```

The binary on disk lands as `~/.tx3/default/bin/preflight`, which matches preflight's clap `name = "preflight"` and trix's `home::tool_path("preflight")` lookup.

## Sequencing

Three PRs across two repos in this strict order. The order eliminates any window where a user could update trix and find `preflight` missing.

1. **Toolchain PR.** Add the `preflight` entry to all three manifests in `tx3-lang/toolchain`. Merge to main. Effect: any `tx3up` run starts installing preflight to `~/.tx3/default/bin/preflight`. Risk: zero — adds a standalone binary that nothing currently consumes.
2. **Trix PR.** Strip audit, add wrapper, drop deps. Merge to main of `tx3-lang/trix`. Effect: trix binary shrinks, `commands::audit` now delegates to preflight. Behind `unstable` + `hide`, so end users on stable do not see the change.
3. **Trix release.** Standard `cargo-release` flow (`release.toml` + `cliff.toml` already configured). Tag → cargo-dist publishes binaries → users running `tx3up` get the new trix and already have preflight from step 1.

### Inverse order is unsafe

If trix releases first, a user running `tx3up` between trix release and toolchain merge gets the new trix without preflight. Their `trix audit --provider ...` would fail with `tool preflight not found` (correct error, suggests `tx3up`, but that suggestion would not help until the toolchain PR lands).

### Local verification per step

| Step | Verification |
|---|---|
| Toolchain | Run `tx3up` against the local manifest path (or staging branch); confirm `~/.tx3/default/bin/preflight` exists and is executable. |
| Trix | `cargo build --features unstable` succeeds without `aiken-lang` or `serde_yaml_ng` in the dep graph. Smoke run with `TX3_PREFLIGHT_PATH=/tmp/mock-preflight.sh` script that records argv; assert all flags + `--main-source` are forwarded. |
| Release | From a clean machine: `tx3up` → `trix audit --provider scaffold` end-to-end. Confirm `.tx3/audit/{state.json,vulnerabilities.md,aiken-ast.json}` produced. |

### Rollback

- Trix: revert PR + patch release. Preflight stays installed but unused. No data loss, no user-facing breakage beyond the audit command.
- Toolchain: only revert if also reverting trix. A toolchain-only revert after a trix release pointing to preflight would break `trix audit` for new `tx3up` users.

## Risks

1. **Flag drift between trix and preflight.** Each new flag in preflight requires a manual mirror in trix's `Args`. Mitigation: low rate of change (preflight has ~12 flags), and CI integration test from the testing section catches forwarded-flag mismatches.
2. **`config.protocol.main` resolution.** It is typically a relative path. `Command::new` inherits the parent's cwd by default, so preflight resolves it from the same project root. Confirmed safe; documented as an implementation note.
3. **Transition window.** Mitigated by the toolchain-first sequencing.

## Out of scope (follow-ups)

- Removing `unstable`/`hide` once the feature is declared GA (separate decision, separate spec).
- Reusing `preflight::Args` from trix as a Cargo library dependency (would re-introduce `aiken-lang` transitively; would only become viable after a preflight refactor that splits Args into a dependency-light sub-crate).
- Adding `preflight` to additional release channels or alternative installers.
