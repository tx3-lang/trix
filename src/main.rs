use clap::{Parser, Subcommand};

mod commands;
mod config;
mod global;
mod home;
mod spawn;

use commands::{bindgen, build, check, devnet, init, inspect, publish, telemetry, test, wallet};
use config::Config;
use miette::{IntoDiagnostic as _, Result};

#[derive(Parser)]
#[command(name = "trix")]
#[command(about = "Package manager for the Tx3 language", long_about = None)]
#[command(version)]
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

    /// Inspect a Tx3 file
    Inspect(inspect::Args),

    /// Run a Tx3 testing file
    Test(test::Args),

    /// Build a Tx3 file
    Build(build::Args),

    /// Manage wallets
    Wallet(wallet::Args),

    /// Publish a Tx3 package into the registry (UNSTABLE - This feature is experimental and may change)
    #[command(hide = true)]
    Publish(publish::Args),

    /// Manage telemetry config
    Telemetry(telemetry::Args),
}

pub fn load_config() -> Result<Option<Config>> {
    let current_dir = std::env::current_dir().into_diagnostic()?;

    let config_path = current_dir.join("trix.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let config = Config::load(&config_path)?;

    Ok(Some(config))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config()?;

    global::ensure_global_config()?;

    match config {
        Some(config) => match cli.command {
            Commands::Init(args) => init::run(args, Some(&config)),
            Commands::Invoke(args) => devnet::invoke::run(args, &config),
            Commands::Devnet(args) => devnet::devnet::run(args, &config),
            Commands::Explore(args) => devnet::explore::run(args, &config),
            Commands::Bindgen(args) => bindgen::run(args, &config).await,
            Commands::Check(args) => check::run(args, &config),
            Commands::Inspect(args) => inspect::run(args, &config),
            Commands::Test(args) => test::run(args, &config),
            Commands::Build(args) => build::run(args, &config),
            Commands::Wallet(args) => wallet::run(args, &config),
            Commands::Publish(args) => publish::run(args, &config),
            Commands::Telemetry(args) => telemetry::run(args),
        },
        None => match cli.command {
            Commands::Init(args) => init::run(args, None),
            Commands::Telemetry(args) => telemetry::run(args),
            _ => Err(miette::miette!("No trix.toml found in current directory")),
        },
    }
}
