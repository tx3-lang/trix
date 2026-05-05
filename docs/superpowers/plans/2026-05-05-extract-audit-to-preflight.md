# Extract `trix audit` to `preflight` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the in-tree `trix audit` implementation with a thin spawn wrapper that delegates to the standalone `preflight` binary, drop the heavy `aiken-lang` and `serde_yaml_ng` dependencies, and add `preflight` to the `tx3up` toolchain manifests so users get the binary on their next refresh.

**Architecture:** `commands::audit::run` becomes a thin dispatcher behind the existing `unstable` feature gate; when enabled it calls a new `spawn::preflight::run` that resolves the binary via `home::tool_path("preflight")` and forwards every preflight CLI flag plus injects `--main-source` from `RootConfig.protocol.main`. Pattern mirrors `spawn::tx3c` / `spawn::dolos` / `spawn::cshell`.

**Tech Stack:** Rust 2024 edition, `clap` v4 derive macros, `miette` for diagnostics, `assert_cmd` + `tempfile` for integration tests, `std::process::Command` for spawning.

**Repos involved:**
- `/Users/mduthey/Documents/Work/txpipe/tx3/trix` (this repo, on branch `feat/aiken-vulnerability-detection`)
- `/Users/mduthey/Documents/Work/txpipe/tx3/toolchain` (separate repo, `tx3-lang/toolchain`)

**Spec:** `docs/superpowers/specs/2026-05-05-extract-audit-to-preflight-design.md`

---

## File Structure

### Files created

