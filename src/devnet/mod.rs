use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    process::Child,
    str::FromStr,
};

use miette::IntoDiagnostic as _;

use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

use crate::{config::IdentityConfig, wallet::WalletProxy};

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
    pub actors: Vec<IdentityConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            utxos: vec![],
            actors: vec![],
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> miette::Result<Self> {
        let data = std::fs::read_to_string(path).into_diagnostic()?;
        let config = toml::from_str::<Self>(&data).into_diagnostic()?;
        Ok(config)
    }
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
