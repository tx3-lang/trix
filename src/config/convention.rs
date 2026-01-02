use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use miette::Result;

use crate::config::serde::NamedMap;

use super::model::*;

const PUBLIC_PREVIEW_TRP_KEY: &str = "trp1ffyf88ugcyg6j6n3yuh";
const PUBLIC_PREPROD_TRP_KEY: &str = "trp1mtg35n2n9lv7yauanfa";
const PUBLIC_MAINNET_TRP_KEY: &str = "trp1lrnhzcax5064cgxsaup";

const PUBLIC_PREVIEW_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const PUBLIC_PREPROD_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";
const PUBLIC_MAINNET_U5C_KEY: &str = "trpjodqbmjblunzpbikpcrl";

const KNOWN_NETWORKS: &[KnownNetwork] = &[
    KnownNetwork::CardanoMainnet,
    KnownNetwork::CardanoPreview,
    KnownNetwork::CardanoPreprod,
    KnownNetwork::CardanoLocal,
];

impl KnownNetwork {
    pub fn as_network_name(&self) -> &'static str {
        match self {
            KnownNetwork::CardanoMainnet => "cardano-mainnet",
            KnownNetwork::CardanoPreview => "cardano-preview",
            KnownNetwork::CardanoPreprod => "cardano-preprod",
            KnownNetwork::CardanoLocal => "cardano-local",
        }
    }
}

// TODO: once we introduce the concept of different "ledger families", we won't be able to infer the network just from the profile
impl From<KnownProfile> for KnownNetwork {
    fn from(value: KnownProfile) -> Self {
        match value {
            KnownProfile::Local => KnownNetwork::CardanoLocal,
            KnownProfile::Preview => KnownNetwork::CardanoPreview,
            KnownProfile::Preprod => KnownNetwork::CardanoPreprod,
            KnownProfile::Mainnet => KnownNetwork::CardanoMainnet,
        }
    }
}

impl KnownProfile {
    pub fn as_profile_name(&self) -> &'static str {
        match self {
            KnownProfile::Local => "local",
            KnownProfile::Preview => "preview",
            KnownProfile::Preprod => "preprod",
            KnownProfile::Mainnet => "mainnet",
        }
    }
}

const KNOWN_PROFILES: &[KnownProfile] = &[
    KnownProfile::Local,
    KnownProfile::Preview,
    KnownProfile::Preprod,
    KnownProfile::Mainnet,
];

impl ProfileConfig {
    pub fn env_file_path(&self) -> PathBuf {
        self.env_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(&format!(".env.{}", self.name)))
    }
}

const LOCAL_IDENTITIES: &[&str] = &[
    "alice", "bob", "charlie",
    //"dave", "eve", "frank", "george", "hannah", "ivy", "jack",
];

impl From<&str> for IdentityConfig {
    fn from(name: &str) -> Self {
        IdentityConfig::RandomKey(RandomKeyIdentityConfig {
            name: name.to_string(),
            random_key: true,
        })
    }
}

impl From<KnownProfile> for ProfileConfig {
    fn from(profile: KnownProfile) -> Self {
        Self {
            name: profile.as_profile_name().to_string(),
            network: KnownNetwork::from(profile).as_network_name().to_string(),
            env_file: None,
            identities: match profile {
                KnownProfile::Local => LOCAL_IDENTITIES
                    .iter()
                    .map(|name| (*name).into())
                    .collect::<NamedMap<IdentityConfig>>(),
                _ => NamedMap::default(),
            },
        }
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://tx3.land".to_string(),
        }
    }
}

impl From<KnownNetwork> for TrpConfig {
    fn from(network: KnownNetwork) -> Self {
        match network {
            KnownNetwork::CardanoMainnet => Self {
                url: "https://cardano-mainnet.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_MAINNET_TRP_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoPreview => Self {
                url: "https://cardano-preview.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREVIEW_TRP_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoPreprod => Self {
                url: "https://cardano-preprod.trp-m1.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREPROD_TRP_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoLocal => Self {
                url: "http://localhost:8164".to_string(),
                headers: HashMap::new(),
            },
        }
    }
}

impl From<KnownNetwork> for U5cConfig {
    fn from(network: KnownNetwork) -> Self {
        match network {
            KnownNetwork::CardanoMainnet => Self {
                url: "https://mainnet.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_MAINNET_U5C_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoPreview => Self {
                url: "https://preview.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREVIEW_U5C_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoPreprod => Self {
                url: "https://preprod.utxorpc-v0.demeter.run".to_string(),
                headers: HashMap::from([(
                    "dmtr-api-key".to_string(),
                    PUBLIC_PREPROD_U5C_KEY.to_string(),
                )]),
            },
            KnownNetwork::CardanoLocal => Self {
                url: "http://localhost:5164/u5c".to_string(),
                headers: HashMap::new(),
            },
        }
    }
}

