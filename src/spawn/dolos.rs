use miette::{Context as _, IntoDiagnostic as _};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const DOLOS_TEMPLATE: &str = include_str!("../templates/configs/dolos/dolos.toml");
pub const ALONZO_TEMPLATE: &str = include_str!("../templates/configs/dolos/alonzo.json");
pub const BYRON_TEMPLATE: &str = include_str!("../templates/configs/dolos/byron.json");
pub const CONWAY_TEMPLATE: &str = include_str!("../templates/configs/dolos/conway.json");
pub const SHELLEY_TEMPLATE: &str = include_str!("../templates/configs/dolos/shelley.json");

fn initialize_shelley_config(initial_funds: &HashMap<String, u64>) -> miette::Result<String> {
    let mut original: Value = serde_json::from_str(SHELLEY_TEMPLATE)
        .into_diagnostic()
        .context("parsing shelley JSON")?;

    let object = original
        .get_mut("initialFunds")
        .context("missing 'initialFunds' field")?
        .as_object_mut()
        .context("'initialFunds' is not a JSON object")?;

    for (address, balance) in initial_funds {
        object.insert(
            address.clone(),
            serde_json::Value::Number(serde_json::Number::from(*balance)),
        );
    }

    serde_json::to_string_pretty(&original)
        .into_diagnostic()
        .context("serializing shelley JSON")
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
    initial_funds: &HashMap<String, u64>,
) -> miette::Result<PathBuf> {
    std::fs::create_dir_all(home).into_diagnostic()?;

    save_config(home, "byron.json", BYRON_TEMPLATE)?;

    let shelley_content = initialize_shelley_config(initial_funds)?;
    save_config(home, "shelley.json", &shelley_content)?;

    save_config(home, "alonzo.json", ALONZO_TEMPLATE)?;

    save_config(home, "conway.json", CONWAY_TEMPLATE)?;

    save_config(home, "dolos.toml", DOLOS_TEMPLATE)
}

pub fn daemon(home: &Path, background: bool) -> miette::Result<Child> {
    let tool_path = crate::home::tool_path("dolos")?;

    let config_path = home.join("dolos.toml");

    let mut cmd = Command::new(tool_path.to_str().unwrap_or_default());

    cmd.args(["-c", config_path.to_str().unwrap(), "daemon"]);
    cmd.current_dir(home);

    if background {
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
