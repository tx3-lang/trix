use std::{path::Path, process::Command};

use miette::{Context as _, IntoDiagnostic as _, bail};

use crate::config::ProtocolConfig;

pub fn build_tii(
    source: &Path,
    output: &Path,
    protocol: &ProtocolConfig,
    env_file: Option<&Path>,
) -> miette::Result<()> {
    let tool_path = crate::home::tool_path("tx3c")?;

    let mut cmd = Command::new(tool_path.to_str().unwrap_or_default());

    cmd.args(["build", source.to_str().unwrap()]);
    cmd.args(["--emit", "tii"]);
    cmd.args(["--output", output.to_str().unwrap()]);
    cmd.args(["--protocol-name", protocol.name.as_str()]);
    cmd.args(["--protocol-version", protocol.version.as_str()]);

    if let Some(scope) = protocol.scope.as_ref() {
        cmd.args(["--protocol-scope", scope.as_str()]);
    }

    if let Some(env_file) = env_file {
        cmd.args(["--apply-env-file", env_file.to_str().unwrap()]);
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
