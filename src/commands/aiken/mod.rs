use clap::{Args as ClapArgs, Subcommand};
use miette::Result;

use crate::config::{ProfileConfig, RootConfig};

pub mod audit;
pub mod model;
pub mod provider;

pub use audit::run as run_audit;

#[derive(Subcommand)]
pub enum Command {
    /// Audit Aiken code for vulnerabilities using AI-assisted detection
    Audit(audit::Args),
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Command,
}

#[allow(unused_variables)]
pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> Result<()> {
    #[cfg(feature = "unstable")]
    {
        _run(args, config, profile)
    }
    #[cfg(not(feature = "unstable"))]
    {
        let _ = config;
        let _ = profile;
        Err(miette::miette!(
            "The aiken command is currently unstable and requires the `unstable` feature to be enabled."
        ))
    }
}

fn _run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> Result<()> {
    match args.command {
        Command::Audit(args) => run_audit(args, config, profile),
    }
}
