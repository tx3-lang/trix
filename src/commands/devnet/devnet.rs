use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;
use std::{collections::HashMap, path::Path};

use crate::config::{Config, ProfileConfig};

#[derive(ClapArgs)]
pub struct Args {
    /// run devnet as a background process
    #[arg(short, long, default_value_t = false)]
    background: bool,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::commands::devnet::ensure_devnet_home(config)?;

    let mut daemon = crate::spawn::dolos::daemon(&devnet_home, args.background)?;

    if args.background {
        println!("devnet started in background");
    } else {
        let status = daemon
            .wait()
            .into_diagnostic()
            .context("failed to wait for dolos devnet")?;

        if !status.success() {
            bail!("dolos devnet exited with code: {}", status);
        }
    }

    Ok(())
}