- `src/commands/audit.rs` — flat file replacing the `src/commands/audit/` directory. Owns the clap `Args` struct (mirror of preflight's flags) and the `unstable` feature gate. Single responsibility: CLI surface for `trix audit`.
- `src/spawn/preflight.rs` — sibling of `spawn::tx3c`/`spawn::dolos`. Single responsibility: build the `Command::new(home::tool_path("preflight"))` invocation, forward flags, inject `--main-source`.
- `tests/e2e/audit_wrapper.rs` (new module under e2e) — integration test for the spawn forwarding contract using a mock preflight script. Single responsibility: verify the wrapper sends the right argv.

### Files modified

- `src/spawn/mod.rs` — add `pub mod preflight;`
- `Cargo.toml` — remove `aiken-lang` and `serde_yaml_ng` from `[dependencies]`
- `tests/e2e/mod.rs` — register the new `audit_wrapper` test module
- `tests/e2e/happy_path.rs` — remove `aiken_audit_runs_in_initialized_project` and `aiken_audit_runs_with_heuristic_provider` (depend on internal `trix::commands::audit::model::*` types that no longer exist; replaced by the new spawn wrapper test plus preflight's own E2E tests)
- `tests/e2e/edge_cases.rs` — remove `aiken_audit_fails_with_missing_skills_dir` (asserts on a preflight-side error message we no longer own)

### Files deleted

- `src/commands/audit/` (entire directory: `mod.rs`, `ast.rs`, `model.rs`, and all of `providers/`)
- `skills/vulnerabilities/` (3 markdown files embedded by `audit/mod.rs` via `include_str!`)
- `templates/aiken/` (5 markdown files embedded by `audit/providers/shared.rs` and `audit/mod.rs` via `include_str!`)
- `tests/fixtures/audit/` (currently empty placeholder)

### Files unchanged (verified)

- `src/cli.rs` — `Audit(commands::audit::Args)` variant signature is preserved (Args struct keeps the same name and ReadScopeArg enum)
- `src/main.rs` — `audit::run(args, &config, &profile)` signature is preserved
- `src/commands/mod.rs` — `pub mod audit;` resolves to either the dir or the file; no edit needed
- All other commands and their dependencies

---

## Phase 1: Toolchain manifest update

This phase happens in a different repo (`tx3-lang/toolchain` at `/Users/mduthey/Documents/Work/txpipe/tx3/toolchain`) and must be merged before Phase 2's release reaches users.

### Task 1: Add `preflight` to all three toolchain manifests

**Files:**
- Modify: `/Users/mduthey/Documents/Work/txpipe/tx3/toolchain/manifest-stable.json`
- Modify: `/Users/mduthey/Documents/Work/txpipe/tx3/toolchain/manifest-beta.json`
- Modify: `/Users/mduthey/Documents/Work/txpipe/tx3/toolchain/manifest-nightly.json`

- [ ] **Step 1: Switch to the toolchain repo and create a feature branch**

```bash
cd /Users/mduthey/Documents/Work/txpipe/tx3/toolchain
git checkout main
git pull
git checkout -b add-preflight
```

- [ ] **Step 2: Add `preflight` entry to `manifest-stable.json`**

Open `manifest-stable.json`. Append a new object to the `tools` array (after the `cshell` entry). The full file should end with the entries below (preserving the existing 5 tools, only the `preflight` entry is new):

```json
    {
      "name": "cshell",
      "description": "A terminal wallet for Cardano",
      "repo_name": "cshell",
      "repo_owner": "txpipe",
      "version": "^0.13.2"
    },
    {
      "name": "preflight",
      "description": "Aiken smart contract vulnerability auditor",
      "repo_name": "preflight",
      "repo_owner": "tx3-lang",
      "version": "^0.1"
    }
  ]
}
```

- [ ] **Step 3: Add identical `preflight` entry to `manifest-beta.json`**

`manifest-beta.json` is byte-identical to `manifest-stable.json` today. Append the exact same `preflight` entry as Step 2.

- [ ] **Step 4: Add `preflight` entry to `manifest-nightly.json` with `"version": "^0"`**

Note: nightly uses `"^0"` for all tools, not pinned versions. After the existing `cshell` entry, append:

```json
    {
      "name": "cshell",
      "description": "A terminal wallet for Cardano",
      "repo_name": "cshell",
      "repo_owner": "txpipe",
      "version": "^0"
    },
    {
      "name": "preflight",
      "description": "Aiken smart contract vulnerability auditor",
      "repo_name": "preflight",
      "repo_owner": "tx3-lang",
      "version": "^0"
    }
  ]
}
```

- [ ] **Step 5: Verify all three files are valid JSON**

Run:
```bash
cd /Users/mduthey/Documents/Work/txpipe/tx3/toolchain
python3 -m json.tool manifest-stable.json > /dev/null && echo "stable OK"
python3 -m json.tool manifest-beta.json > /dev/null && echo "beta OK"
python3 -m json.tool manifest-nightly.json > /dev/null && echo "nightly OK"
```

Expected: three lines printing "OK". A non-zero exit means malformed JSON; re-open the file and fix the trailing comma / bracket issue before continuing.

- [ ] **Step 6: Commit**

```bash
cd /Users/mduthey/Documents/Work/txpipe/tx3/toolchain
git add manifest-stable.json manifest-beta.json manifest-nightly.json
git commit -m "feat: add preflight to toolchain manifests"
```

- [ ] **Step 7: Push and open PR**

```bash
git push -u origin add-preflight
gh pr create --title "feat: add preflight to toolchain manifests" --body "$(cat <<'EOF'
## Summary
- Adds `preflight` (Aiken smart contract vulnerability auditor) to stable, beta, and nightly manifests.
- Stable + beta pin to `^0.1`; nightly uses `^0`.
- Required precondition for trix `feat/aiken-vulnerability-detection` to merge — trix will spawn the binary via tx3up's install path.

## Test plan
- [ ] Validate JSON parses for all three manifests
- [ ] After merge, run `tx3up` against this manifest and confirm `~/.tx3/default/bin/preflight` is installed
EOF
)"
```

This PR must merge before Phase 2's release (Step 18) reaches end users.

---

## Phase 2: Trix wrapper

All remaining tasks happen in `/Users/mduthey/Documents/Work/txpipe/tx3/trix` on branch `feat/aiken-vulnerability-detection` (the user has already merged main into this branch).

### Task 2: Read the existing spawn pattern (prep, read-only)

**Files:**
- Read: `src/spawn/tx3c.rs`
- Read: `src/spawn/dolos.rs`
- Read: `src/spawn/cshell.rs`
- Read: `src/home.rs`

- [ ] **Step 1: Confirm the spawn pattern**

Read each file. Confirm:
- All use `crate::home::tool_path("<name>")` to resolve the binary.
- All use `Command::new(...)` and call `.status()` (not `.output()`) so stdio is inherited.
- All return `miette::Result<()>` and `bail!` on non-zero exit codes.
- Flag forwarding is straightforward `cmd.args(["--flag", value])` calls.

The new `spawn/preflight.rs` should match this style exactly. No code changes in this task.

### Task 3: Remove broken audit-internal test imports

**Files:**
- Modify: `tests/e2e/happy_path.rs`
- Modify: `tests/e2e/edge_cases.rs`

These tests reference `trix::commands::audit::model::AnalysisStateJson`, which will not exist after the audit module is replaced. Remove them now so the project keeps compiling at every step.

- [ ] **Step 1: Remove the AnalysisStateJson import and the two audit happy-path tests**

In `tests/e2e/happy_path.rs`:
- Delete line `use trix::commands::audit::model::AnalysisStateJson;` near the top
- Delete the entire test function `aiken_audit_runs_in_initialized_project` (lines ~186-216)
- Delete the entire test function `aiken_audit_runs_with_heuristic_provider` (lines ~218-242)

- [ ] **Step 2: Remove the missing-skills-dir edge case**

In `tests/e2e/edge_cases.rs`, delete the entire `aiken_audit_fails_with_missing_skills_dir` test function (lines ~63-81). The error message it asserts (`"Audit skills directory not found"`) is preflight's responsibility now.

Keep `aiken_audit_fails_without_trix_config` — that test exercises trix's `run_global_command` routing (see `src/main.rs:25-31`), which is unchanged by this work.

- [ ] **Step 3: Verify the test file still compiles**

Run:
```bash
cargo check --features unstable --tests
```

Expected: clean compile. If you see other references to `trix::commands::audit::*` types, grep them out:

```bash
grep -rn "trix::commands::audit::" tests/ src/
```

Only `src/main.rs:50` (the dispatch in `run_scoped_command`) and `src/cli.rs:59` (the `Audit` variant) should match — both use `commands::audit::Args` and `commands::audit::run`, which are preserved.

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/happy_path.rs tests/e2e/edge_cases.rs
git commit -m "test: drop audit tests that depend on internal types being extracted"
```

### Task 4: Replace the audit module with a wrapper skeleton (stubbed spawn)

This task does the structural replacement: deletes the directory, creates the new flat file, deletes the embedded assets, drops the heavy deps, and creates a stubbed `spawn::preflight`. The project compiles at the end. Functionality is broken — `trix audit` returns a "not implemented" error — but tests for help text and the global-config check still pass.

**Files:**
- Delete: `src/commands/audit/` (entire dir, 10 .rs files)
- Delete: `skills/vulnerabilities/` (3 .md files)
- Delete: `templates/aiken/` (5 .md files)
- Delete: `tests/fixtures/audit/` (empty placeholder)
- Create: `src/commands/audit.rs`
- Create: `src/spawn/preflight.rs`
- Modify: `src/spawn/mod.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Delete the audit directory and embedded assets**

```bash
cd /Users/mduthey/Documents/Work/txpipe/tx3/trix
rm -rf src/commands/audit
rm -rf skills/vulnerabilities
rm -rf templates/aiken
rmdir tests/fixtures/audit 2>/dev/null || true
```

If `templates/` or `skills/` end up empty, leave them alone — they may have other contents:
```bash
ls templates/ skills/
```
`templates/` should still contain `tx3/`, `configs/`, `profile/` — and `skills/` is now gone (it only had `vulnerabilities/`). That is fine; the directory will be recreated if other skills are added later.

At this point the project will NOT compile (`commands/mod.rs` and `cli.rs` reference `commands::audit::*`). Steps 2-4 fix that.

- [ ] **Step 2: Create `src/commands/audit.rs` (the wrapper)**

Create the file with this exact content:

```rust
use clap::{Args as ClapArgs, ValueEnum};
use miette::Result;

use crate::config::{ProfileConfig, RootConfig};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReadScopeArg {
    Workspace,
    Strict,
}

impl ReadScopeArg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Strict => "strict",
        }
    }
}

#[derive(ClapArgs)]
pub struct Args {
    /// Path where the incremental analysis state JSON will be written.
    #[arg(long, default_value = ".tx3/audit/state.json")]
    pub state_out: String,

    /// Path where the final vulnerability report markdown will be written.
    #[arg(long, default_value = ".tx3/audit/vulnerabilities.md")]
    pub report_out: String,

    /// Path to vulnerability skill definitions.
    #[arg(long, default_value = "skills/vulnerabilities")]
    pub skills_dir: String,

    /// Path where the Aiken AST snapshot JSON will be written.
    #[arg(long, default_value = ".tx3/audit/aiken-ast.json")]
    pub ast_out: String,

    /// Analysis provider: scaffold | heuristic | openai | anthropic | ollama
    #[arg(long, default_value = "scaffold")]
    pub provider: String,

    /// API endpoint override. Default depends on --provider.
    #[arg(long)]
    pub endpoint: Option<String>,

    /// Model override. Default depends on --provider.
    #[arg(long)]
    pub model: Option<String>,

    /// API key environment variable override. Default depends on --provider.
    #[arg(long)]
    pub api_key_env: Option<String>,

    /// Optional reasoning effort hint for OpenAI-compatible providers (e.g. low|medium|high).
    #[arg(long)]
    pub reasoning_effort: Option<String>,

    /// Print chat-style progress of model requests and local tool actions while auditing.
    #[arg(long, default_value_t = false)]
    pub ai_logs: bool,

    /// Regenerate AST even if an up-to-date snapshot is already available.
    #[arg(long, default_value_t = false)]
    pub no_ast_cache: bool,

    /// File read scope for AI-assisted local tool requests: workspace | strict.
    #[arg(long, value_enum, default_value_t = ReadScopeArg::Workspace)]
    pub read_scope: ReadScopeArg,

    /// Ask confirmation before executing each AI-requested local read action.
    #[arg(long, default_value_t = false)]
    pub interactive_permissions: bool,
}

#[allow(unused_variables)]
pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> Result<()> {
    #[cfg(feature = "unstable")]
    {
        let _ = profile;
        crate::spawn::preflight::run(args, config)
    }
    #[cfg(not(feature = "unstable"))]
    {
        let _ = (args, config, profile);
        Err(miette::miette!(
            "The audit command is currently unstable and requires the `unstable` feature to be enabled."
        ))
    }
}
```

- [ ] **Step 3: Create `src/spawn/preflight.rs` (stubbed for now)**

Create the file. We start with a stub that returns an error, so the integration test in Task 5 can see "red" before we implement the real logic in Task 6.

```rust
use miette::bail;

use crate::commands::audit::Args;
use crate::config::RootConfig;

#[allow(unused_variables)]
pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    bail!("preflight spawn not implemented")
}
```

- [ ] **Step 4: Register the new `spawn::preflight` module**

Edit `src/spawn/mod.rs`. Add `pub mod preflight;` so the file looks like:

```rust
pub mod cshell;
pub mod dolos;
pub mod preflight;
pub mod tx3c;
```

- [ ] **Step 5: Drop `aiken-lang` and `serde_yaml_ng` from `Cargo.toml`**

Open `Cargo.toml`. In the `[dependencies]` section:
- Delete the line `aiken-lang = "1.1.21"`
- Delete the line `serde_yaml_ng = "0.10"`

Leave all other dependencies in place. The `[features] unstable = []` block stays.

- [ ] **Step 6: Verify the project compiles cleanly with the unstable feature**

Run:
```bash
cargo build --features unstable
```

Expected: clean compile. If it fails:
- `unresolved import trix::commands::audit::model` — there is still a leftover reference somewhere; grep with `grep -rn "audit::model\|audit::providers\|audit::ast" src/ tests/` and remove.
- `cannot find module audit` — make sure `src/commands/audit.rs` exists and `src/commands/audit/` was deleted.
- `unresolved import crate::spawn::preflight` — make sure `src/spawn/mod.rs` has the new `pub mod preflight;` line.

- [ ] **Step 7: Verify the project compiles cleanly without the feature too**

Run:
```bash
cargo build
```

Expected: clean compile. The `#[cfg(not(feature = "unstable"))]` branch in `audit.rs` returns the error path; the `spawn::preflight` module is still compiled (Rust modules don't get gated by feature unless explicitly annotated), but its only caller is in the gated block, so the unused-warning is suppressed by `#[allow(unused_variables)]`.

- [ ] **Step 8: Run the existing test suite to confirm no regressions**

Run:
```bash
cargo test --features unstable
```

Expected: all tests pass. The two audit `--help` tests in `tests/e2e/smoke.rs` (`audit_help_runs_without_error`, `audit_help_displays_provider_options`) pass because the `Args` mirror in the new `audit.rs` reproduces the same flags. The `aiken_audit_fails_without_trix_config` test in `edge_cases.rs` passes because the global routing in `main.rs` is unchanged.

If one of those three tests fails:
- `audit_help_runs_without_error` expects `"vulnerability"` in stdout — verify the command doc on `Audit(commands::audit::Args)` in `src/cli.rs:57-59` still contains the word "vulnerability".
- `audit_help_displays_provider_options` expects `"provider"` in stdout — verify the `provider` flag's `///` doc comment in the new `Args` includes the word.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(audit): replace in-tree audit with wrapper + stubbed spawn

- Delete src/commands/audit/, skills/vulnerabilities/, templates/aiken/
- Add src/commands/audit.rs as a thin clap wrapper (Args mirrors preflight)
- Add src/spawn/preflight.rs as a stub returning a 'not implemented' error
- Drop aiken-lang and serde_yaml_ng from Cargo.toml
- Remove tests that depended on internal audit types

Spawn forwarding implemented in the next commit."
```

### Task 5: Write the failing integration test for spawn forwarding (TDD red)

**Files:**
- Create: `tests/e2e/audit_wrapper.rs`
- Modify: `tests/e2e/mod.rs`

- [ ] **Step 1: Add the `audit_wrapper` module to the e2e test harness**

Edit `tests/e2e/mod.rs`. Find the section that lists test submodules (around line 209):

```rust
pub mod edge_cases;
pub mod happy_path;
pub mod smoke;
```

Add `pub mod audit_wrapper;` so it reads:

```rust
pub mod audit_wrapper;
pub mod edge_cases;
pub mod happy_path;
pub mod smoke;
```

- [ ] **Step 2: Write the integration test**

Create `tests/e2e/audit_wrapper.rs` with this exact content:

```rust
//! Integration tests for the `trix audit` spawn wrapper.
//!
//! Strategy: point `TX3_PREFLIGHT_PATH` at a small bash script that records
//! its argv to a file and exits 0. Then run `trix audit ...` and assert that
//! the recorded argv contains the flags we expect to forward.

#![cfg(all(unix, feature = "unstable"))]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn install_mock_preflight(ctx: &TestContext, log_path: &str) -> std::path::PathBuf {
    let mock_path = ctx.file_path("mock-preflight.sh");
    let log_full_path = ctx.file_path(log_path);

    let script = format!(
        "#!/usr/bin/env bash\nprintf '%s\\n' \"$@\" > {log}\nexit 0\n",
        log = log_full_path.display(),
    );
    fs::write(&mock_path, script).expect("write mock script");

    let mut perms = fs::metadata(&mock_path).expect("stat mock").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&mock_path, perms).expect("chmod mock");

    mock_path
}

fn run_audit_with_mock(ctx: &TestContext, audit_args: &[&str]) -> (CommandResult, Vec<String>) {
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    let mock_path = install_mock_preflight(ctx, "argv.log");
    let mock_path_str = mock_path.to_string_lossy().to_string();

    let result = ctx.run_trix_with_env(
        audit_args,
        &[("TX3_PREFLIGHT_PATH", mock_path_str.as_str())],
    );

    let recorded = fs::read_to_string(ctx.file_path("argv.log"))
        .expect("mock should have written argv.log");
    let lines: Vec<String> = recorded.lines().map(str::to_string).collect();

    (result, lines)
}

fn flag_value<'a>(argv: &'a [String], flag: &str) -> Option<&'a str> {
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .map(String::as_str)
}

