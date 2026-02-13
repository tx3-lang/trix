use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

use crate::config::serde::{KnownOrCustom, Named, NamedMap};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtocolConfig {
    pub name: String,
    pub scope: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub main: PathBuf,
    pub readme: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum KnownLedgerFamily {
    Cardano,
    Bitcoin,
    Midnight,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LedgerConfig {
    pub family: KnownLedgerFamily,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegistryConfig {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExplicitKeyIdentityConfig {
    #[serde(skip)]
    pub name: String,

    pub key_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RandomKeyIdentityConfig {
    #[serde(skip)]
    pub name: String,

    pub random_key: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum IdentityConfig {
    RandomKey(RandomKeyIdentityConfig),
    ExplicitKey(ExplicitKeyIdentityConfig),
}

impl Named for IdentityConfig {
    fn name(&self) -> String {
        match self {
            IdentityConfig::RandomKey(config) => config.name.clone(),
            IdentityConfig::ExplicitKey(config) => config.name.clone(),
        }
    }

    fn set_name(&mut self, name: String) {
        match self {
            IdentityConfig::RandomKey(config) => config.name = name,
            IdentityConfig::ExplicitKey(config) => config.name = name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum KnownProfile {
    #[default]
    Local,
    Preview,
    Preprod,
    Mainnet,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileConfig {
    #[serde(skip)]
    pub name: String,

    pub network: String,

    #[serde(default)]
    pub env_file: Option<PathBuf>,

    #[serde(default, skip_serializing_if = "NamedMap::is_empty")]
    pub identities: NamedMap<IdentityConfig>,
}

impl Named for ProfileConfig {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum KnownNetwork {
    CardanoMainnet,
    CardanoPreview,
    CardanoPreprod,
    #[default]
    CardanoLocal,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct TrpConfig {
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct U5cConfig {
    pub url: String,

    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct NetworkConfig {
    #[serde(skip)]
    pub name: String,

    pub is_testnet: bool,
    pub trp: TrpConfig,
    pub u5c: U5cConfig,
}

pub type NetworkOption = KnownOrCustom<KnownNetwork, NetworkConfig>;

impl Named for NetworkOption {
    fn name(&self) -> String {
        match self {
            NetworkOption::Known(network) => network.as_network_name().to_string(),
            NetworkOption::Custom(network) => network.name.clone(),
        }
    }

    fn set_name(&mut self, name: String) {
        match self {
            NetworkOption::Known(_) => (), // do nothing
            NetworkOption::Custom(x) => x.name = name,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodegenPluginConfig {
    pub repo: String,
    pub path: String,
    pub r#ref: Option<String>, // default: main
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::enum_variant_names)]
pub enum KnownCodegenPlugin {
    TsClient,
    RustClient,
    PythonClient,
    GoClient,
}

pub type CodegenPlugin = KnownOrCustom<KnownCodegenPlugin, CodegenPluginConfig>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodegenConfig {
    pub job_id: Option<String>,
    pub plugin: CodegenPlugin,
    pub output_dir: Option<PathBuf>,
    pub options: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RootConfig {
    pub protocol: ProtocolConfig,

    pub ledger: LedgerConfig,

    #[serde(default)]
    pub registry: Option<RegistryConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codegen: Vec<CodegenConfig>,

    #[serde(default, skip_serializing_if = "NamedMap::is_empty")]
    pub networks: NamedMap<NetworkOption>,

    #[serde(default, skip_serializing_if = "NamedMap::is_empty")]
    pub profiles: NamedMap<ProfileConfig>,
}
