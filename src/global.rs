use miette::{Context, IntoDiagnostic};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
}
impl Default for TelemetryConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

pub fn ensure_global_config() -> miette::Result<()> {
    let mut trix_path = crate::home::tx3_dir()?;
    trix_path.push("trix/config.toml");

    if !trix_path.exists() {
        std::fs::create_dir_all(trix_path.parent().unwrap()).into_diagnostic()?;

        let global_config = Config::default();
        let toml_str = toml::to_string_pretty(&global_config).into_diagnostic()?;

        std::fs::write(&trix_path, toml_str)
            .into_diagnostic()
            .context("saving trix config.toml file")?;

        println!(
            "note: trix collects anonymous usage data to improve the tool.\nSee https://docs.txpipe.io/tx3/telemetry for details.\nTo disable this, run `trix telemetry off`.\n"
        );
    }

    Ok(())
}
