use clap::{Parser, Subcommand};

mod commands;
mod config;

use commands::{bindgen, check, devnet, init, invoke};
use config::Config;
use miette::IntoDiagnostic;
use miette::Result;

#[derive(Parser)]
#[command(name = "trix")]
#[command(about = "Package manager for the Tx3 language", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Tx3 project
    Init(init::Args),

    /// Invoke a transaction template
    Invoke(invoke::Args),

    /// Start development network
    Devnet(devnet::Args),

    /// Generate bindings for smart contracts
    Bindgen(bindgen::Args),

    /// Check a Tx3 package and all of its dependencies for errors
    Check(check::Args),
}

fn load_config() -> miette::Result<Config> {
    let config_path = std::env::current_dir().into_diagnostic()?.join("trix.toml");
    if config_path.exists() {
        Config::load(&config_path)
    } else {
        Ok(Config::default())
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config()?;

    match cli.command {
        Commands::Init(args) => init::run(args, &config),
        Commands::Invoke(args) => invoke::run(args, &config),
        Commands::Devnet(args) => devnet::run(args, &config),
        Commands::Bindgen(args) => bindgen::run(args, &config),
        Commands::Check(args) => check::run(args, &config),
    }
}
