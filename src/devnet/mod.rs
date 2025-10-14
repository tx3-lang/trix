use std::{collections::HashMap, fmt::Display, path::Path, str::FromStr};

use miette::IntoDiagnostic as _;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum UtxoSpec {
    Value(UtxoSpecValue),
    Bytes(UtxoSpecBytes),
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UtxoSpecValue {
    #[serde_as(as = "DisplayFromStr")]
    pub address: AddressSpec,
    pub value: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UtxoSpecBytes {
    #[serde(rename = "ref")]
    pub r#ref: String,
    pub raw_bytes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub utxos: Vec<UtxoSpec>,
}

impl Config {
    pub fn iter_utxos_values(
        &self,
        wallets: &HashMap<String, String>,
    ) -> impl Iterator<Item = miette::Result<(String, u64)>> {
        self.utxos.iter().filter_map(|utxo| {
            match utxo {
                UtxoSpec::Value(v) => {
                    let address = v.address.resolve_address(wallets).ok()?;
                    Some(Ok((address, v.value)))
                },
                UtxoSpec::Bytes(_) => None,
            }
        })
    }

    pub fn iter_utxos_bytes(&self) -> miette::Result<Vec<(String, Vec<u8>)>> {
        self.utxos.iter().filter_map(|utxo| {
            match utxo {
                UtxoSpec::Value(_) => None,
                UtxoSpec::Bytes(b) => Some(Ok((b.r#ref.clone(), hex::decode(&b.raw_bytes).into_diagnostic().ok()?))),
            }
        }).collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self { utxos: vec![] }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> miette::Result<Self> {
        let data = std::fs::read_to_string(path).into_diagnostic()?;
        let config = toml::from_str::<Self>(&data).into_diagnostic()?;
        Ok(config)
    }
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

    #[test]
    fn test_config_serde() {
        // load test data file ./test_data/devnet.toml
        let config = Config::load("./src/devnet/test_data/devnet.toml").unwrap();

        let wallets = HashMap::from_iter(vec![
            ("alice".to_string(), "addr1aaa".to_string()),
            ("bob".to_string(), "addr1bbb".to_string()),
            ("charlie".to_string(), "addr1ccc".to_string()),
        ]);

        let all = config
            .iter_utxos_values(&wallets)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let map: HashMap<String, u64> = HashMap::from_iter(all);

        assert_eq!(map.get("addr1aaa").unwrap(), &1000);
        assert_eq!(map.get("addr1bbb").unwrap(), &2000);
        assert_eq!(map.get("addr1ccc").unwrap(), &3000);
        assert_eq!(map.get("addr1ddd").unwrap(), &4000);
    }
}
