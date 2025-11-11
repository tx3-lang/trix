use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use bip39::Mnemonic;
use cryptoxide::{digest::Digest, sha2::Sha256};

use miette::{Context as _, IntoDiagnostic as _, bail};
use serde::{Deserialize, Deserializer, Serialize, de};

pub const CONFIG_TEMPLATE: &str = include_str!("../templates/configs/cshell/cshell.toml");

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

pub fn initialize_config(root: &Path) -> miette::Result<PathBuf> {
    let config_path = root.join("cshell.toml");

    std::fs::create_dir_all(root).into_diagnostic()?;

    std::fs::write(&config_path, CONFIG_TEMPLATE)
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
    tx3_file: &Path,
    tx3_args: &serde_json::Value,
    tx3_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
) -> miette::Result<Command> {
    let mut cmd = new_generic_command(home)?;

    cmd.args([
        "tx",
        "invoke",
        "--tx3-file",
        tx3_file.to_str().unwrap(),
        "--output-format",
        "json",
    ]);

    if let Some(tx3_template) = tx3_template {
        cmd.args(["--tx3-template", tx3_template]);
    }

    let tx3_args_json = serde_json::to_string(tx3_args).into_diagnostic()?;
    cmd.args(["--tx3-args-json", &tx3_args_json]);

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
    tx3_file: &Path,
    tx3_args: &serde_json::Value,
    tx3_template: Option<&str>,
    signers: Vec<&str>,
    r#unsafe: bool,
    skip_submit: bool,
) -> miette::Result<()> {
    let mut cmd = tx_invoke_cmd(
        home,
        tx3_file,
        tx3_args,
        tx3_template,
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
