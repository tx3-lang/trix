use std::path::PathBuf;

use clap::Parser;

use trix::{
    cli::{Cli, Commands},
    commands as cmds,
    config::RootConfig,
    global, telemetry, updates,
};
use miette::{IntoDiagnostic as _, Result};

/// Walk up parent directories looking for a `trix.toml`, matching the same
/// convention as `dirs::protocol_root`. Returns the loaded config and the
/// on-disk path so callers (e.g. `trix codegen`) can save back to the same
/// file regardless of cwd.
pub fn load_config() -> Result<Option<(RootConfig, PathBuf)>> {
    let mut cwd = std::env::current_dir().into_diagnostic()?;

    loop {
        let candidate = cwd.join("trix.toml");
        if candidate.exists() {
            let config = RootConfig::load(&candidate)?;
            return Ok(Some((config, candidate)));
        }
        match cwd.parent() {
            Some(parent) => cwd = parent.to_path_buf(),
            None => return Ok(None),
        }
    }
}

fn run_global_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init(args) => cmds::init::run(args, None),
        Commands::Telemetry(args) => cmds::telemetry::run(args),
        Commands::Use(args) => {
            if args.no_init {
                return Err(miette::miette!(
                    "No trix.toml found and --no-init was passed; nothing to do"
                ));
            }
            let (config, path) = cmds::init::bootstrap_consumer_project()?;
            let profile = config.resolve_profile(&cli.profile)?;
            cmds::use_cmd::run(args, &config, &path, &profile)
        }
        _ => Err(miette::miette!("No trix.toml found in current directory")),
    }
}

async fn run_scoped_command(cli: Cli, config: RootConfig, config_path: PathBuf) -> Result<()> {
    let profile = config.resolve_profile(&cli.profile)?;

    let metric = telemetry::track_command_execution(&cli);

    let result = match cli.command {
        Commands::Init(args) => cmds::init::run(args, Some(&config)),
        Commands::Invoke(args) => cmds::invoke::run(args, &config, &profile),
        Commands::Devnet(args) => cmds::devnet::run(args, &config, &profile),
        Commands::Explore(args) => cmds::explore::run(args, &config, &profile),
        Commands::Codegen(args) => cmds::codegen::run(args, &config, &config_path, &profile).await,
        Commands::Check(args) => cmds::check::run(args, &config, &profile),
        Commands::Inspect(args) => cmds::inspect::run(args, &config),
        Commands::Test(args) => cmds::test::run(args, &config, &profile),
        Commands::Build(args) => cmds::build::run(args, &config, &profile),
        Commands::Identities(args) => cmds::identities::run(args, &config, &profile),
        Commands::Profile(args) => cmds::profile::run(args, &config, &profile),
        Commands::Publish(args) => cmds::publish::run(args, &config).await,
        Commands::Use(args) => cmds::use_cmd::run(args, &config, &config_path, &profile),
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

    let loaded = load_config()?;

    let global_config = global::ensure_global_config()?;

    if global_config.telemetry.enabled {
        telemetry::initialize_telemetry(&global_config.telemetry)?;
    }

    match loaded {
        Some((config, path)) => run_scoped_command(cli, config, path).await,
        None => run_global_command(cli),
    }
}
