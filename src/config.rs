use miette::IntoDiagnostic as _;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub protocol: ProtocolConfig,
    pub registry: Option<RegistryConfig>,
    pub profiles: Option<ProfilesConfig>,
    pub bindings: Vec<BindingsConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtocolConfig {
    pub name: String,
    pub scope: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub main: PathBuf,
    pub readme: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryConfig {
    pub url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://tx3.land".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfilesConfig {
    pub devnet: ProfileConfig,
    pub preview: Option<ProfileConfig>,
    pub preprod: Option<ProfileConfig>,
    pub mainnet: Option<ProfileConfig>,
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        Self {
            devnet: KnownChain::CardanoDevnet.into(),
            preview: Some(KnownChain::CardanoPreview.into()),
            preprod: Some(KnownChain::CardanoPreprod.into()),
            mainnet: Some(KnownChain::CardanoMainnet.into()),
        }
    }
}

impl From<KnownChain> for ProfileConfig {
    fn from(chain: KnownChain) -> Self {
        Self {
            chain: chain.clone(),
            env_file: None,
            wallets: match chain {
                KnownChain::CardanoDevnet => vec![
                    WalletConfig {
                        name: "alice".to_string(),
                        random_key: true,
                        initial_balance: 1000000000000000000,
                    },
                    WalletConfig {
                        name: "bob".to_string(),
                        random_key: true,
                        initial_balance: 1000000000000000000,
                    },
                ],
                _ => vec![],
            },
            trp: None,
            u5c: None,
        }
    }
}

const PUBLIC_PREVIEW_TRP_KEY: &str = "trp1ffyf88ugcyg6j6n3yuh";
const PUBLIC_PREPROD_TRP_KEY: &str = "trp1mtg35n2n9lv7yauanfa";
const PUBLIC_MAINNET_TRP_KEY: &str = "trp1lrnhzcax5064cgxsaup";

const PUBLIC_PREVIEW_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const PUBLIC_PREPROD_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const PUBLIC_MAINNET_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ProfileConfig {
    pub chain: KnownChain,
    pub env_file: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wallets: Vec<WalletConfig>,

    pub trp: Option<TrpConfig>,
    pub u5c: Option<U5cConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletConfig {
    pub name: String,
    pub random_key: bool,
    pub initial_balance: u64,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum KnownChain {
    CardanoMainnet,
    CardanoPreview,
    CardanoPreprod,
    CardanoDevnet,
}

impl Default for KnownChain {
    fn default() -> Self {
        Self::CardanoDevnet
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct TrpConfig {
    pub url: String,
    pub headers: HashMap<String, String>,
}

impl From<KnownChain> for TrpConfig {
    fn from(chain: KnownChain) -> Self {
        match chain {
            KnownChain::CardanoMainnet => Self {
                url: "https://cardano-mainnet.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_MAINNET_TRP_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoPreview => Self {
                url: "https://cardano-preview.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREVIEW_TRP_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoPreprod => Self {
                url: "https://cardano-preprod.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREPROD_TRP_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoDevnet => Self {
                url: "http://localhost:8164".to_string(),
                headers: HashMap::new(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct U5cConfig {
    pub url: String,
    pub headers: HashMap<String, String>,
}

impl From<KnownChain> for U5cConfig {
    fn from(chain: KnownChain) -> Self {
        match chain {
            KnownChain::CardanoMainnet => Self {
                url: "https://mainnet.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_MAINNET_U5C_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoPreview => Self {
                url: "https://preview.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREVIEW_U5C_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoPreprod => Self {
                url: "https://preprod.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREPROD_U5C_KEY.to_string(),
                )]),
            },
            KnownChain::CardanoDevnet => Self {
                url: "http://localhost:3000/u5c".to_string(),
                headers: HashMap::new(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BindingsConfig {
    pub plugin: String,
    pub output_dir: PathBuf,
    pub options: Option<HashMap<String, serde_json::Value>>,
}

impl Config {
    pub fn load(path: &PathBuf) -> miette::Result<Config> {
        let contents = std::fs::read_to_string(path).into_diagnostic()?;
        let config = toml::from_str(&contents).into_diagnostic()?;
        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> miette::Result<()> {
        let contents = toml::to_string_pretty(self).into_diagnostic()?;
        std::fs::write(path, contents).into_diagnostic()?;
        Ok(())
    }
}
