use tokio::{sync::OnceCell, task::JoinHandle};
use tracing::debug;

use crate::{Cli, Commands, global::TelemetryConfig};

mod client;
mod fingerprint;

pub use client::{CommandMetric, OtlpClient};

static TELEMETRY_CLIENT: OnceCell<OtlpClient> = OnceCell::const_new();

pub fn initialize_telemetry(config: &TelemetryConfig) -> miette::Result<()> {
    if !config.enabled {
        debug!("telemetry is disabled, skipping telemetry initialization");
        return Ok(());
    }

    let client = OtlpClient::setup(config);

    TELEMETRY_CLIENT
        .set(client)
        .map_err(|_| miette::miette!("Telemetry already initialized"))?;

    debug!("telemetry initialized");

    Ok(())
}

impl From<&Cli> for Option<CommandMetric> {
    fn from(cli: &Cli) -> Self {
        match cli.command {
            Commands::Build(_) => Some(CommandMetric::new("build")),
            Commands::Check(_) => Some(CommandMetric::new("check")),
            Commands::Codegen(_) => Some(CommandMetric::new("codegen")),
            Commands::Devnet(_) => Some(CommandMetric::new("devnet")),
            Commands::Explore(_) => Some(CommandMetric::new("explore")),
            Commands::Init(_) => Some(CommandMetric::new("init")),
            Commands::Invoke(_) => Some(CommandMetric::new("invoke")),
            Commands::Inspect(_) => Some(CommandMetric::new("inspect")),
            Commands::Test(_) => Some(CommandMetric::new("test")),
            Commands::Identities(_) => Some(CommandMetric::new("identities")),
            Commands::Publish(_) => Some(CommandMetric::new("publish")),
            _ => None,
        }
    }
}

pub fn track_command_execution(call: &Cli) -> Option<JoinHandle<()>> {
    let Some(client) = TELEMETRY_CLIENT.get() else {
        debug!("skipping since not initialized");
        return None;
    };

    let metric: Option<CommandMetric> = call.into();

    let Some(metric) = metric else {
        debug!("skipping since command is not relevant for telemetry");
        return None;
    };

    debug!("submitting command telemetry");

    let handle = tokio::spawn(async move {
        let _ = client.send_metric(metric).await; // Silent failure
        debug!("telemetry sent");
    });

    Some(handle)
}
