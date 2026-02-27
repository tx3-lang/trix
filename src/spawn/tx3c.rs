use std::{path::Path, process::Command};

use miette::{Context as _, IntoDiagnostic as _, bail};

use crate::config::RootConfig;

pub fn build_tii(source: &Path, output: &Path, config: &RootConfig) -> miette::Result<()> {
    let tool_path = crate::home::tool_path("tx3c")?;

    let mut cmd = Command::new(tool_path.to_str().unwrap_or_default());

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
