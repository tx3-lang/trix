use crate::config::Config;
use clap::{Args as ClapArgs, Subcommand};

mod ir;

#[derive(Subcommand)]
pub enum Command {
    Ir(ir::Args),
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    match args.command {
        Command::Ir(args) => ir::run(args, config),
    }
}
