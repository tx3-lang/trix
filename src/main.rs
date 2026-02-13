use clap::Parser;

use trix::{
    builder,
    cli::{Cli, Commands},
    commands as cmds,
    config::RootConfig,
    devnet, dirs, global, home, spawn, telemetry, updates, wallet,
};
use miette::{IntoDiagnostic as _, Result};

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

    let metric = telemetry::track_command_execution(&cli);

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
