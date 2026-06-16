use std::{
    collections::HashMap,
    path::Path,
    process::{Child, Command, Stdio},
};

use askama::Template;

use miette::{bail, Context as _, IntoDiagnostic as _};
use serde::{de, Deserialize, Deserializer, Serialize};

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

// Flat view of a wallet UTxO, holding just what the `expect` checks read:
// lovelace, native assets, and the datum hash. Built from the cshell wire shape
// below via `WireUtxo::into_utxo` — raw bytes are already base64-decoded here,
// so callers `hex::encode`/`from_utf8` them directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: Vec<u8>,
    pub output_coin: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Datum {
    pub hash: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UtxoAsset {
    pub policy_id: Vec<u8>,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UTxO {
    pub coin: String, // lovelace, kept as a string to sidestep overflow
    pub assets: Vec<UtxoAsset>,
    pub datum: Option<Datum>,
}

// ---- cshell `wallet utxos --output-format json` wire shape ----
//
// cshell emits utxorpc `AnyUtxoData` via pbjson: camelCase keys, `bytes` fields
// as base64, the `parsed_state` oneof flattened to its variant name (`cardano`),
// `uint64` as decimal strings, and `BigInt` as a nested oneof object
// (e.g. `{ "int": "2000000" }`). We deserialize that shape directly rather than
// reuse trix's `utxorpc` types, whose pinned spec models `coin` as a plain
// `uint64` and so won't parse cshell's BigInt-shaped coin.

#[derive(Debug, Deserialize)]
struct WalletUtxosOutput {
    #[serde(default)]
    utxos: Vec<WireUtxo>,
}

#[derive(Debug, Default, Deserialize)]
struct WireUtxo {
    // `parsed_state` oneof — the cardano variant carries everything we read.
    #[serde(default)]
    cardano: Option<WireCardano>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCardano {
    #[serde(default)]
    coin: Option<WireBigInt>,
    #[serde(default)]
    assets: Vec<WireMultiasset>,
    #[serde(default)]
    datum: Option<WireDatum>,
}

#[derive(Debug, Default, Deserialize)]
struct WireBigInt {
    // The plain `int` variant of utxorpc's BigInt oneof — lovelace fits here.
    // The `bigUInt`/`bigNInt` byte variants don't occur for ADA amounts.
    #[serde(default)]
    int: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMultiasset {
    #[serde(default)]
    policy_id: Option<String>,
    #[serde(default)]
    assets: Vec<WireAsset>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireAsset {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    output_coin: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireDatum {
    #[serde(default)]
    hash: Option<String>,
}

fn decode_b64(s: &str) -> miette::Result<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    STANDARD
        .decode(s)
        .into_diagnostic()
        .context("decoding base64 bytes in cshell wallet utxos output")
}

impl WireUtxo {
    fn into_utxo(self) -> miette::Result<UTxO> {
        let cardano = self.cardano.unwrap_or_default();

        let coin = cardano
            .coin
            .and_then(|b| b.int)
            .unwrap_or_else(|| "0".to_string());

        let mut assets = Vec::with_capacity(cardano.assets.len());
        for multiasset in cardano.assets {
            let policy_id = match multiasset.policy_id {
                Some(policy) => decode_b64(&policy)?,
                None => Vec::new(),
            };

            let mut inner = Vec::with_capacity(multiasset.assets.len());
            for asset in multiasset.assets {
                inner.push(Asset {
                    name: match asset.name {
                        Some(name) => decode_b64(&name)?,
                        None => Vec::new(),
                    },
                    output_coin: asset.output_coin.unwrap_or_else(|| "0".to_string()),
                });
            }

            assets.push(UtxoAsset {
                policy_id,
                assets: inner,
            });
        }

        let datum = match cardano.datum.and_then(|d| d.hash) {
            Some(hash) => Some(Datum {
                hash: decode_b64(&hash)?,
            }),
            None => None,
        };

        Ok(UTxO {
            coin,
            assets,
            datum,
        })
    }
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
    crate::spawn::ensure_supported("cshell")?;

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

pub fn wallet_utxos(home: &Path, wallet_name: &str, provider: &str) -> miette::Result<Vec<UTxO>> {
    let mut cmd = new_generic_command(home)?;

    // `cshell wallet utxos <NAME> <PROVIDER>` — the provider is positional and
    // falls back to cshell's *default* provider when omitted. The generated
    // cshell.toml marks its single provider `is_default = false`, so the expect
    // path must name it explicitly (the same one the invoke path submits to),
    // or cshell errors with "Wallet and provider not found".
    cmd.args([
        "wallet",
        "utxos",
        wallet_name,
        provider,
        "--output-format",
        "json",
    ]);

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .into_diagnostic()
        .context("running CShell wallet utxos")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "CShell failed to get wallet utxos for `{wallet_name}`: {}",
            stderr.trim()
        );
    }

    let parsed: WalletUtxosOutput = serde_json::from_slice(&output.stdout)
        .into_diagnostic()
        .context("parsing cshell wallet utxos output")?;

    parsed.utxos.into_iter().map(WireUtxo::into_utxo).collect()
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
