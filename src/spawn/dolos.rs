use miette::{Context as _, IntoDiagnostic as _};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const DOLOS_TEMPLATE: &str = include_str!("../../templates/configs/dolos/dolos.toml");
pub const ALONZO_TEMPLATE: &str = include_str!("../../templates/configs/dolos/alonzo.json");
pub const BYRON_TEMPLATE: &str = include_str!("../../templates/configs/dolos/byron.json");
pub const CONWAY_TEMPLATE: &str = include_str!("../../templates/configs/dolos/conway.json");
pub const SHELLEY_TEMPLATE: &str = include_str!("../../templates/configs/dolos/shelley.json");

fn build_root_config(
    custom_utxos: Vec<dolos_core::config::CustomUtxo>,
) -> miette::Result<dolos_core::config::RootConfig> {
    let mut config: dolos_core::config::RootConfig = toml::from_str(DOLOS_TEMPLATE)
        .into_diagnostic()
        .context("parsing dolos.toml template")?;

    // Override chain config with custom UTXOs
    config.chain = dolos_core::config::ChainConfig::Cardano(dolos_core::config::CardanoConfig {
        custom_utxos,
        ..Default::default()
    });

    Ok(config)
}

fn save_config(home: &Path, name: &str, content: &str) -> miette::Result<PathBuf> {
    let config = home.join(name);

    std::fs::write(&config, content)
        .into_diagnostic()
        .context("saving config file")?;

    Ok(config)
}

pub fn initialize_config(
    home: &Path,
    custom_utxos: Vec<dolos_core::config::CustomUtxo>,
) -> miette::Result<PathBuf> {
    std::fs::create_dir_all(home).into_diagnostic()?;

    save_config(home, "byron.json", BYRON_TEMPLATE)?;
    save_config(home, "shelley.json", SHELLEY_TEMPLATE)?;
    save_config(home, "alonzo.json", ALONZO_TEMPLATE)?;
    save_config(home, "conway.json", CONWAY_TEMPLATE)?;

    let root_content = build_root_config(custom_utxos)?;
    let root_content = toml::to_string_pretty(&root_content).into_diagnostic()?;

    let root_path = save_config(home, "dolos.toml", &root_content)?;

    Ok(root_path)
}

pub fn daemon(home: &Path, silent: bool) -> miette::Result<Child> {
    let tool_path = crate::home::tool_path("dolos")?;

    let config_path = home.join("dolos.toml");

    let mut cmd = Command::new(tool_path.to_str().unwrap_or_default());

    cmd.args(["-c", config_path.to_str().unwrap(), "daemon"]);
    cmd.current_dir(home);

    if silent {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }

    let child = cmd
        .spawn()
        .into_diagnostic()
        .context("failed to spawn dolos devnet")?;

    Ok(child)
}
