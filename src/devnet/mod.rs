use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Child,
    str::FromStr,
};

use miette::{Context as _, Diagnostic, IntoDiagnostic as _};

use inquire::Text;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use thiserror::Error;

use crate::{config::IdentityConfig, wallet::WalletProxy};

#[derive(Debug, Error, Diagnostic)]
#[error("devnet error")]
pub enum Error {
    #[error("can't open devnet config file")]
    #[diagnostic(help("Try running `trix devnet new` to create a devnet config file"))]
    CantOpenConfig(#[source] std::io::Error),

    #[error("invalid devnet config file: {0}")]
    #[diagnostic(help("Try fixing the devnet config file"))]
    InvalidConfig(#[source] toml::de::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum AddressSpec {
    NamedWallet(String),
    Address(String),
}

impl AddressSpec {
    pub fn resolve_address(&self, wallets: &HashMap<String, String>) -> miette::Result<String> {
        match self {
            AddressSpec::NamedWallet(name) => {
                let wallet = wallets
                    .get(name)
                    .ok_or(miette::miette!("Wallet {} not found", name))?;

                Ok(wallet.to_string())
            }
            AddressSpec::Address(address) => Ok(address.to_string()),
        }
    }
}

impl Display for AddressSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddressSpec::NamedWallet(name) => write!(f, "@{}", name),
            AddressSpec::Address(address) => write!(f, "{}", address),
        }
    }
}

impl FromStr for AddressSpec {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("@") {
            Ok(Self::NamedWallet(s[1..].to_string()))
        } else {
            Ok(Self::Address(s.to_string()))
        }
    }
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExplicitUtxoSpec {
    #[serde_as(as = "DisplayFromStr")]
    pub address: AddressSpec,
    pub value: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NativeBytesUtxoSpec {
    #[serde(rename = "ref")]
    pub r#ref: String,
    pub raw_bytes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum UtxoSpec {
    Explicit(ExplicitUtxoSpec),
    NativeBytes(NativeBytesUtxoSpec),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub utxos: Vec<UtxoSpec>,
}

impl Default for Config {
    fn default() -> Self {
        Self { utxos: vec![] }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> miette::Result<Self> {
        let data = std::fs::read_to_string(&path).map_err(Error::CantOpenConfig)?;

        let config = toml::from_str::<Self>(&data).map_err(Error::InvalidConfig)?;

        Ok(config)
    }
}

const DEFAULT_DEVNET_WALLET_AMOUNT: u64 = 100_000_000_000;

pub fn inquire_config(profile: &crate::config::ProfileConfig) -> miette::Result<Config> {
    let mut utxos = Vec::new();

    for (identity_name, _identity) in profile.identities.iter() {
        let balance_str = Text::new(&format!("Initial balance for '{}':", identity_name))
            .with_default(&DEFAULT_DEVNET_WALLET_AMOUNT.to_string())
            .prompt()
            .into_diagnostic()
            .context(format!("failed to read balance for {}", identity_name))?;

        let balance = balance_str
            .parse::<u64>()
            .map_err(|e| miette::miette!("Invalid balance: {}. Must be a valid number.", e))
            .context("parsing balance")?;

        utxos.push(UtxoSpec::Explicit(ExplicitUtxoSpec {
            address: AddressSpec::NamedWallet(identity_name.clone()),
            value: balance,
        }));
    }

    Ok(Config { utxos })
}

fn map_address(
    address: &AddressSpec,
    aliases: &HashMap<String, String>,
) -> miette::Result<pallas::ledger::addresses::Address> {
    let resolved = address.resolve_address(aliases)?;
    pallas::ledger::addresses::Address::from_bech32(&resolved).into_diagnostic()
}

fn dolos_utxo_from_explicit_spec(
    spec: &ExplicitUtxoSpec,
    aliases: &HashMap<String, String>,
) -> miette::Result<dolos_core::config::CustomUtxo> {
    let utxo = pallas::ledger::primitives::conway::TransactionOutput::PostAlonzo(
        pallas::codec::utils::KeepRaw::from(
            pallas::ledger::primitives::conway::PostAlonzoTransactionOutput {
                address: map_address(&spec.address, aliases)?.to_vec().into(),
                value: pallas::ledger::primitives::conway::Value::Coin(spec.value),
                // TODO: support this data from explicit spec
                datum_option: None,
                script_ref: None,
            },
        ),
    );

    let cbor = pallas::codec::minicbor::to_vec(&utxo).into_diagnostic()?;

    // TODO: use pallas::crypto::hash::Hasher to create a unique hash from the cbor bytes
    let hash = pallas::crypto::hash::Hasher::<256>::hash(&cbor);

    Ok(dolos_core::config::CustomUtxo {
        ref_: dolos_core::TxoRef(hash, 0),
        era: Some(pallas::ledger::traverse::Era::Conway.into()),
        cbor,
    })
}

fn dolos_utxo_from_spec(
    utxo: &UtxoSpec,
    aliases: &HashMap<String, String>,
) -> miette::Result<dolos_core::config::CustomUtxo> {
    match utxo {
        UtxoSpec::Explicit(x) => dolos_utxo_from_explicit_spec(x, aliases),
        UtxoSpec::NativeBytes(x) => Ok(dolos_core::config::CustomUtxo {
            ref_: x.r#ref.parse().map_err(|e: String| miette::miette!(e))?,
            cbor: hex::decode(&x.raw_bytes).into_diagnostic()?,
            era: Some(pallas::ledger::traverse::Era::Conway.into()),
        }),
    }
}

pub fn build_dolos_utxos(
    config: &Config,
    aliases: &HashMap<String, String>,
) -> miette::Result<Vec<dolos_core::config::CustomUtxo>> {
    config
        .utxos
        .iter()
        .map(|spec| dolos_utxo_from_spec(spec, aliases))
        .collect()
}

fn setup_home(devnet: &Config, ctx: &Context) -> miette::Result<PathBuf> {
    let hashable_content = serde_json::to_vec(&devnet).into_diagnostic()?;

    let devnet_home = crate::home::consistent_tmp_dir("devnet", &hashable_content)?;

    let initial_utxos = build_dolos_utxos(&devnet, &ctx.aliases)?;

    let _ = crate::spawn::dolos::initialize_config(&devnet_home, initial_utxos)?;

    Ok(devnet_home)
}

pub struct DevnetDaemon {
    pub home: PathBuf,
    pub daemon: Child,
}

pub struct Context {
    pub aliases: HashMap<String, String>,
}

impl Context {
    pub fn from_wallet(wallet: &WalletProxy) -> Self {
        Self {
            aliases: wallet.addresses.clone(),
        }
    }
}

pub fn start_daemon(devnet: &Config, ctx: &Context, silent: bool) -> miette::Result<DevnetDaemon> {
    let home = setup_home(&devnet, ctx)?;

    let daemon = crate::spawn::dolos::daemon(&home, silent)?;

    Ok(DevnetDaemon { home, daemon })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let address = AddressSpec::from_str("@alice").unwrap();
        assert_eq!(address, AddressSpec::NamedWallet("alice".to_string()));

        let address = AddressSpec::from_str("addr1abcdef").unwrap();
        assert_eq!(address, AddressSpec::Address("addr1abcdef".to_string()));
    }
}