#[test]
fn forwards_default_flags_and_injects_main_source() {
    let ctx = TestContext::new();
    let (result, argv) = run_audit_with_mock(&ctx, &["audit"]);

    assert_success(&result);

    // Default flag values from src/commands/audit.rs are forwarded.
    assert_eq!(
        flag_value(&argv, "--provider"),
        Some("scaffold"),
        "argv: {:?}",
        argv
    );
    assert_eq!(flag_value(&argv, "--state-out"), Some(".tx3/audit/state.json"));
    assert_eq!(
        flag_value(&argv, "--report-out"),
        Some(".tx3/audit/vulnerabilities.md")
    );
    assert_eq!(flag_value(&argv, "--skills-dir"), Some("skills/vulnerabilities"));
    assert_eq!(flag_value(&argv, "--ast-out"), Some(".tx3/audit/aiken-ast.json"));
    assert_eq!(flag_value(&argv, "--read-scope"), Some("workspace"));

    // --main-source is injected from RootConfig.protocol.main, not from the
    // user-facing CLI of `trix audit`. The init template uses "main.tx3".
    assert_eq!(flag_value(&argv, "--main-source"), Some("main.tx3"));

    // Boolean flags default to off → not present in argv.
    assert!(!argv.iter().any(|a| a == "--ai-logs"));
    assert!(!argv.iter().any(|a| a == "--no-ast-cache"));
    assert!(!argv.iter().any(|a| a == "--interactive-permissions"));
}

