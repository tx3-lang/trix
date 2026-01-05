use clap::{Args as ClapArgs, Subcommand};
use miette::{Context, IntoDiagnostic, bail};
use std::path::PathBuf;

use crate::config::{ProfileConfig, RootConfig};
use crate::devnet::Config as DevnetConfig;

pub mod copy;
pub mod new;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Retrieve the UTxO dependencies for one transaction
    Copy(copy::Args),
    /// Create a new devnet configuration file
    New(new::Args),
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    #[clap(subcommand)]
    command: Option<Command>,

    /// Path to the devnet config file
    #[arg(long)]
    config: Option<PathBuf>,

    /// run devnet as a background process
    #[arg(short, long, default_value_t = false)]
    background: bool,
}

pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    match args.command {
        Some(Command::Copy(args)) => copy::run(args, config, profile),
        Some(Command::New(args)) => new::run(args, config, profile),
        None => run_devnet(args, config, profile),
    }
}

pub fn run_devnet(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    let path = match args.config {
        Some(path) => path,
        None => crate::dirs::protocol_root()?.join("devnet.toml"),
    };

    let wallet = crate::wallet::setup(config, profile)?;

    let devnet = DevnetConfig::load(&path)?;

    let ctx = crate::devnet::Context::from_wallet(&wallet);

    let mut daemon = crate::devnet::start_daemon(&devnet, &ctx, args.background)?;

    if args.background {
        println!("devnet started in background");
    } else {
        let status = daemon
            .daemon
            .wait()
            .into_diagnostic()
            .context("failed to wait for dolos devnet")?;

        if !status.success() {
            bail!("dolos devnet exited with code: {}", status);
        }
    }

    Ok(())
}
