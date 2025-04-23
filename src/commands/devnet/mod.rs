use std::{collections::HashMap, fs, io::Write, path::PathBuf, process::Command};

use cryptoxide::{digest::Digest, sha2::Sha256};
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;
use serde::Serialize;

use crate::config::Config;

pub mod devnet;
pub mod explore;

const CSHELL: &str = include_str!("../templates/configs/cshell/cshell.toml");
const DOLOS: &str = include_str!("../templates/configs/dolos/dolos.toml");

const ALONZO: &str = include_str!("../templates/configs/dolos/alonzo.json");
const BYRON: &str = include_str!("../templates/configs/dolos/byron.json");
const CONWAY: &str = include_str!("../templates/configs/dolos/conway.json");
const SHELLEY: &str = include_str!("../templates/configs/dolos/shelley.json");

fn get_home_path() -> miette::Result<PathBuf> {
    let home_dir = if cfg!(target_os = "windows") {
        dirs::data_local_dir()
    } else {
        dirs::home_dir()
    }
    .context("Could not determine home directory")?;

    Ok(home_dir)
}

fn handle_devnet(home_path: &PathBuf, config: &Config) -> miette::Result<PathBuf> {
    let value = serde_json::to_vec(&config.profiles.dev.wallets).into_diagnostic()?;
    let mut hasher = Sha256::new();
    hasher.input(&value.as_slice());
    let hex = hasher.result_str();
    let truncated = &hex[..16];

    let mut tmp_path = home_path.clone();
    tmp_path.push(format!(".tx3"));
    if !tmp_path.exists() {
        bail!("run tx3 up cli to prepare the environment first")
    }
    tmp_path.push(format!("tmp/{truncated}_devnet"));

    if !tmp_path.exists() {
        let tmp_path_str = tmp_path.to_str().unwrap();

        fs::create_dir_all(&tmp_path)
            .into_diagnostic()
            .context("failed to create target directory")?;

        let mut cshell_config_path = tmp_path.clone();
        cshell_config_path.push("cshell.toml");

        let mut cshell_path = home_path.clone();
        if cfg!(target_os = "windows") {
            cshell_path.push(".tx3/default/bin/cshell.exe");
        } else {
            cshell_path.push(".tx3/default/bin/cshell");
        };

        let cshell = serde_json::to_value(toml::from_str::<toml::Value>(CSHELL).into_diagnostic()?)
            .into_diagnostic()?;
        write_file_toml(&format!("{tmp_path_str}/cshell.toml").into(), &cshell)?;

        let mut initial_funds = HashMap::new();
        for wallet in &config.profiles.dev.wallets {
            let mut cmd = Command::new(cshell_path.to_str().unwrap_or_default());
            cmd.args([
                "-s",
                cshell_config_path.to_str().unwrap_or_default(),
                "wallet",
                "create",
                "--name",
                &wallet.name,
                "--password",
                &wallet.name,
                "--output-format",
                "json",
            ]);
            let output = cmd
                .output()
                .into_diagnostic()
                .expect("fail to create devnet wallets");
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("cshell failed to create wallet:\n{}", stderr.trim());
            }

            let output: serde_json::Value =
                serde_json::from_slice(&output.stdout).into_diagnostic()?;

            let address = output
                .get("addresses")
                .context("missing 'addresses' field in cshell JSON output")?
                .get("testnet")
                .context("missing 'testnet' field in cshell 'addresses'")?
                .as_str()
                .unwrap();
            let address = Address::from_bech32(&address).into_diagnostic()?.to_hex();
            initial_funds.insert(address, wallet.initial_balance);
        }

        let mut shelley = serde_json::from_str::<serde_json::Value>(SHELLEY).into_diagnostic()?;
        map_shelley_initial_funds(initial_funds, &mut shelley)?;
        write_file(&format!("{tmp_path_str}/shelley.json").into(), &shelley)?;

        let mut dolos =
            serde_json::to_value(toml::from_str::<toml::Value>(DOLOS).into_diagnostic()?)
                .into_diagnostic()?;
        map_genisis_path(tmp_path_str, &mut dolos)?;
        write_file_toml(&format!("{tmp_path_str}/dolos.toml").into(), &dolos)?;

        let byron = serde_json::from_str::<serde_json::Value>(BYRON).into_diagnostic()?;
        write_file(&format!("{tmp_path_str}/byron.json").into(), &byron)?;

        let alonzo = serde_json::from_str::<serde_json::Value>(ALONZO).into_diagnostic()?;
        write_file(&format!("{tmp_path_str}/alonzo.json").into(), &alonzo)?;

        let conway = serde_json::from_str::<serde_json::Value>(CONWAY).into_diagnostic()?;
        write_file(&format!("{tmp_path_str}/conway.json").into(), &conway)?;
    }
    return Ok(tmp_path);
}

fn write_file<T>(path: &PathBuf, content: &T) -> miette::Result<()>
where
    T: ?Sized + Serialize,
{
    let mut new_file = fs::File::create(&path)
        .into_diagnostic()
        .context(format!("Failed to create file: {:?}", path))?;

    new_file
        .write_all(
            serde_json::to_vec_pretty(&content)
                .into_diagnostic()?
                .as_slice(),
        )
        .into_diagnostic()
        .context(format!("Failed to write to file: {:?}", path))?;

    Ok(())
}

fn write_file_toml<T>(path: &PathBuf, content: &T) -> miette::Result<()>
where
    T: ?Sized + Serialize,
{
    let toml_string = toml::to_string_pretty(content)
        .into_diagnostic()
        .context(format!("Failed to serialize TOML for file: {:?}", path))?;

    let mut new_file = fs::File::create(&path)
        .into_diagnostic()
        .context(format!("Failed to create file: {:?}", path))?;

    new_file
        .write_all(toml_string.as_bytes())
        .into_diagnostic()
        .context(format!("Failed to write to file: {:?}", path))?;

    Ok(())
}

fn map_genisis_path(path: &str, value: &mut serde_json::Value) -> miette::Result<()> {
    if let Some(genesis) = value.get_mut("genesis") {
        if let Some(obj) = genesis.as_object_mut() {
            obj.insert(
                "byron_path".into(),
                serde_json::Value::String(format!("{}/byron.json", path)),
            );
            obj.insert(
                "shelley_path".into(),
                serde_json::Value::String(format!("{}/shelley.json", path)),
            );
            obj.insert(
                "alonzo_path".into(),
                serde_json::Value::String(format!("{}/alonzo.json", path)),
            );
            obj.insert(
                "conway_path".into(),
                serde_json::Value::String(format!("{}/conway.json", path)),
            );
        } else {
            bail!("'genesis' is not a TOML object")
        }
    } else {
        bail!("missing 'genesis' field in dolos TOML")
    }

    Ok(())
}

fn map_shelley_initial_funds(
    initial_funds: HashMap<String, u64>,
    value: &mut serde_json::Value,
) -> miette::Result<()> {
    if let Some(obj) = value.get_mut("initialFunds") {
        if let Some(obj) = obj.as_object_mut() {
            for (address, balance) in initial_funds {
                obj.insert(
                    address.clone(),
                    serde_json::Value::Number(serde_json::Number::from(balance)),
                );
            }
        } else {
            bail!("'initialFunds' is not a JSON object")
        }
    } else {
        bail!("missing 'initialFunds' field in shelley JSON")
    }

    Ok(())
}
