//! Compatibility matrix and version gating for the external CLIs `trix`
//! drives.
//!
//! `trix` links no implementation crate of a dependent tool; it orchestrates
//! the toolchain binaries (`tx3c`, `cshell`, `dolos`) as subprocesses. Each
//! tool's CLI surface — subcommands, flags, JSON output — is the versioned
//! contract. Rather than embed markers in payloads, we gate on the binary's
//! own `--version`. This module owns that mechanism and the single matrix for
//! every integration. See `design/004-toolchain-delegation.md`.

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::config::RootConfig;

/// The supported version window for one external CLI. `min` is the inclusive
/// lower bound — the oldest release whose surface `trix` relies on. The
/// exclusive upper bound is derived, not stored: it is the **next major**
/// version (`min.major + 1`.0.0).
///
/// A breaking change to a tool's CLI is expected to be signalled by a major
/// version bump (semver), so `trix` accepts any release within the same major
/// and needs updating only when a tool makes a breaking, major change — not on
/// every minor. Raise `min` when `trix` starts relying on a newer capability.
struct Compat {
    tool: &'static str,
    min: &'static str,
}

const COMPAT_MATRIX: &[Compat] = &[
    // 0.22.0 introduced parametric tuple types: tx3c now emits TIR carrying the
    // `Tuple` type/expression variants, a forward-incompatible addition that
    // pre-0.22 readers cannot decode. Pin the floor here so the TIR `trix`
    // consumes always matches the schema it supports.
    Compat {
        tool: "tx3c",
        min: "0.22.0",
    },
];

fn entry(tool: &str) -> Option<&'static Compat> {
    COMPAT_MATRIX.iter().find(|c| c.tool == tool)
}

/// Per-tool minimum versions declared by the current project's `trix.toml`
/// `[toolchain]` table. Set once at command startup (a process drives a single
/// project), read during version gating.
static PROJECT_MINS: OnceLock<HashMap<String, semver::Version>> = OnceLock::new();

/// Record the project-declared minimum tool versions from `config`. Call once,
/// before the first tool spawn. Version strings are parsed here so a malformed
/// `[toolchain]` entry in `trix.toml` fails fast with a clear error.
///
/// A project minimum is a *lower bound only*: it raises the floor of the
/// built-in support window ([`COMPAT_MATRIX`]) but never relaxes its upper
/// bound. A tool with no matrix entry is still gated against its project
/// minimum, if one is declared.
pub fn register_project_requirements(config: &RootConfig) -> miette::Result<()> {
    let mins = collect_project_mins(config)?;
    // OnceLock: first writer wins; a process only ever loads one project.
    let _ = PROJECT_MINS.set(mins);
    Ok(())
}

/// Parse the declared `[toolchain]` minimums into a tool→version map, failing
/// on a malformed version string. Pure (no global state) so it is unit-testable.
fn collect_project_mins(config: &RootConfig) -> miette::Result<HashMap<String, semver::Version>> {
    let mut mins = HashMap::new();

    if let Some(req) = config.toolchain.as_ref().and_then(|t| t.tx3c.as_ref()) {
        let version = semver::Version::parse(req).map_err(|e| {
            miette::miette!("invalid `[toolchain] tx3c` version {req:?} in trix.toml: {e}")
        })?;
        mins.insert("tx3c".to_string(), version);
    }

    Ok(mins)
}

fn project_min(tool: &str) -> Option<semver::Version> {
    PROJECT_MINS.get().and_then(|m| m.get(tool).cloned())
}

/// Probe `<tool> --version` and confirm it falls within the supported window
/// in [`COMPAT_MATRIX`] (`min <= v`, and `v` within the same major as `min`).
///
/// A no-op for tools not in the matrix. Cached per tool: a toolchain can't
/// change mid-process, so each tool is probed at most once. Call this before
/// the first subprocess invocation of any gated tool.
///
/// Escape hatch: setting `TX3_SKIP_COMPAT_CHECK` to a non-empty value bypasses
/// the window. This exists for developing/CI-testing against an *unreleased*
/// toolchain — a locally built tool carries the new CLI surface but still
/// reports the pre-bump version until its release is cut. Not for end users.
pub fn ensure_supported(tool: &str) -> miette::Result<()> {
    if std::env::var_os("TX3_SKIP_COMPAT_CHECK").is_some_and(|v| !v.is_empty()) {
        return Ok(());
    }

    let matrix = entry(tool);
    let project_min = project_min(tool);

    // Nothing to enforce: tool is neither in the matrix nor constrained by the
    // project's `trix.toml`.
    if matrix.is_none() && project_min.is_none() {
        return Ok(());
    }

    static CACHE: OnceLock<Mutex<HashMap<String, Result<(), String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let cached = cache.lock().unwrap().get(tool).cloned();
    let result = match cached {
        Some(r) => r,
        None => {
            let r = check(tool, matrix, project_min.as_ref());
            cache.lock().unwrap().insert(tool.to_string(), r.clone());
            r
        }
    };

    result.map_err(|m| miette::miette!("incompatible tx3 toolchain: {m}"))
}

