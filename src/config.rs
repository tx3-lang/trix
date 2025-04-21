use miette::IntoDiagnostic as _;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub protocol: ProtocolConfig,

    #[serde(default)]
    pub registry: RegistryConfig,

    #[serde(default)]
    pub profiles: ProfilesConfig,

    #[serde(default)]
    pub bindings: Vec<BindingsConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub name: String,
    pub scope: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub main: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfilesConfig {
    pub dev: ProfileConfig,
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        Self {
            dev: ProfileConfig {
                env_file: None,
                wallets: Vec::new(),
                network: NetworkConfig {
                    rpc_url: "http://localhost:8545".to_string(),
                    chain_id: 1337,
                },
                trp: TRPConfig {
                    url: "http://localhost:3000/trp".to_string(),
                },
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub env_file: Option<String>,
    pub wallets: Vec<WalletConfig>,
    pub network: NetworkConfig,
    pub trp: TRPConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletConfig {
    pub name: String,
    pub random_key: bool,
    pub initial_balance: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub chain_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TRPConfig {
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BindingsConfig {
    pub plugin: String,
    pub output_dir: PathBuf,
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

    pub fn default() -> Self {
        Self {
            protocol: ProtocolConfig {
                name: "my-project".to_string(),
                scope: None,
                version: "0.1.0".to_string(),
                description: None,
                main: PathBuf::from("main.tx3"),
            },
            registry: RegistryConfig::default(),
            profiles: ProfilesConfig::default(),
            bindings: Vec::new(),
        }
    }
}