#[test]
fn forwards_provider_overrides_and_optional_flags() {
    let ctx = TestContext::new();
    let (result, argv) = run_audit_with_mock(
        &ctx,
        &[
            "audit",
            "--provider", "openai",
            "--model", "gpt-test",
            "--endpoint", "https://example/v1/responses",
            "--api-key-env", "MY_KEY",
            "--reasoning-effort", "high",
            "--ai-logs",
            "--no-ast-cache",
            "--read-scope", "strict",
            "--interactive-permissions",
        ],
    );

    assert_success(&result);

    assert_eq!(flag_value(&argv, "--provider"), Some("openai"));
    assert_eq!(flag_value(&argv, "--model"), Some("gpt-test"));
    assert_eq!(
        flag_value(&argv, "--endpoint"),
        Some("https://example/v1/responses")
    );
    assert_eq!(flag_value(&argv, "--api-key-env"), Some("MY_KEY"));
    assert_eq!(flag_value(&argv, "--reasoning-effort"), Some("high"));
    assert_eq!(flag_value(&argv, "--read-scope"), Some("strict"));

    assert!(argv.iter().any(|a| a == "--ai-logs"));
    assert!(argv.iter().any(|a| a == "--no-ast-cache"));
    assert!(argv.iter().any(|a| a == "--interactive-permissions"));
}

