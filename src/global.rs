use miette::{Context, IntoDiagnostic};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub user_fingerprint: Option<String>,
    pub otlp_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self { 
            enabled: true,
            user_fingerprint: None,
            otlp_endpoint: None,
        }
    }
}

pub fn ensure_global_config() -> miette::Result<()> {
    let mut trix_path = crate::home::tx3_dir()?;
    trix_path.push("trix/config.toml");

    if !trix_path.exists() {
        std::fs::create_dir_all(trix_path.parent().unwrap()).into_diagnostic()?;
        let mut config = Config::default();
        // Generate user fingerprint when creating config for the first time
        config.telemetry.user_fingerprint = Some(crate::telemetry::generate_user_fingerprint()?);
        config.telemetry.otlp_endpoint = Some(crate::telemetry::DEFAULT_TELEMETRY_ENDPOINT.to_string());
        save_config(&config)?;
        print_telemetry_info();
    } else {
        // Ensure existing config has a user fingerprint and otlp_endpoint
        let mut config = read_config()?;
        if config.telemetry.user_fingerprint.is_none() {
            config.telemetry.user_fingerprint = Some(crate::telemetry::generate_user_fingerprint()?);
            save_config(&config)?;
        }
    }

    Ok(())
}

pub fn print_telemetry_info() {
    println!(
        "note: trix collects anonymous usage data to improve the tool.\nSee https://docs.txpipe.io/tx3/telemetry for details.\nTo disable this, run `trix telemetry off`.\n"
    );
}

pub fn read_config() -> miette::Result<Config> {
    let mut trix_path = crate::home::tx3_dir()?;
    trix_path.push("trix/config.toml");

    let trix_config = std::fs::read_to_string(&trix_path).into_diagnostic()?;
    let config = toml::from_str::<Config>(&trix_config)
        .into_diagnostic()
        .context(format!(
            "invalid trix global config. Fix or remove {}",
            trix_path.to_str().unwrap()
        ))?;

    Ok(config)
}

pub fn save_config(config: &Config) -> miette::Result<()> {
    let mut trix_path = crate::home::tx3_dir()?;
    trix_path.push("trix/config.toml");

    let toml_str = toml::to_string_pretty(&config).into_diagnostic()?;

    std::fs::write(&trix_path, toml_str)
        .into_diagnostic()
        .context("saving trix config.toml file")?;

    Ok(())
}
