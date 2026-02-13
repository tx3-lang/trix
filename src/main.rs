use clap::{Parser, Subcommand};

mod builder;
mod commands;
mod config;
mod devnet;
mod dirs;
mod global;
mod home;
mod spawn;
mod telemetry;
mod updates;
mod wallet;

use commands as cmds;
use config::RootConfig;
use miette::{IntoDiagnostic as _, Result};

#[derive(Parser)]
#[command(name = "trix")]
#[command(about = "Package manager for the Tx3 language", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, short, default_value = "local", global = true)]
    profile: String,

    #[arg(long, short, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Tx3 project
    Init(cmds::init::Args),

    /// Invoke a transaction template
    Invoke(cmds::invoke::Args),

    /// Start development network (powered by Dolos)
    Devnet(cmds::devnet::Args),

    /// Explore a network (powered by CShell)
    Explore(cmds::explore::Args),

    /// Generate bindings for smart contracts
    Codegen(cmds::codegen::Args),

    /// Check a Tx3 package and all of its dependencies for errors
    Check(cmds::check::Args),

    /// Inspect a Tx3 file
    Inspect(cmds::inspect::Args),

    /// Run a Tx3 testing file
    Test(cmds::test::Args),

    /// Build a Tx3 file
    Build(cmds::build::Args),

    /// Manage crypographic identities
    Identities(cmds::identities::Args),

    /// Inspect and manage profiles
    Profile(cmds::profile::Args),

    /// Publish a Tx3 package into the registry (UNSTABLE - This feature is experimental and may change)
    #[command(hide = true)]
    Publish(cmds::publish::Args),

    /// Telemetry configuration. Trix collects anonymous usage data to improve the tool.
    Telemetry(cmds::telemetry::Args),
}

pub fn load_config() -> Result<Option<RootConfig>> {
    let current_dir = std::env::current_dir().into_diagnostic()?;

    let config_path = current_dir.join("trix.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let config = RootConfig::load(&config_path)?;

    Ok(Some(config))
}

fn run_global_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init(args) => cmds::init::run(args, None),
        Commands::Telemetry(args) => cmds::telemetry::run(args),
        _ => Err(miette::miette!("No trix.toml found in current directory")),
    }
}

async fn run_scoped_command(cli: Cli, config: RootConfig) -> Result<()> {
    let profile = config.resolve_profile(&cli.profile)?;

    let metric = crate::telemetry::track_command_execution(&cli);

    let result = match cli.command {
        Commands::Init(args) => cmds::init::run(args, Some(&config)),
        Commands::Invoke(args) => cmds::invoke::run(args, &config, &profile),
        Commands::Devnet(args) => cmds::devnet::run(args, &config, &profile),
        Commands::Explore(args) => cmds::explore::run(args, &config, &profile),
        Commands::Codegen(args) => cmds::codegen::run(args, &config, &profile).await,
        Commands::Check(args) => cmds::check::run(args, &config, &profile),
        Commands::Inspect(args) => cmds::inspect::run(args, &config),
        Commands::Test(args) => cmds::test::run(args, &config, &profile),
        Commands::Build(args) => cmds::build::run(args, &config, &profile),
        Commands::Identities(args) => cmds::identities::run(args, &config, &profile),
        Commands::Profile(args) => cmds::profile::run(args, &config, &profile),
        Commands::Publish(args) => cmds::publish::run(args, &config),
        Commands::Telemetry(args) => cmds::telemetry::run(args),
    };

    if let Some(handle) = metric {
        handle.await.unwrap();
    }

    result
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    // Check for updates silently
    let _ = updates::check_for_updates();

    let config = load_config()?;

    let global_config = global::ensure_global_config()?;

    if global_config.telemetry.enabled {
        telemetry::initialize_telemetry(&global_config.telemetry)?;
    }

    match config {
        Some(config) => run_scoped_command(cli, config).await,
        None => run_global_command(cli),
    }
}
