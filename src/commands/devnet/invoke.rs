use std::fs;
use std::process::{Command, Stdio};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};

use crate::config::Config;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::devnet::ensure_devnet_home(config)?;

    let cononical = config.protocol.main.canonicalize().into_diagnostic()?;

    if !cononical.is_file() {
        bail!(
            "The main protocol file is not a file: {}",
            cononical.display()
        );
    }

    let mut child = crate::spawn::cshell::transation_interactive(&devnet_home, &cononical)?;

    let status = child
        .wait()
        .into_diagnostic()
        .context("failed to wait for cshell explorer")?;

    if !status.success() {
        bail!("cshell explorer exited with code: {}", status);
    }

    Ok(())
}
