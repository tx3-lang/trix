use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

use crate::config::serde::{KnownOrCustom, Named, NamedMap};
use crate::refs::ProtocolRef;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtocolConfig {
    pub name: String,
    pub scope: Option<String>,
    pub version: String,
    pub description: Option<String>,
    pub main: PathBuf,
    pub readme: Option<PathBuf>,

    /// Optional path to a PNG logo (relative to `trix.toml`). When set,
    /// `trix publish` attaches it as an `image/png` OCI layer. See
    /// `design/005-protocol-logos.md` for the publisher contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<PathBuf>,

    /// Repository URL that owns this protocol (e.g.
    /// `https://github.com/acme/widget`). Required at publish time; the
    /// owner segment must equal `scope`. Surfaces as
    /// `org.opencontainers.image.source` on the published manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
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

/// Publisher trust tier. Mirrors the `land.tx3.protocol.publisher.kind`
/// annotation written by `trix publish` and the verification result
/// produced by `trix use`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublisherKind {
    /// Keyless OIDC publish from a GitHub Actions workflow, verified
    /// against Sigstore Fulcio with `repository`/`repository_owner` claims.
    GithubOidc,
    /// Local publish authenticated through the tx3 GitHub App device
    /// flow; provenance attestation is signed by the registry, not Fulcio.
    GithubApp,
}

/// User-declared trust for a published interface: "I trust this publisher
/// tier from this repo (optionally narrowed to a git ref) to keep
/// publishing this dependency." Absence means TOFU on first verify;
/// presence turns publisher drift into a hard error.
///
/// On-disk form is a compact string for the common cases:
///   "github-oidc:acme/widget"        — tier + repo
///   "github-oidc:acme/widget@main"   — tier + repo + git ref
///   "github-app:acme"                — tier + GH login (App-tier has no repo claim)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedPublisher {
    pub tier: PublisherKind,
    /// `owner/repo` for `GithubOidc`; GitHub login for `GithubApp` (no slash).
    pub repository: Option<String>,
    /// Optional narrower pin to a git ref (branch or tag).
    pub git_ref: Option<String>,
}

impl std::fmt::Display for TrustedPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tier = match self.tier {
            PublisherKind::GithubOidc => "github-oidc",
            PublisherKind::GithubApp => "github-app",
        };
        match (&self.repository, &self.git_ref) {
            (Some(repo), Some(r)) => write!(f, "{tier}:{repo}@{r}"),
            (Some(repo), None) => write!(f, "{tier}:{repo}"),
            (None, _) => f.write_str(tier),
        }
    }
}

impl std::str::FromStr for TrustedPublisher {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (tier_str, rest) = s
            .split_once(':')
            .ok_or_else(|| format!("expected '<tier>:<repo>[@ref]', got '{s}'"))?;
        let tier = match tier_str {
            "github-oidc" => PublisherKind::GithubOidc,
            "github-app" => PublisherKind::GithubApp,
            other => return Err(format!("unknown publisher tier '{other}'")),
        };
        let (repo, git_ref) = match rest.split_once('@') {
            Some((r, gr)) if !gr.is_empty() => (r, Some(gr.to_string())),
            Some((_, _)) => return Err(format!("empty git ref after '@' in '{s}'")),
            None => (rest, None),
        };
        if repo.is_empty() {
            return Err(format!("empty repository in '{s}'"));
        }
        match tier {
            PublisherKind::GithubOidc if !repo.contains('/') => {
                return Err(format!(
                    "github-oidc expects 'owner/repo', got '{repo}' in '{s}'"
                ));
            }
            PublisherKind::GithubApp if repo.contains('/') => {
                return Err(format!(
                    "github-app expects a bare GitHub login (no '/'), got '{repo}' in '{s}'"
                ));
            }
            _ => {}
        }
        Ok(TrustedPublisher {
            tier,
            repository: Some(repo.to_string()),
            git_ref,
        })
    }
}

impl Serialize for TrustedPublisher {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TrustedPublisher {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InterfaceEntry {
    /// Filled in by NamedMap deserialization with the [interfaces.<alias>] key.
    #[serde(skip)]
    pub alias: String,

    /// Canonical reference, e.g. "acme/widget:0.1.3". Always a
    /// ProtocolRef::Registry with a concrete version; aliases and "latest"
    /// are rejected on load (the file is a pinned lockfile).
    #[serde(rename = "ref")]
    pub reference: ProtocolRef,

    /// OCI manifest digest captured at `trix use` time.
    pub digest: String,

    /// Trusted publisher pin. Absent => TOFU on first verify, warn on drift.
    /// Present => verification must match exactly or fail hard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustedPublisher>,
}

impl Named for InterfaceEntry {
    fn name(&self) -> String {
        self.alias.clone()
    }
    fn set_name(&mut self, name: String) {
        self.alias = name;
    }
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

    #[serde(default, skip_serializing_if = "NamedMap::is_empty")]
    pub interfaces: NamedMap<InterfaceEntry>,
}
