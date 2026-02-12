use clap::{Args as ClapArgs, Subcommand};
use miette::IntoDiagnostic;
use std::collections::BTreeMap;
use std::path::Path;

use crate::config::convention::{KNOWN_NETWORKS, KNOWN_PROFILES};
use crate::config::serde::Named;
use crate::config::RootConfig;

pub mod list;
pub mod show;

pub use list::run as run_list;
pub use show::run as run_show;

#[derive(Subcommand)]
pub enum Command {
    /// List all available profiles (built-in + custom)
    List,
    /// Show effective configuration for a specific profile
    Show(ShowArgs),
}

#[derive(ClapArgs)]
pub struct ListArgs;

#[derive(ClapArgs)]
pub struct ShowArgs {
    /// Profile name to inspect
    pub name: String,
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Command,
}

pub fn run(
    args: Args,
    config: &RootConfig,
    profile: &crate::config::ProfileConfig,
) -> miette::Result<()> {
    match args.command {
        Command::List => run_list(ListArgs, config, profile),
        Command::Show(args) => run_show(args, config, profile),
    }
}

// ============================================================================
// Shared View Model Data Structures
// ============================================================================

#[derive(Debug, Clone)]
pub enum ConfigSource {
    BuiltIn,
    Explicit,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::BuiltIn => write!(f, "built-in"),
            ConfigSource::Explicit => write!(f, "from trix.toml"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EnvFileStatus {
    Found,
    NotFound,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct EndpointView {
    pub url: String,
    pub url_source: ConfigSource,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct NetworkView {
    pub name: String,
    pub source: ConfigSource,
    pub is_testnet: bool,
    pub trp: EndpointView,
    pub u5c: EndpointView,
}

#[derive(Debug, Clone)]
pub struct IdentityView {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct EnvFileView {
    pub file_name: String,
    pub status: EnvFileStatus,
    pub variables: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ProfileView {
    pub name: String,
    pub source: ConfigSource,
    pub network: NetworkView,
    pub identities: Vec<IdentityView>,
    pub env_file: EnvFileView,
}

#[derive(Debug, Clone)]
pub struct ProfileListItem {
    pub name: String,
    pub source: ConfigSource,
    pub network: String,
    pub network_source: ConfigSource,
}

#[derive(Debug, Clone)]
pub struct NetworkListItem {
    pub name: String,
    pub source: ConfigSource,
}

#[derive(Debug, Clone)]
pub struct ProfileListView {
    pub profiles: Vec<ProfileListItem>,
    pub networks: Vec<NetworkListItem>,
}

// ============================================================================
// Source Resolution
// ============================================================================

pub(crate) fn resolve_profile_source(profile_name: &str, config: &RootConfig) -> ConfigSource {
    if config.profiles.contains_key(profile_name) {
        ConfigSource::Explicit
    } else {
        ConfigSource::BuiltIn
    }
}

pub(crate) fn resolve_network_source(network_name: &str, config: &RootConfig) -> ConfigSource {
    if config.networks.contains_key(network_name) {
        ConfigSource::Explicit
    } else {
        ConfigSource::BuiltIn
    }
}

// ============================================================================
// Utilities
// ============================================================================

pub(crate) fn mask_value(value: &str) -> String {
    if value.len() <= 8 {
        "***".to_string()
    } else {
        let first = &value[..4];
        let last = &value[value.len() - 4..];
        format!("{}...{}", first, last)
    }
}

pub(crate) fn should_mask_env_var(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("key")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("private")
}

pub(crate) fn load_and_mask_env_vars(path: &Path) -> miette::Result<Vec<(String, String)>> {
    use miette::{Context, IntoDiagnostic};

    let content = std::fs::read_to_string(path)
        .into_diagnostic()
        .context("Failed to read env file")?;

    let parsed: BTreeMap<String, String> = dotenv_parser::parse_dotenv(&content)
        .map_err(|e| miette::miette!("Failed to parse env file: {}", e))?;

    Ok(parsed
        .into_iter()
        .map(|(key, value)| {
            let display_value = if should_mask_env_var(&key) {
                mask_value(&value)
            } else {
                value
            };
            (key, display_value)
        })
        .collect())
}
