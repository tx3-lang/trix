use clap::{Parser, Subcommand};

mod commands;
mod config;
mod devnet;
mod dirs;
mod global;
mod home;
mod spawn;
mod updates;

use commands as cmds;
use config::Config;
use miette::{IntoDiagnostic as _, Result};

use crate::config::ProfileConfig;

#[derive(Parser)]
#[command(name = "trix")]
#[command(about = "Package manager for the Tx3 language", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, short, default_value = "devnet")]
    profile: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Tx3 project
    Init(cmds::init::Args),

    /// Invoke a transaction template
    Invoke(cmds::devnet::invoke::Args),

    /// Start development network (powered by Dolos)
    Devnet(cmds::devnet::Args),

    /// Explore a network (powered by CShell)
    Explore(cmds::devnet::explore::Args),

    /// Generate bindings for smart contracts
    Bindgen(cmds::bindgen::Args),

    /// Check a Tx3 package and all of its dependencies for errors
    Check(cmds::check::Args),

    /// Inspect a Tx3 file
    Inspect(cmds::inspect::Args),

    /// Run a Tx3 testing file
    Test(cmds::test::Args),

    /// Build a Tx3 file
    Build(cmds::build::Args),

    /// Manage wallets
    Wallet(cmds::wallet::Args),

    /// Publish a Tx3 package into the registry (UNSTABLE - This feature is experimental and may change)
    #[command(hide = true)]
    Publish(cmds::publish::Args),

    /// Telemetry configuration. Trix collects anonymous usage data to improve the tool.
    Telemetry(cmds::telemetry::Args),
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

pub fn load_profile(config: &Config, profile: &str) -> Result<ProfileConfig> {
    let profiles = config.profiles.get_by_name(profile);

    let profile = profiles.ok_or_else(|| miette::miette!("Profile not found"))?;

    Ok(profile.clone())
}

fn run_global_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init(args) => cmds::init::run(args, None),
        Commands::Telemetry(args) => cmds::telemetry::run(args),
        _ => Err(miette::miette!("No trix.toml found in current directory")),
    }
}

async fn run_scoped_command(cli: Cli, config: Config) -> Result<()> {
    let profile = load_profile(&config, &cli.profile)?;

    match cli.command {
        Commands::Init(args) => cmds::init::run(args, Some(&config)),
        Commands::Invoke(args) => cmds::devnet::invoke::run(args, &config, &profile),
        Commands::Devnet(args) => cmds::devnet::run(args, &config, &profile),
        Commands::Explore(args) => cmds::devnet::explore::run(args, &config, &profile),
        Commands::Bindgen(args) => cmds::bindgen::run(args, &config, &profile).await,
        Commands::Check(args) => cmds::check::run(args, &config),
        Commands::Inspect(args) => cmds::inspect::run(args, &config),
        Commands::Test(args) => cmds::test::run(args, &config, &profile),
        Commands::Build(args) => cmds::build::run(args, &config, &profile),
        Commands::Wallet(args) => cmds::wallet::run(args, &config, &profile),
        Commands::Publish(args) => cmds::publish::run(args, &config),
        Commands::Telemetry(args) => cmds::telemetry::run(args),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Check for updates silently
    let _ = updates::check_for_updates();

    let config = load_config()?;

    global::ensure_global_config()?;

    match config {
        Some(config) => run_scoped_command(cli, config).await,
        None => run_global_command(cli),
    }
}
