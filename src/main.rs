use clap::{Parser, Subcommand};

mod commands;
mod config;

use commands::{bindgen, check, devnet, init};
use config::Config;
use miette::{IntoDiagnostic as _, Result};

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
    Invoke(devnet::invoke::Args),

    /// Start development network (powered by Dolos)
    Devnet(devnet::devnet::Args),

    /// Explore a network (powered by CShell)
    Explore(devnet::explore::Args),

    /// Generate bindings for smart contracts
    Bindgen(bindgen::Args),

    /// Check a Tx3 package and all of its dependencies for errors
    Check(check::Args),
}

pub fn load_config() -> Result<Config> {
    let current_dir = std::env::current_dir().into_diagnostic()?;

    let config_path = current_dir.join("trix.toml");

    if !config_path.exists() {
        miette::bail!("No trix.toml found in current directory");
    }

    Config::load(&config_path)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config()?;

    match cli.command {
        Commands::Init(args) => init::run(args, &config),
        Commands::Invoke(args) => devnet::invoke::run(args, &config),
        Commands::Devnet(args) => devnet::devnet::run(args, &config),
        Commands::Explore(args) => devnet::explore::run(args, &config),
        Commands::Bindgen(args) => bindgen::run(args, &config),
        Commands::Check(args) => check::run(args, &config),
    }
}
