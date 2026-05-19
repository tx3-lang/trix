//! Compatibility matrix and version gating for the external CLIs `trix`
//! drives.
//!
//! `trix` links no `tx3-*` crate; it orchestrates the toolchain binaries
//! (`tx3c`, `cshell`, `dolos`) as subprocesses. Each tool's CLI surface —
//! subcommands, flags, JSON output — is the versioned contract. Rather than
//! embed markers in payloads, we gate on the binary's own `--version` against
//! a supported window. This module owns that mechanism and the single matrix
//! for every integration.

use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// The supported version window for one external CLI: `min` is the inclusive
/// lower bound, `before` the exclusive upper bound (`min <= v < before`).
///
/// The window is bounded on both ends on purpose: too-old binaries lack
/// capabilities `trix` relies on, and — since the toolchain is pre-1.0, where
/// a new minor may break the CLI — a too-new binary may have changed the
/// contract out from under us. Widen the window when `trix` is updated to
/// track a new release.
struct Compat {
    tool: &'static str,
    min: &'static str,
    before: &'static str,
}

const COMPAT_MATRIX: &[Compat] = &[
    // 0.18.0 introduced `decode`, `--emit tir-json`, `--diagnostics-format`
    // (0.17.0 was cut before that surface existed).
    Compat {
        tool: "tx3c",
        min: "0.18.0",
        before: "0.19.0",
    },
];

fn entry(tool: &str) -> Option<&'static Compat> {
    COMPAT_MATRIX.iter().find(|c| c.tool == tool)
}

/// Probe `<tool> --version` and confirm it falls within the supported window
/// in [`COMPAT_MATRIX`].
///
/// A no-op for tools not in the matrix. Cached per tool: a toolchain can't
/// change mid-process, so each tool is probed at most once. Call this before
/// the first subprocess invocation of any gated tool.
///
/// Escape hatch: setting `TX3_SKIP_COMPAT_CHECK` to a non-empty value bypasses
/// the window. This exists for developing/CI-testing against an *unreleased*
/// toolchain — a locally built `tx3c` carries the new CLI surface but still
/// reports the pre-bump version until its release is cut. Not for end users.
pub fn ensure_supported(tool: &str) -> miette::Result<()> {
    if std::env::var_os("TX3_SKIP_COMPAT_CHECK").is_some_and(|v| !v.is_empty()) {
        return Ok(());
    }

    let Some(c) = entry(tool) else {
        return Ok(());
    };

    static CACHE: OnceLock<Mutex<HashMap<&'static str, Result<(), String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let cached = cache.lock().unwrap().get(c.tool).cloned();
    let result = match cached {
        Some(r) => r,
        None => {
            let r = check(c);
            cache.lock().unwrap().insert(c.tool, r.clone());
            r
        }
    };

    result.map_err(|m| miette::miette!("incompatible tx3 toolchain: {m}"))
}

fn check(c: &Compat) -> Result<(), String> {
    let tool = c.tool;
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

    let min = semver::Version::parse(c.min).expect("valid matrix const");
    let before = semver::Version::parse(c.before).expect("valid matrix const");

    if found < min {
        return Err(format!(
            "your {tool} is {found}, but this trix requires {tool} >= {min}. \
             Run `tx3up` to update your tx3 toolchain."
        ));
    }

    if found >= before {
        return Err(format!(
            "your {tool} is {found}, newer than this trix supports \
             ({tool} >= {min}, < {before}). Update trix (or pin an older {tool})."
        ));
    }

    Ok(())
}