fn check(
    tool: &str,
    matrix: Option<&Compat>,
    project_min: Option<&semver::Version>,
) -> Result<(), String> {
    let path = crate::home::tool_path(tool).map_err(|e| e.to_string())?;

    let output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| format!("could not run `{tool} --version`: {e}"))?;

    if !output.status.success() {
        return Err(format!("`{tool} --version` exited with an error"));
    }

    // clap-based tools print `<name> <semver>`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = stdout.split_whitespace().last().unwrap_or("").trim();
    let found = semver::Version::parse(raw)
        .map_err(|e| format!("cannot parse {tool} version from {stdout:?}: {e}"))?;

    evaluate(tool, &found, matrix, project_min)
}

/// Decide whether `found` satisfies the project floor and trix's support
/// window. Pure (no subprocess, no globals) so the version logic is
/// unit-testable in isolation from the `--version` probe.
fn evaluate(
    tool: &str,
    found: &semver::Version,
    matrix: Option<&Compat>,
    project_min: Option<&semver::Version>,
) -> Result<(), String> {
    // Project-declared floor (from `trix.toml [toolchain]`). Lower bound only.
    if let Some(min) = project_min.filter(|min| found < *min) {
        return Err(format!(
            "your {tool} is {found}, but this protocol requires {tool} >= {min} \
             (declared in trix.toml [toolchain]). Run `tx3up` to update your tx3 toolchain."
        ));
    }

    // trix's own built-in support window.
    if let Some(c) = matrix {
        let min = semver::Version::parse(c.min).expect("valid matrix const");
        // Exclusive upper bound: the next major. Same-major releases are
        // accepted; a breaking CLI change must come with a major bump.
        let before = semver::Version::new(min.major + 1, 0, 0);

        if *found < min {
            return Err(format!(
                "your {tool} is {found}, but this trix requires {tool} >= {min}. \
                 Run `tx3up` to update your tx3 toolchain."
            ));
        }

        if *found >= before {
            return Err(format!(
                "your {tool} is {found}, newer than this trix supports \
                 ({tool} >= {min}, < {before}). Update trix (or pin an older {tool})."
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    const MATRIX: Compat = Compat {
        tool: "tx3c",
        min: "0.18.0",
    };

    #[test]
    fn project_min_is_a_lower_bound() {
        assert!(evaluate("tx3c", &v("0.21.0"), None, Some(&v("0.22.0"))).is_err());
        assert!(evaluate("tx3c", &v("0.22.0"), None, Some(&v("0.22.0"))).is_ok());
        assert!(evaluate("tx3c", &v("0.25.0"), None, Some(&v("0.22.0"))).is_ok());
    }

    #[test]
    fn matrix_window_enforced() {
        assert!(evaluate("tx3c", &v("0.17.0"), Some(&MATRIX), None).is_err()); // below floor
        assert!(evaluate("tx3c", &v("0.18.0"), Some(&MATRIX), None).is_ok());
        assert!(evaluate("tx3c", &v("0.99.0"), Some(&MATRIX), None).is_ok()); // same major
        assert!(evaluate("tx3c", &v("1.0.0"), Some(&MATRIX), None).is_err()); // next major
    }

    #[test]
    fn project_min_raises_floor_above_matrix() {
        // The matrix allows >= 0.18, but the project demands >= 0.22.
        let err = evaluate("tx3c", &v("0.20.0"), Some(&MATRIX), Some(&v("0.22.0"))).unwrap_err();
        assert!(err.contains("this protocol requires"), "got: {err}");
        assert!(evaluate("tx3c", &v("0.22.0"), Some(&MATRIX), Some(&v("0.22.0"))).is_ok());
    }

    #[test]
    fn matrix_upper_bound_applies_even_when_project_min_satisfied() {
        // Project min is met, but the tool is newer than this trix supports.
        let err = evaluate("tx3c", &v("1.5.0"), Some(&MATRIX), Some(&v("0.22.0"))).unwrap_err();
        assert!(err.contains("newer than this trix supports"), "got: {err}");
    }

    const BASE_TOML: &str = "\
[protocol]
name = \"x\"
version = \"0.1.0\"
main = \"main.tx3\"
[ledger]
family = \"cardano\"
";

    fn config(toml_src: &str) -> RootConfig {
        toml::from_str(toml_src).unwrap()
    }

    #[test]
    fn collects_declared_tx3c_min() {
        let cfg = config(&format!("{BASE_TOML}[toolchain]\ntx3c = \"0.22.0\"\n"));
        let mins = collect_project_mins(&cfg).unwrap();
        assert_eq!(mins.get("tx3c"), Some(&v("0.22.0")));
    }

    #[test]
    fn absent_toolchain_table_yields_no_mins() {
        let cfg = config(BASE_TOML);
        assert!(collect_project_mins(&cfg).unwrap().is_empty());
    }

    #[test]
    fn invalid_declared_version_is_rejected() {
        let cfg = config(&format!("{BASE_TOML}[toolchain]\ntx3c = \"not-semver\"\n"));
        assert!(collect_project_mins(&cfg).is_err());
    }
}