impl From<KnownNetwork> for NetworkConfig {
    fn from(network: KnownNetwork) -> Self {
        Self {
            name: network.as_network_name().to_string(),
            trp: TrpConfig::from(network),
            u5c: U5cConfig::from(network),
            is_testnet: !matches!(network, KnownNetwork::CardanoMainnet),
        }
    }
}

impl From<NetworkOption> for NetworkConfig {
    fn from(network: NetworkOption) -> Self {
        match network {
            NetworkOption::Known(network) => NetworkConfig::from(network),
            NetworkOption::Custom(network) => network,
        }
    }
}
pub const KNOWN_CODEGEN_PLUGINS: &[KnownCodegenPlugin] = &[
    KnownCodegenPlugin::TsClient,
    KnownCodegenPlugin::RustClient,
    KnownCodegenPlugin::PythonClient,
    KnownCodegenPlugin::GoClient,
];

impl std::fmt::Display for KnownCodegenPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            KnownCodegenPlugin::TsClient => "ts-client",
            KnownCodegenPlugin::RustClient => "rust-client",
            KnownCodegenPlugin::PythonClient => "python-client",
            KnownCodegenPlugin::GoClient => "go-client",
        };

        write!(f, "{str}")
    }
}

impl From<KnownCodegenPlugin> for CodegenPluginConfig {
    fn from(plugin: KnownCodegenPlugin) -> Self {
        match plugin {
            KnownCodegenPlugin::TsClient => CodegenPluginConfig {
                repo: "tx3-lang/web-sdk".to_string(),
                // When web-sdk get updated, we need to change this path to bindgen/client-lib when we update the ref
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            KnownCodegenPlugin::RustClient => CodegenPluginConfig {
                repo: "tx3-lang/rust-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            KnownCodegenPlugin::PythonClient => CodegenPluginConfig {
                repo: "tx3-lang/python-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
            KnownCodegenPlugin::GoClient => CodegenPluginConfig {
                repo: "tx3-lang/go-sdk".to_string(),
                path: ".trix/client-lib".to_string(),
                r#ref: Some("bindgen-v1alpha2".to_string()),
            },
        }
    }
}

impl From<CodegenPlugin> for CodegenPluginConfig {
    fn from(plugin: CodegenPlugin) -> Self {
        match plugin {
            CodegenPlugin::Known(plugin) => CodegenPluginConfig::from(plugin),
            CodegenPlugin::Custom(plugin) => plugin,
        }
    }
}

impl CodegenPlugin {
    pub fn name(&self) -> String {
        match self {
            CodegenPlugin::Known(plugin) => plugin.to_string(),
            CodegenPlugin::Custom(plugin) => format!("custom-{}", plugin.repo),
        }
    }
}

impl CodegenConfig {
    pub fn job_id(&self) -> String {
        if let Some(explicit) = &self.job_id {
            return explicit.clone();
        }

        self.plugin.name()
    }

    pub fn output_dir(&self) -> miette::Result<PathBuf> {
        if let Some(explicit) = &self.output_dir {
            return Ok(explicit.clone());
        }

        let parent = crate::dirs::target_dir("codegen")?;
        let dir = parent.join(self.job_id());

        Ok(dir)
    }
}

impl RootConfig {
    pub fn available_networks(&self) -> HashSet<String> {
        let explicit: Vec<_> = self.networks.keys().cloned().collect();

        let implicit: Vec<_> = KNOWN_NETWORKS
            .iter()
            .map(|n| n.as_network_name().to_string())
            .collect();

        explicit.into_iter().chain(implicit).collect()
    }

    pub fn resolve_network(&self, network: &str) -> Result<NetworkConfig> {
        let explicit = self.networks.get(network);

        if let Some(explicit) = explicit {
            let config = NetworkConfig::from(explicit.clone());
            return Ok(config);
        }

        let implicit = KNOWN_NETWORKS
            .iter()
            .find(|n| n.as_network_name() == network);

        if let Some(implicit) = implicit {
            return Ok(NetworkConfig::from(*implicit));
        }

        Err(miette::miette!("Network not found"))
    }

    pub fn available_profiles(&self) -> HashSet<String> {
        let explicit: Vec<_> = self.profiles.keys().cloned().collect();

        let implicit: Vec<_> = KNOWN_PROFILES
            .iter()
            .map(|p| p.as_profile_name().to_string())
            .collect();

        explicit.into_iter().chain(implicit).collect()
    }

    pub fn resolve_profile(&self, profile: &str) -> Result<ProfileConfig> {
        let explicit = self.profiles.get(profile);

        if let Some(explicit) = explicit {
            return Ok(explicit.clone());
        }

        let implicit = KNOWN_PROFILES
            .iter()
            .find(|p| p.as_profile_name() == profile);

        if let Some(implicit) = implicit {
            return Ok(ProfileConfig::from(*implicit));
        }

        Err(miette::miette!("{profile} profile not found in config"))
    }

    pub fn resolve_profile_network(&self, profile: &str) -> Result<NetworkConfig> {
        let profile = self.resolve_profile(profile)?;

        let network = self.resolve_network(&profile.network)?;

        Ok(network)
    }
}