#[test]
fn propagates_non_zero_exit_from_preflight() {
    let ctx = TestContext::new();
    let init_result = ctx.run_trix(&["init", "--yes"]);
    assert_success(&init_result);

    // Mock that exits non-zero.
    let mock_path = ctx.file_path("mock-fail.sh");
    fs::write(&mock_path, "#!/usr/bin/env bash\nexit 7\n").expect("write");
    let mut perms = fs::metadata(&mock_path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&mock_path, perms).expect("chmod");

    let result = ctx.run_trix_with_env(
        &["audit"],
        &[(
            "TX3_PREFLIGHT_PATH",
            mock_path.to_string_lossy().to_string().as_str(),
        )],
    );

    assert!(
        !result.success(),
        "trix audit should fail when preflight exits non-zero"
    );
}
```

- [ ] **Step 3: Run the new tests and confirm they fail**

Run:
```bash
cargo test --features unstable --test e2e_tests audit_wrapper
```

Expected: all three tests in `audit_wrapper` FAIL with errors mentioning `"preflight spawn not implemented"`. This confirms our stub is being reached (good) but doesn't yet do the work (red).

If the tests pass instead, something's wrong — the stub should be returning an error. If the tests skip, check the `#![cfg(all(unix, feature = "unstable"))]` line — `cargo test --features unstable` on a unix machine should run them.

