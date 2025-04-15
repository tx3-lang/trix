use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub project: ProjectConfig,
    pub network: NetworkConfig,
    pub bindings: BindingsConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub rpc_url: String,
    pub chain_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BindingsConfig {
    pub output_dir: PathBuf,
    pub contracts: Vec<String>,
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Config> {
        let contents = std::fs::read_to_string(path)?;
        let config = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    pub fn default() -> Self {
        Self {
            project: ProjectConfig {
                name: "my-project".to_string(),
                version: "0.1.0".to_string(),
                description: None,
            },
            network: NetworkConfig {
                rpc_url: "http://localhost:8545".to_string(),
                chain_id: 1337,
            },
            bindings: BindingsConfig {
                output_dir: PathBuf::from("bindings"),
                contracts: Vec::new(),
            },
        }
    }
}
