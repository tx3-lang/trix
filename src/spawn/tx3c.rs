use std::{path::Path, process::Command};

use miette::{bail, Context as _, IntoDiagnostic as _};
use serde::Deserialize;

use crate::config::RootConfig;
use crate::spawn::ensure_supported;

/// One analyzer diagnostic, as emitted by `tx3c … --diagnostics-format json`.
/// Its shape is part of the `tx3c` CLI contract, gated by the compatibility
/// matrix in [`crate::spawn`].
#[derive(Debug, Deserialize)]
pub struct Diagnostic {
    pub severity: String,
    #[serde(default)]
    pub code: Option<String>,
    pub message: String,
    #[serde(default)]
    pub span: Option<DiagnosticSpan>,
}

#[derive(Debug, Deserialize)]
pub struct DiagnosticSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Deserialize)]
struct DiagnosticsEnvelope {
    diagnostics: Vec<Diagnostic>,
}

fn tx3c() -> miette::Result<Command> {
    ensure_supported("tx3c")?;
    let tool_path = crate::home::tool_path("tx3c")?;
    Ok(Command::new(tool_path.to_str().unwrap_or_default()))
}

pub fn build_tii(source: &Path, output: &Path, config: &RootConfig) -> miette::Result<()> {
    let mut cmd = tx3c()?;

    cmd.args(["build", source.to_str().unwrap()]);
    cmd.args(["--emit", "tii"]);
    cmd.args(["--output", output.to_str().unwrap()]);
    cmd.args(["--protocol-name", config.protocol.name.as_str()]);
    cmd.args(["--protocol-version", config.protocol.version.as_str()]);

    if let Some(scope) = config.protocol.scope.as_ref() {
        cmd.args(["--protocol-scope", scope.as_str()]);
    }

    for profile in config.available_profiles() {
        let profile = config.resolve_profile(&profile)?;

        let env_file = profile.env_file_path();

        if env_file.is_file() {
            let value = format!("{}:{}", profile.name, env_file.to_str().unwrap());
            cmd.args(["--profile-env-file", value.as_str()]);
        } else {
            cmd.args(["--profile", profile.name.as_str()]);
        }
    }

    let output = cmd
        .status()
        .into_diagnostic()
        .context("running tx3c build")?;

    if !output.success() {
        bail!("tx3c failed to build tii");
    }

    Ok(())
}

pub fn codegen(tii_path: &Path, templates: &Path, output: &Path) -> miette::Result<()> {
    let mut cmd = tx3c()?;

    cmd.args(["codegen", "--tii", tii_path.to_str().unwrap()]);
    cmd.args(["--template", templates.to_str().unwrap()]);
    cmd.args(["--output", output.to_str().unwrap()]);

    let output = cmd
        .status()
        .into_diagnostic()
        .context("running tx3c codegen")?;

    if !output.success() {
        bail!("tx3c failed to run codegen");
    }

    Ok(())
}

/// Run the front end over `source` (parse + analyze, no lowering, no
/// artifact) and return the analyzer diagnostics. Empty ⇒ the check passed.
///
/// `tx3c` exits non-zero when there are errors but still writes the envelope
/// to stdout, so a non-zero status is *not* a spawn failure here — we parse
/// stdout regardless and only treat an unparseable/empty stream as one.
pub fn check(source: &Path) -> miette::Result<Vec<Diagnostic>> {
    let mut cmd = tx3c()?;
    cmd.args(["build", source.to_str().unwrap()]);
    cmd.args(["--diagnostics-format", "json"]);

    let output = cmd
        .output()
        .into_diagnostic()
        .context("running tx3c check")?;

    let envelope: DiagnosticsEnvelope = serde_json::from_slice(&output.stdout)
        .into_diagnostic()
        .with_context(|| {
            format!(
                "parsing tx3c diagnostics (stderr: {})",
                String::from_utf8_lossy(&output.stderr).trim()
            )
        })?;

    Ok(envelope.diagnostics)
}

/// Capture the stdout of a `tx3c` invocation that prints a single JSON value,
/// bailing with stderr on a non-zero exit. Used by the TIR-inspection paths.
fn capture_json(mut cmd: Command, what: &str) -> miette::Result<serde_json::Value> {
    let output = cmd
        .output()
        .into_diagnostic()
        .with_context(|| format!("running tx3c {what}"))?;

    if !output.status.success() {
        bail!(
            "tx3c {what} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout)
        .into_diagnostic()
        .with_context(|| format!("parsing tx3c {what} output"))
}

/// Lower `tx_name` from project `source` and return its v1beta0 TIR as JSON.
pub fn tir_from_source(
    source: &Path,
    tx_name: &str,
) -> miette::Result<serde_json::Value> {
    let mut cmd = tx3c()?;
    cmd.args(["build", source.to_str().unwrap()]);
    cmd.args(["--emit", "tir-json"]);
    cmd.args(["--tx", tx_name]);
    capture_json(cmd, "tir-json")
}

/// Decode `tx_name`'s TIR out of a published `.tii`. Same JSON shape as
/// [`tir_from_source`], so callers can't tell source from artifact.
pub fn decode_tir(
    tii_path: &Path,
    tx_name: &str,
) -> miette::Result<serde_json::Value> {
    let mut cmd = tx3c()?;
    cmd.args(["decode", "--tii", tii_path.to_str().unwrap()]);
    cmd.args(["--emit", "tir-json"]);
    cmd.args(["--tx", tx_name]);
    capture_json(cmd, "decode")
}
