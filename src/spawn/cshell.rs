use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use bip39::Mnemonic;
use cryptoxide::{digest::Digest, sha2::Sha256};

use miette::{Context as _, IntoDiagnostic as _, bail};
use serde::{Deserialize, Deserializer, de};

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

fn generate_deterministic_mnemonic(input: &str) -> miette::Result<Mnemonic> {
    let mut hasher = Sha256::new();
    hasher.input(input.as_bytes());
    let hash = hasher.result_str();

    let entropy = &hash[..16];

    Mnemonic::from_entropy(entropy.as_bytes()).into_diagnostic()
}

pub fn wallet_create(home: &Path, wallet_name: &str) -> miette::Result<serde_json::Value> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let mut cmd = Command::new(&tool_path);

    let mnemonic = generate_deterministic_mnemonic(wallet_name)?.to_string();

    cmd.args([
        "-s",
        config_path.to_str().unwrap_or_default(),
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
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let output = Command::new(&tool_path)
        .args([
            "-s",
            config_path.to_str().unwrap_or_default(),
            "wallet",
            "list",
            "--output-format",
            "json",
        ])
        .stdout(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet list")?;

    if !output.status.success() {
        bail!("CShell failed to list wallets");
    }

    serde_json::from_slice(&output.stdout).into_diagnostic()
}

pub fn transaction(
    home: &Path,
    tx3_file: &Path,
    tx3_args_json: &serde_json::Value,
    tx3_template: &str,
    signer: &str,
    r#unsafe: bool,
) -> miette::Result<serde_json::Value> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let mut cmd = Command::new(&tool_path);

    let unsafe_arg = if r#unsafe { "--unsafe" } else { "" };

    cmd.args([
        "-s",
        config_path.to_str().unwrap_or_default(),
        "tx",
        "new",
        "--tx3-file",
        tx3_file.to_str().unwrap(),
        "--tx3-args-json",
        serde_json::to_string(tx3_args_json).unwrap().as_str(),
        "--tx3-template",
        tx3_template,
        "--signer",
        signer,
        unsafe_arg,
        "--output-format",
        "json",
    ]);

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

pub fn transation_interactive(home: &Path, tx3_file: &Path) -> miette::Result<Child> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let child = Command::new(&tool_path)
        .args([
            "-s",
            config_path.to_str().unwrap_or_default(),
            "tx",
            "new",
            "--tx3-file",
            tx3_file.to_str().unwrap_or_default(),
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .into_diagnostic()
        .context("spawning CShell transaction interactive")?;

    Ok(child)
}

pub fn wallet_balance(home: &Path, wallet_name: &str) -> miette::Result<OutputBalance> {
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let output = Command::new(&tool_path)
        .args([
            "-s",
            config_path.to_str().unwrap_or_default(),
            "wallet",
            "balance",
            wallet_name,
            "--output-format",
            "json",
        ])
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
    let tool_path = crate::home::tool_path("cshell")?;

    let config_path = home.join("cshell.toml");

    let mut cmd = Command::new(&tool_path);

    cmd.args(["-s", config_path.to_str().unwrap_or_default(), "explorer"]);

    let child = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .into_diagnostic()
        .context("spawning CShell explorer")?;

    Ok(child)
}
