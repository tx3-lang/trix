use clap::{Args as ClapArgs, Subcommand};

use crate::config::RootConfig;

mod imports;
mod tir;

#[derive(Subcommand)]
pub enum Command {
    /// Inspect types available from a plutus.json (CIP57) file
    Imports(imports::Args),

    /// Inspect the intermediate representation of a transaction
    Tir(tir::Args),
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    match args.command {
        Command::Imports(args) => imports::run(args, config),
        Command::Tir(args) => tir::run(args, config),
    }
}
