use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};

use crate::config::Config;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::commands::devnet::ensure_devnet_home(config)?;

    let mut child = crate::spawn::cshell::explorer(&devnet_home)?;

    let status = child
        .wait()
        .into_diagnostic()
        .context("failed to wait for cshell explorer")?;

    if !status.success() {
        bail!("cshell explorer exited with code: {}", status);
    }

    Ok(())
}
