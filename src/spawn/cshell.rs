use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use askama::Template;
use bip39::Mnemonic;
use cryptoxide::{digest::Digest, sha2::Sha256};

use miette::{Context as _, IntoDiagnostic as _, bail};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::config::ProfileConfig;

#[derive(Debug, Deserialize)]
pub struct OutputWallet {
    pub name: String,
    pub addresses: OutputAddress,
}

#[derive(Debug, Deserialize)]
pub struct OutputAddress {
    pub testnet: String,
}

fn string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    s.parse::<u64>().map_err(de::Error::custom)
}

#[derive(Debug, Deserialize)]
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

#[derive(Template)]
#[template(path = "configs/cshell/cshell.toml.askama")]
struct CshellTomlTemplate {
    profile: ProfileConfig,
}

impl CshellTomlTemplate {
    fn u5c_url(&self) -> &str {
        self.profile
            .u5c
            .as_ref()
            .map(|u5c| u5c.url.as_str())
            .unwrap_or("http://localhost:5164")
    }

    fn trp_url(&self) -> &str {
        self.profile
            .trp
            .as_ref()
            .map(|trp| trp.url.as_str())
            .unwrap_or("http://localhost:8164")
    }
}

pub fn initialize_config(root: &Path, profile: &ProfileConfig) -> miette::Result<PathBuf> {
    let config_path = root.join("cshell.toml");

    std::fs::create_dir_all(root).into_diagnostic()?;

    let template = CshellTomlTemplate {
        profile: profile.clone(),
    };
    let rendered = template.render().into_diagnostic()?;

    std::fs::write(&config_path, rendered)
        .into_diagnostic()
        .context("writing cshell config")?;

    Ok(config_path)
}

fn new_generic_command(home: &Path) -> miette::Result<Command> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let mut cmd = Command::new(&tool_path);

    cmd.args(["-s", config_path.to_str().unwrap_or_default()]);

    Ok(cmd)
}

fn generate_deterministic_mnemonic(input: &str) -> miette::Result<Mnemonic> {
    let mut hasher = Sha256::new();
    hasher.input(input.as_bytes());
    let hash = hasher.result_str();

    let entropy: [u8; 32] = hash[..32].as_bytes().try_into().unwrap();

    Mnemonic::from_entropy(&entropy).into_diagnostic()
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

pub fn wallet_create(home: &Path, wallet_name: &str) -> miette::Result<serde_json::Value> {
    let mut cmd = new_generic_command(home)?;

    let mnemonic = generate_deterministic_mnemonic(wallet_name)?.to_string();

    cmd.args([
        "wallet",
        "restore",
        "--name",
        wallet_name,
        "--mnemonic",
        &mnemonic,
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

pub fn tx_invoke_cmd(
    home: &Path,
    tii_file: &Path,
    args: &serde_json::Value,
    tx_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
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

    // println!("{:#?}", cmd);

    Ok(cmd)
}

pub fn tx_invoke_interactive(
    home: &Path,
    tii_file: &Path,
    args: &serde_json::Value,
    tx_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
) -> miette::Result<()> {
    let mut cmd = tx_invoke_cmd(
        home,
        tii_file,
        args,
        tx_template,
        signers,
        r#unsafe,
        skip_submit,
    )?;

    cmd.stdout(Stdio::inherit())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .into_diagnostic()
        .context("running CShell transaction")?;

    Ok(())
}

pub fn tx_invoke_json(
    home: &Path,
    tx3_file: &Path,
    tx3_args: &serde_json::Value,
    tx3_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
) -> miette::Result<serde_json::Value> {
    let mut cmd = tx_invoke_cmd(
        home,
        tx3_file,
        tx3_args,
        tx3_template,
        signers,
        r#unsafe,
        skip_submit,
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

    dbg!(&cmd);

    let output = cmd
        .stdout(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet utxos")?;

    dbg!(&output);

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
            } else {
                if v.is_array() {
                    let list: Vec<UTxO> = serde_json::from_value(v).into_diagnostic()?;
                    Ok(list)
                } else {
                    bail!("unexpected CShell wallet balance output shape")
                }
            }
        }
    }
}

pub fn explorer(home: &Path) -> miette::Result<Child> {
    let mut cmd = new_generic_command(home)?;

    cmd.args(["explorer"]);

    let child = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .into_diagnostic()
        .context("spawning CShell explorer")?;

    Ok(child)
}
