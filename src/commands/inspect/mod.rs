use crate::config::Config;
use clap::{Args as ClapArgs, Subcommand};

mod tir;

#[derive(Subcommand)]
pub enum Command {
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
        Command::Tir(args) => tir::run(args, config),
    }
}
