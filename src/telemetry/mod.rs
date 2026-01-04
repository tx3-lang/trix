use tokio::sync::OnceCell;
use tracing::{debug, warn};

use crate::{global::TelemetryConfig, telemetry::fingerprint::get_user_fingerprint};

mod client;
mod fingerprint;

pub use client::{CommandSpan, OtlpClient};

static TELEMETRY_CLIENT: OnceCell<OtlpClient> = OnceCell::const_new();

pub fn initialize_telemetry(config: &TelemetryConfig) -> miette::Result<()> {
    if !config.enabled {
        debug!("telemetry is disabled, skipping telemetry initialization");
        return Ok(());
    }

    let Some(endpoint) = config.otlp_endpoint.as_ref() else {
        warn!("no OTLP endpoint configured, skipping telemetry initialization");
        return Ok(());
    };

    let user = get_user_fingerprint();

    let client = OtlpClient::new(endpoint.clone(), config.timeout_ms, user);

    TELEMETRY_CLIENT
        .set(client)
        .map_err(|_| miette::miette!("Telemetry already initialized"))
}

#[tracing::instrument]
pub fn track_command_execution(name: &str) {
    let Some(client) = TELEMETRY_CLIENT.get() else {
        debug!("skipping since not initialized");
        return;
    };

    let span = CommandSpan::new(name);
    debug!("submitting command telemetry");

    tokio::spawn(async move {
        let _ = client.send_span(span).await; // Silent failure
        debug!("telemetry sent");
    });
}
