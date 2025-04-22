use std::process::{Command, Stdio};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};

use crate::config::Config;

use super::{get_home_path, handle_devnet};

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    let home_path = get_home_path()?;
    let tmp_path = handle_devnet(&home_path, config)?;

    let mut cshell_config_path = tmp_path.clone();
    cshell_config_path.push("cshell.toml");

    let mut cshell_path = home_path.clone();
    if cfg!(target_os = "windows") {
        cshell_path.push(".tx3/default/bin/cshell.exe");
    } else {
        cshell_path.push(".tx3/default/bin/cshell");
    };

    let mut cmd = Command::new(cshell_path.to_str().unwrap_or_default());

    cmd.args([
        "-s",
        cshell_config_path.to_str().unwrap_or_default(),
        "explorer",
    ])
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit());

    let mut child = cmd
        .spawn()
        .into_diagnostic()
        .context("failed to spawn cshell explorer")?;

    let status = child
        .wait()
        .into_diagnostic()
        .context("failed to wait for cshell explorer")?;

    if !status.success() {
        bail!("cshell explorer exited with code: {}", status);
    }

    Ok(())
}
