use crate::config::Config;
use clap::{Args as ClapArgs, Subcommand};

mod scope;
mod tir;

#[derive(Subcommand)]
pub enum Command {
    /// Inspect the global scope and available symbols in a tx3 file
    Scope(scope::Args),
    /// Inspect the intermediate representation of a transaction
    Tir(tir::Args),
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    match args.command {
        Command::Scope(_) => scope::run(config),
        Command::Tir(args) => tir::run(args, config),
    }
}
