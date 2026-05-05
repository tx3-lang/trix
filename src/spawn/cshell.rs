use std::{
    collections::HashMap,
    path::Path,
    process::{Child, Command, Stdio},
};

use askama::Template;

use miette::{Context as _, IntoDiagnostic as _, bail};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::config::{TrpConfig, U5cConfig};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OutputWallet {
    pub name: String,
    pub addresses: OutputAddress,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OutputAddress {
    pub testnet: String,
}

#[allow(dead_code)]
fn string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    s.parse::<u64>().map_err(de::Error::custom)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct OutputBalance {
    #[serde(deserialize_with = "string_to_u64")]
    pub coin: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Asset {
    #[serde(with = "hex::serde")]
    pub name: Vec<u8>,
    pub output_coin: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Datum {
    #[serde(with = "hex::serde")]
    pub hash: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UtxoAsset {
    #[serde(with = "hex::serde")]
    pub policy_id: Vec<u8>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct UTxO {
    #[serde(with = "hex::serde")]
    pub tx: Vec<u8>,
    pub tx_index: u64,
    pub address: String,
    pub coin: String, // To avoid overflow
    pub assets: Vec<UtxoAsset>,
    pub datum: Option<Datum>,
}

pub struct Provider {
    pub name: String,
    pub u5c: U5cConfig,
    pub trp: TrpConfig,
    pub is_testnet: bool,
}

#[derive(Template)]
#[template(path = "configs/cshell/cshell.toml.askama")]
pub struct CshellTomlTemplate {
    pub provider: Provider,
}

fn new_generic_command(home: &Path) -> miette::Result<Command> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let mut cmd = Command::new(&tool_path);

    cmd.args(["-s", config_path.to_str().unwrap_or_default()]);

    Ok(cmd)
}

#[derive(Deserialize, Serialize)]
pub struct WalletInfoOutput {
    pub name: String,
    pub public_key: String,
    pub addresses: HashMap<String, String>,
}

pub fn wallet_info(home: &Path, wallet_name: &str) -> miette::Result<WalletInfoOutput> {
    let mut cmd = new_generic_command(home)?;

    cmd.args([
        "wallet",
        "info",
        "--name",
        wallet_name,
        "--output-format",
        "json",
    ])
    .stdout(Stdio::piped());

    let child = cmd
        .spawn()
        .into_diagnostic()
        .context("spawning CShell wallet info")?;

    let output = child
        .wait_with_output()
        .into_diagnostic()
        .context("running CShell wallet info")?;

    if !output.status.success() {
        bail!("CShell failed to get wallet info");
    }

    let output = serde_json::from_slice(&output.stdout).into_diagnostic()?;

    Ok(output)
}

pub fn wallet_create(home: &Path, name: &str, mnemonic: &str) -> miette::Result<serde_json::Value> {
    let mut cmd = new_generic_command(home)?;

    cmd.args([
        "wallet",
        "restore",
        "--name",
        name,
        "--mnemonic",
        mnemonic,
        "--unsafe",
        "--output-format",
        "json",
    ])
    .stdout(Stdio::piped());

    let child = cmd
        .spawn()
        .into_diagnostic()
        .context("spawning CShell wallet create")?;

    let output = child
        .wait_with_output()
        .into_diagnostic()
        .context("running CShell wallet create")?;

    if !output.status.success() {
        bail!("CShell failed to create wallet");
    }

    serde_json::from_slice(&output.stdout).into_diagnostic()
}

#[allow(dead_code)]
pub fn wallet_list(home: &Path) -> miette::Result<Vec<OutputWallet>> {
    let mut cmd = new_generic_command(home)?;

    let output = cmd
        .args(["wallet", "list", "--output-format", "json"])
        .stdout(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet list")?;

    if !output.status.success() {
        bail!("CShell failed to list wallets");
    }

    serde_json::from_slice(&output.stdout).into_diagnostic()
}

#[allow(clippy::too_many_arguments)]
pub fn tx_invoke_cmd(
    home: &Path,
    tii_file: &Path,
    tii_profile: Option<&str>,
    tx_template: Option<&str>,
    args: &serde_json::Value,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
    provider: Option<&str>,
) -> miette::Result<Command> {
    let mut cmd = new_generic_command(home)?;

    cmd.args([
        "tx",
        "invoke",
        "--tii-file",
        tii_file.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    if let Some(tii_profile) = tii_profile {
        cmd.args(["--profile", tii_profile]);
    }

    if let Some(tx_template) = tx_template {
        cmd.args(["--tx-template", tx_template]);
    }

    let args_json = serde_json::to_string(args).into_diagnostic()?;
    cmd.args(["--args-json", &args_json]);

    for signer in signers {
        cmd.args(["--signers", signer]);
    }

    if r#unsafe {
        cmd.args(["--unsafe"]);
    }

    if skip_submit {
        cmd.args(["--skip-submit"]);
    }

    if let Some(provider) = provider {
        cmd.args(["--provider", provider]);
    }

    Ok(cmd)
}

#[allow(clippy::too_many_arguments)]
pub fn tx_invoke_interactive(
    home: &Path,
    tii_file: &Path,
    tii_profile: Option<&str>,
    tx_template: Option<&str>,
    args: &serde_json::Value,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
    provider: Option<&str>,
) -> miette::Result<()> {
    let mut cmd = tx_invoke_cmd(
        home,
        tii_file,
        tii_profile,
        tx_template,
        args,
        signers,
        r#unsafe,
        skip_submit,
        provider,
    )?;

    cmd.stdout(Stdio::inherit())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .into_diagnostic()
        .context("running CShell transaction")?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn tx_invoke_json(
    home: &Path,
    tii_file: &Path,
    tii_profile: Option<&str>,
    args: &serde_json::Value,
    tx_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
    provider: Option<&str>,
) -> miette::Result<serde_json::Value> {
    let mut cmd = tx_invoke_cmd(
        home,
        tii_file,
        tii_profile,
        tx_template,
        args,
        signers,
        r#unsafe,
        skip_submit,
        provider,
    )?;

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .into_diagnostic()
        .context("running CShell transaction")?;

    if !output.status.success() {
        bail!("CShell failed to execute transaction");
    }

    serde_json::from_slice(&output.stdout).into_diagnostic()
}

#[allow(dead_code)]
pub fn wallet_balance(home: &Path, wallet_name: &str) -> miette::Result<OutputBalance> {
    let mut cmd = new_generic_command(home)?;

    let output = cmd
        .args(["wallet", "balance", wallet_name, "--output-format", "json"])
        .stdout(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet balance")?;

    if !output.status.success() {
        bail!("CShell failed to get wallet balance");
    }

    serde_json::from_slice(&output.stdout).into_diagnostic()
}

pub fn wallet_utxos(home: &Path, wallet_name: &str) -> miette::Result<Vec<UTxO>> {
    let mut cmd = new_generic_command(home)?;

    cmd.args(["wallet", "utxos", wallet_name, "--output-format", "json"]);

    let output = cmd
        .stdout(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet utxos")?;

    if !output.status.success() {
        bail!("CShell failed to get wallet utxos");
    }

    match serde_json::from_slice::<Vec<UTxO>>(&output.stdout) {
        Ok(list) => Ok(list),
        Err(_) => {
            let v: serde_json::Value = serde_json::from_slice(&output.stdout).into_diagnostic()?;
            if let Some(utxos_val) = v.get("utxos") {
                let list: Vec<UTxO> =
                    serde_json::from_value(utxos_val.clone()).into_diagnostic()?;
                Ok(list)
            } else if v.is_array() {
                let list: Vec<UTxO> = serde_json::from_value(v).into_diagnostic()?;
                Ok(list)
            } else {
                bail!("unexpected CShell wallet balance output shape")
            }
        }
    }
}

pub fn explorer(home: &Path, provider: &str) -> miette::Result<Child> {
    let mut cmd = new_generic_command(home)?;

    cmd.args(["explorer"]);
    cmd.args(["--provider", provider]);

    let child = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .into_diagnostic()
        .context("spawning CShell explorer")?;

    Ok(child)
}

/// Test connection to a provider
pub fn provider_test(home: &Path, provider: &str) -> miette::Result<()> {
    let mut cmd = new_generic_command(home)?;

    cmd.args(["provider", "test", "--name", provider]);

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell provider test")?;

    if !output.status.success() {
        bail!("CShell provider test failed");
    }

    Ok(())
}