If the tests error compiling because `argv.log` is unwritten, check that the mock script's heredoc-style write actually escapes correctly when interpolated. The script content uses Rust raw-style format with `\\n` so the bash `\n` is literal in the file — that's intentional.

### Task 6: Implement the real spawn forwarding (TDD green)

**Files:**
- Modify: `src/spawn/preflight.rs`

- [ ] **Step 1: Replace the stub with the real implementation**

Replace the entire contents of `src/spawn/preflight.rs` with:

```rust
use std::process::Command;

use miette::{Context as _, IntoDiagnostic as _, bail};

use crate::commands::audit::Args;
use crate::config::RootConfig;

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    let tool_path = crate::home::tool_path("preflight")?;

    let mut cmd = Command::new(tool_path);

    // Always-present flags with default values.
    cmd.args(["--state-out", &args.state_out]);
    cmd.args(["--report-out", &args.report_out]);
    cmd.args(["--skills-dir", &args.skills_dir]);
    cmd.args(["--ast-out", &args.ast_out]);
    cmd.args(["--provider", &args.provider]);
    cmd.args(["--read-scope", args.read_scope.as_str()]);

    // Optional string flags.
    if let Some(value) = &args.endpoint {
        cmd.args(["--endpoint", value]);
    }
    if let Some(value) = &args.model {
        cmd.args(["--model", value]);
    }
    if let Some(value) = &args.api_key_env {
        cmd.args(["--api-key-env", value]);
    }
    if let Some(value) = &args.reasoning_effort {
        cmd.args(["--reasoning-effort", value]);
    }

    // Boolean flags.
    if args.ai_logs {
        cmd.arg("--ai-logs");
    }
    if args.no_ast_cache {
        cmd.arg("--no-ast-cache");
    }
    if args.interactive_permissions {
        cmd.arg("--interactive-permissions");
    }

    // --main-source is injected from RootConfig.protocol.main; preflight
    // expects it as the fallback when its own .ak discovery returns empty.
    let main_source = config.protocol.main.to_string_lossy().to_string();
    cmd.args(["--main-source", &main_source]);

    let status = cmd
        .status()
        .into_diagnostic()
        .context("running preflight")?;

    if !status.success() {
        bail!("preflight exited with non-zero status");
    }

    Ok(())
}
```

