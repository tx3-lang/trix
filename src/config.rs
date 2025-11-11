use miette::IntoDiagnostic as _;
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub protocol: ProtocolConfig,
    pub registry: Option<RegistryConfig>,
    pub bindings: Vec<BindingsConfig>,

    #[serde(default)]
    pub profiles: ProfilesConfig,

    #[serde(default)]
    pub wallets: Vec<WalletConfig>,
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

#[derive(Debug, Serialize, Clone)]
pub struct ProfilesConfig(
    #[serde(serialize_with = "serialize_profiles")] HashMap<String, ProfileConfig>,
);

fn serialize_profiles<S>(
    profiles: &HashMap<String, ProfileConfig>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::Serialize;
    profiles.serialize(serializer)
}

fn deserialize_profiles<'de, D>(deserializer: D) -> Result<HashMap<String, ProfileConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut map = HashMap::<String, ProfileConfig>::deserialize(deserializer)?;

    for (key, value) in map.iter_mut() {
        value.name = key.clone();
    }

    Ok(map)
}

impl<'de> Deserialize<'de> for ProfilesConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = deserialize_profiles(deserializer)?;

        Ok(ProfilesConfig(map))
    }
}

impl ProfilesConfig {
    pub fn get_by_name(&self, profile: &str) -> Option<&ProfileConfig> {
        self.0.get(profile)
    }
}

impl Default for ProfilesConfig {
    fn default() -> Self {
        let map = KNOWN_CHAINS
            .iter()
            .map(|c| ProfileConfig::from(c.clone()))
            .map(|p| (p.name.clone(), p))
            .collect();

        Self(map)
    }
}

impl From<KnownChain> for ProfileConfig {
    fn from(chain: KnownChain) -> Self {
        Self {
            name: chain.as_profile_name().to_string(),
            chain: chain.clone(),
            env_file: None,
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
    #[serde(skip)]
    pub name: String,

    pub chain: KnownChain,
    pub env_file: Option<PathBuf>,
    pub trp: Option<TrpConfig>,
    pub u5c: Option<U5cConfig>,
}

impl ProfileConfig {
    pub fn env_file_path(&self) -> PathBuf {
        self.env_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(&format!(".env.{}", self.name)))
    }
}

pub fn load_profile_env_vars(profile: &ProfileConfig) -> miette::Result<HashMap<String, String>> {
    let path = profile.env_file_path();

    if !path.exists() {
        return Ok(HashMap::new());
    }

    dotenvy::from_path(path).into_diagnostic()?;

    let value = dotenvy::vars().collect::<HashMap<String, String>>();

    Ok(value)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WalletConfig {
    pub name: String,
    pub random_key: bool,
    pub key_path: Option<PathBuf>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum KnownChain {
    CardanoMainnet,
    CardanoPreview,
    CardanoPreprod,
    CardanoDevnet,
}

const KNOWN_CHAINS: &[KnownChain] = &[
    KnownChain::CardanoMainnet,
    KnownChain::CardanoPreview,
    KnownChain::CardanoPreprod,
    KnownChain::CardanoDevnet,
];

impl KnownChain {
    pub fn as_profile_name(&self) -> &'static str {
        match self {
            KnownChain::CardanoMainnet => "mainnet",
            KnownChain::CardanoPreview => "preview",
            KnownChain::CardanoPreprod => "preprod",
            KnownChain::CardanoDevnet => "devnet",
        }
    }
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
pub struct BindingsTemplateConfig {
    pub repo: String,
    pub path: String,
    pub r#ref: Option<String>, // default: main
}

impl Default for BindingsTemplateConfig {
    fn default() -> Self {
        Self {
            repo: String::new(),
            path: "bindgen".to_string(),
            r#ref: None,
        }
    }
}

impl BindingsTemplateConfig {
    // Unify the creation of BindingsTemplateConfig from plugin name
    pub fn from_plugin(plugin: &str) -> Self {
        // Reference should be updated on every release if is required
        match plugin {
            "typescript" => BindingsTemplateConfig {
                repo: "tx3-lang/web-sdk".to_string(),
                // When web-sdk get updated, we need to change this path to bindgen/client-lib when we update the ref
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            "rust" => BindingsTemplateConfig {
                repo: "tx3-lang/rust-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            "python" => BindingsTemplateConfig {
                repo: "tx3-lang/python-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            "go" => BindingsTemplateConfig {
                repo: "tx3-lang/go-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            _ => BindingsTemplateConfig::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BindingsConfig {
    // Deprecated field, use template instead
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    #[serde(default)]
    pub template: BindingsTemplateConfig,
    pub output_dir: PathBuf,
    pub options: Option<HashMap<String, serde_json::Value>>,
}

impl Config {
    pub fn load(path: &PathBuf) -> miette::Result<Config> {
        let contents = std::fs::read_to_string(path).into_diagnostic()?;
        let mut config: Config = toml::from_str(&contents).into_diagnostic()?;

        // Post-process bindings to handle backward compatibility
        // Eventually, this should be removed once deprecated plugin option is removed
        for binding in &mut config.bindings {
            if binding.template.repo.is_empty() && binding.plugin.is_some() {
                binding.template =
                    BindingsTemplateConfig::from_plugin(binding.plugin.as_ref().unwrap());
            }
        }

        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> miette::Result<()> {
        let contents = toml::to_string_pretty(self).into_diagnostic()?;
        std::fs::write(path, contents).into_diagnostic()?;
        Ok(())
    }
}
