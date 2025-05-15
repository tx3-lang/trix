use miette::{IntoDiagnostic as _, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Visitor};
use std::{
    collections::HashMap,
    fmt::{self, Display},
    path::PathBuf,
    str::FromStr,
};

const CARDANO_PREVIEW_PUBLIC_TRP_KEY: &str = "trp1ffyf88ugcyg6j6n3yuh";
const CARDANO_PREPROD_PUBLIC_TRP_KEY: &str = "trp1mtg35n2n9lv7yauanfa";
const CARDANO_MAINNET_PUBLIC_TRP_KEY: &str = "trp1lrnhzcax5064cgxsaup";

const CARDANO_PREVIEW_PUBLIC_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const CARDANO_PREPROD_PUBLIC_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const CARDANO_MAINNET_PUBLIC_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub protocol: ProtocolConfig,
    pub registry: Option<RegistryConfig>,
    pub profiles: Option<Vec<ProfileConfig>>,
    pub bindings: Vec<BindingsConfig>,
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

    pub fn devnet(&self) -> Option<ProfileConfig> {
        if let Some(profiles) = &self.profiles {
            return profiles
                .iter()
                .find(|p| p.chain.eq(&KnownChain::Devnet))
                .cloned();
        }
        None
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtocolConfig {
    pub name: String,
    pub scope: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub main: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryConfig {
    pub url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://registry.dgram.io".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ProfileConfig {
    pub chain: KnownChain,
    pub env_file: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wallets: Vec<WalletConfig>,

    pub trp: Option<TrpConfig>,
    pub u5c: Option<U5cConfig>,
}

impl From<KnownChain> for ProfileConfig {
    fn from(chain: KnownChain) -> Self {
        Self {
            chain: chain.clone(),
            env_file: None,
            wallets: match chain {
                KnownChain::Devnet => vec![
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletConfig {
    pub name: String,
    pub random_key: bool,
    pub initial_balance: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CardanoNetwork {
    Mainnet,
    Preprod,
    Preview,
}
impl CardanoNetwork {
    pub fn all() -> Vec<String> {
        vec![
            CardanoNetwork::Mainnet,
            CardanoNetwork::Preprod,
            CardanoNetwork::Preview,
        ]
        .into_iter()
        .map(|c| c.to_string())
        .collect()
    }
}
impl Display for CardanoNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            CardanoNetwork::Mainnet => "Mainnet",
            CardanoNetwork::Preprod => "Preprod",
            CardanoNetwork::Preview => "Preview",
        };
        write!(f, "{name}")
    }
}
impl FromStr for CardanoNetwork {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Mainnet" => Ok(Self::Mainnet),
            "Preprod" => Ok(Self::Preprod),
            "Preview" => Ok(Self::Preview),
            _ => bail!("invalid cardano network"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum KnownChain {
    #[default]
    Devnet,
    Cardano(CardanoNetwork),
}
impl KnownChain {
    pub fn all() -> Vec<String> {
        vec!["Cardano".into()]
    }

    pub fn network(chain: &str) -> Vec<String> {
        match chain {
            "Cardano" => CardanoNetwork::all(),
            _ => Vec::new(),
        }
    }
}

impl Display for KnownChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Devnet => "Devnet",
            Self::Cardano(_) => "Cardano",
        };
        write!(f, "{name}")
    }
}

impl FromStr for KnownChain {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Devnet" => Ok(Self::Devnet),
            "CardanoMainnet" => Ok(Self::Cardano(CardanoNetwork::Mainnet)),
            "CardanoPreview" => Ok(Self::Cardano(CardanoNetwork::Preview)),
            "CardanoPreprod" => Ok(Self::Cardano(CardanoNetwork::Preprod)),
            _ => bail!(format!("Unknown chain: {}", s)),
        }
    }
}

impl Serialize for KnownChain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = match self {
            KnownChain::Devnet => self.to_string(),
            KnownChain::Cardano(c) => format!("{self}{c}"),
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for KnownChain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KnownChainVisitor;

        impl Visitor<'_> for KnownChainVisitor {
            type Value = KnownChain;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string like 'Devnet' or 'CardanoPreprod'")
            }

            fn visit_str<E>(self, v: &str) -> Result<KnownChain, E>
            where
                E: serde::de::Error,
            {
                v.parse::<KnownChain>()
                    .map_err(|err| E::custom(err.to_string()))
            }

            fn visit_string<E>(self, v: String) -> Result<KnownChain, E>
            where
                E: serde::de::Error,
            {
                v.as_str()
                    .parse::<KnownChain>()
                    .map_err(|err| E::custom(err.to_string()))
            }
        }

        deserializer.deserialize_string(KnownChainVisitor)
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
            KnownChain::Devnet => Self {
                url: "http://localhost:3000/trp".to_string(),
                headers: HashMap::new(),
            },
            KnownChain::Cardano(chain) => match chain {
                CardanoNetwork::Mainnet => Self {
                    url: "https://cardano-mainnet.trp-m1.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_MAINNET_PUBLIC_TRP_KEY.to_string(),
                    )]),
                },
                CardanoNetwork::Preview => Self {
                    url: "https://cardano-preview.trp-m1.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_PREVIEW_PUBLIC_TRP_KEY.to_string(),
                    )]),
                },
                CardanoNetwork::Preprod => Self {
                    url: "https://cardano-preprod.trp-m1.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_PREPROD_PUBLIC_TRP_KEY.to_string(),
                    )]),
                },
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
            KnownChain::Devnet => Self {
                url: "http://localhost:3000/u5c".to_string(),
                headers: HashMap::new(),
            },
            KnownChain::Cardano(chain) => match chain {
                CardanoNetwork::Mainnet => Self {
                    url: "https://mainnet.utxorpc-v0.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_MAINNET_PUBLIC_U5C_KEY.to_string(),
                    )]),
                },
                CardanoNetwork::Preview => Self {
                    url: "https://preview.utxorpc-v0.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_PREVIEW_PUBLIC_U5C_KEY.to_string(),
                    )]),
                },
                CardanoNetwork::Preprod => Self {
                    url: "https://preprod.utxorpc-v0.demeter.run".to_string(),
                    headers: HashMap::from([(
                        "dmtr-api-key".to_string(),
                        CARDANO_PREPROD_PUBLIC_U5C_KEY.to_string(),
                    )]),
                },
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BindingsConfig {
    pub plugin: String,
    pub output_dir: PathBuf,
}