- [ ] **Step 2: Run the integration tests and confirm they pass**

Run:
```bash
cargo test --features unstable --test e2e_tests audit_wrapper
```

Expected: all three tests in `audit_wrapper` PASS.

If `forwards_default_flags_and_injects_main_source` fails on the `--main-source` assertion with something other than `"main.tx3"`:
- Inspect what `trix init --yes` writes for `protocol.main` (read `templates/tx3/trix.toml` or run `cat trix.toml` in the temp dir during the test). Update the assertion to match the init template's actual default.

If `propagates_non_zero_exit_from_preflight` fails because trix returns success: confirm the new `spawn/preflight.rs` checks `!status.success()` and `bail!`s. The mock exits 7 → trix should bail.

- [ ] **Step 3: Run the full test suite to confirm no regressions**

Run:
```bash
cargo test --features unstable
```

Expected: all tests pass, including the existing `audit_help_*` smoke tests and `aiken_audit_fails_without_trix_config`.

- [ ] **Step 4: Run a quick build without the feature to confirm gating still works**

Run:
```bash
cargo build
```

Expected: clean compile. Then verify the unstable error message:
```bash
TRIX_BIN="$(pwd)/target/debug/trix"
SMOKE_DIR="$(mktemp -d)"
cd "$SMOKE_DIR"
"$TRIX_BIN" init --yes
"$TRIX_BIN" audit 2>&1 | head -5
cd - > /dev/null
rm -rf "$SMOKE_DIR"
```

Expected: the audit invocation prints something containing `"requires the `unstable` feature to be enabled"`.

- [ ] **Step 5: Commit**

```bash
cd /Users/mduthey/Documents/Work/txpipe/tx3/trix
git add src/spawn/preflight.rs tests/e2e/mod.rs tests/e2e/audit_wrapper.rs
git commit -m "feat(audit): forward flags to preflight via Command::new

- spawn::preflight::run resolves the binary via home::tool_path
- forwards every public preflight flag verbatim
- injects --main-source from RootConfig.protocol.main
- propagates non-zero exit codes via miette bail
- integration test uses TX3_PREFLIGHT_PATH + a bash mock to assert argv"
```

### Task 7: Final verification

**Files:** none modified.

- [ ] **Step 1: Run cargo fmt to confirm style is consistent**

Run:
```bash
cargo fmt --check
```

Expected: no output (clean). If files are flagged, run `cargo fmt` and amend the previous commit:
```bash
cargo fmt
git add -u
git commit --amend --no-edit
```

- [ ] **Step 2: Run cargo clippy on both feature configurations**

Run:
```bash
cargo clippy --features unstable -- -D warnings
cargo clippy -- -D warnings
```

Expected: no warnings. If clippy complains about unused variables in `spawn/preflight.rs`, you forgot to remove the `#[allow(unused_variables)]` attribute that was on the stub — remove it now since the real implementation uses everything.

- [ ] **Step 3: Run the full test suite once more on both feature configurations**

Run:
```bash
cargo test --features unstable
cargo test
```

Expected: all tests pass in both runs.

- [ ] **Step 4: Confirm dependency graph no longer contains aiken-lang**

Run:
```bash
cargo tree --features unstable | grep -E "aiken-lang|serde_yaml_ng" || echo "clean"
```

Expected: prints `clean`. If it prints any aiken or serde_yaml_ng line, some other dependency is pulling them in transitively — investigate which crate (the line above the match in `cargo tree` shows the parent).

- [ ] **Step 5: Push the branch**

```bash
git push origin feat/aiken-vulnerability-detection
```

This phase is complete when the push succeeds and the branch is ready for PR review.

---

## Phase 3: Release coordination (out of plan scope)

The trix release that picks up these changes must wait until the toolchain PR from Phase 1 is merged. Once both are merged:

1. Confirm Phase 1 is on `main` of `tx3-lang/toolchain`.
2. Open the trix PR from `feat/aiken-vulnerability-detection` to `main`. Merge.
3. Cut a trix release via the existing `cargo-release` flow (`release.toml` configured in repo root).
4. Validate end-to-end on a clean machine: `tx3up` → `trix audit --provider scaffold` produces `.tx3/audit/{state.json,vulnerabilities.md,aiken-ast.json}`.

Release execution is a manual operator step, not part of this implementation plan.

---

## Self-review

**Spec coverage:**
- Architecture diagram → represented in `audit.rs` + `spawn/preflight.rs` design across Tasks 4 and 6.
- Trix-side files deleted/created/modified → Task 4 (file structure) + Tasks 3, 5, 6, 7.
- `aiken-lang` and `serde_yaml_ng` removal → Task 4 Step 5; verified Task 7 Step 4.
- Toolchain manifest changes → Task 1 (all three manifests).
- Sequencing (toolchain first, then trix) → enforced by Phase 1 preceding Phase 2; release dependency documented in Phase 3.
- Stdio inheritance via `.status()` → Task 6 Step 1.
- `home::tool_path("preflight")` lookup → Task 6 Step 1.
- `unstable` feature gate preserved → Task 4 Step 2 (the cfg block in `audit::run`).
- `#[command(hide = true)]` preserved → no edit to `cli.rs` (file structure section confirms this).
- Test strategy (mock preflight via `TX3_PREFLIGHT_PATH`) → Task 5 Step 2.
- Risk: flag drift → caught by Task 5 tests.
- Risk: `config.protocol.main` resolution → covered by Task 5 default-flags assertion plus Task 6 Step 1 implementation.
- Risk: transition window → handled by Phase 1 precedence.

**Placeholder scan:** searched for "TBD", "TODO", "implement later", "similar to" — none present in task code blocks. All steps include the actual code, exact commands, and expected outputs.

**Type consistency:** `Args`, `ReadScopeArg`, and `ReadScopeArg::as_str(self) -> &'static str` are defined in Task 4 Step 2 and used unchanged in Task 6 Step 1. `crate::commands::audit::Args` and `crate::config::RootConfig` imports match across both files.
