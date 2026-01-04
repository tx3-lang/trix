use std::path::{Path, PathBuf};

use crate::config::{
    CodegenConfig, CodegenPlugin, KNOWN_CODEGEN_PLUGINS, KnownLedgerFamily, LedgerConfig,
    ProfileConfig, ProtocolConfig, RootConfig, serde::NamedMap,
};
use clap::Args as ClapArgs;
use inquire::{MultiSelect, Text};
use miette::{Context, IntoDiagnostic};

// Include template files at compile time
const TEMPLATE_MAIN_TX3: &str = include_str!("../../templates/tx3/main.tx3.tpl");
const TEMPLATE_TEST_TOML: &str = include_str!("../../templates/tx3/test.toml.tpl");
const TEMPLATE_GITIGNORE: &str = include_str!("../../templates/tx3/.gitignore.tpl");
const DEFAULT_PROJECT_NAME: &str = "my-project";
const DEFAULT_DEVNET_WALLET_AMOUNT: u64 = 100_000_000_000;

fn infer_project_name() -> String {
    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(_) => return DEFAULT_PROJECT_NAME.to_string(),
    };

    let project_name = current_dir
        .file_name()
        .and_then(|f| f.to_str())
        .map(|s| s.to_string());

    project_name.unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string())
}

fn prompt<'a>(msg: &'a str, default: Option<&'a str>, initial: Option<&'a str>) -> Text<'a> {
    let mut prompt = Text::new(msg);

    if let Some(initial) = initial {
        prompt = prompt.with_initial_value(initial);
    } else if let Some(default) = default {
        prompt = prompt.with_default(default);
    }

    prompt
}

fn infer_devnet(profile: &ProfileConfig) -> crate::devnet::Config {
    let utxos = profile
        .identities
        .keys()
        .map(|key| {
            crate::devnet::UtxoSpec::Explicit(crate::devnet::ExplicitUtxoSpec {
                address: crate::devnet::AddressSpec::NamedWallet(key.clone()),
                value: DEFAULT_DEVNET_WALLET_AMOUNT,
            })
        })
        .collect();

    crate::devnet::Config { utxos }
}

fn apply_template_if_not_exists(path: impl Into<PathBuf>, template: &str) -> miette::Result<()> {
    let path = path.into();

    if !path.exists() {
        std::fs::write(&path, template)
            .into_diagnostic()
            .context(format!("writing template to {}", path.to_string_lossy()))?;
    }

    Ok(())
}

fn apply(config: RootConfig, devnet: Option<crate::devnet::Config>) -> miette::Result<()> {
    if let Some(devnet) = devnet {
        let devnet_toml = toml::to_string_pretty(&devnet).into_diagnostic()?;
        apply_template_if_not_exists("devnet.toml", &devnet_toml)?;
    }

    apply_template_if_not_exists(".gitignore", TEMPLATE_GITIGNORE)?;

    apply_template_if_not_exists("main.tx3", TEMPLATE_MAIN_TX3)?;

    std::fs::create_dir_all("tests").into_diagnostic()?;

    apply_template_if_not_exists("tests/basic.toml", TEMPLATE_TEST_TOML)?;

    let trix_toml = toml::to_string_pretty(&config).into_diagnostic()?;

    std::fs::write("trix.toml", &trix_toml)
        .into_diagnostic()
        .context("writing trix.toml")?;

    Ok(())
}

fn default_config() -> RootConfig {
    RootConfig {
        protocol: ProtocolConfig {
            name: infer_project_name(),
            scope: None,
            version: "0.0.0".into(),
            description: None,
            main: "main.tx3".into(),
            readme: None,
        },
        ledger: LedgerConfig {
            family: KnownLedgerFamily::Cardano,
        },
        codegen: Vec::new(),
        profiles: NamedMap::default(),
        networks: NamedMap::default(),
        registry: None,
    }
}

fn inquire_config(initial: &RootConfig) -> miette::Result<RootConfig> {
    let protocol_name = prompt("Protocol name:", None, Some(&initial.protocol.name))
        .prompt()
        .into_diagnostic()?;

    let owner_scope = prompt("Owner scope:", None, initial.protocol.scope.as_deref())
        .prompt_skippable()
        .into_diagnostic()?;

    let description = prompt(
        "Description:",
        None,
        initial.protocol.description.as_deref(),
    )
    .prompt_skippable()
    .into_diagnostic()?;

    let version = prompt("Version:", Some("0.0.0"), Some(&initial.protocol.version))
        .prompt()
        .into_diagnostic()?;

    let generate_bindings =
        MultiSelect::new("Generate bindings for:", KNOWN_CODEGEN_PLUGINS.to_vec())
            .prompt()
            .unwrap_or_default();

    let config = RootConfig {
        protocol: ProtocolConfig {
            name: protocol_name,
            scope: owner_scope,
            version,
            description,
            main: "main.tx3".into(),
            readme: None,
        },
        codegen: generate_bindings
            .iter()
            .map(|binding| CodegenConfig {
                plugin: CodegenPlugin::Known(*binding),
                job_id: None,
                output_dir: None,
                options: None,
            })
            .collect(),
        ..initial.clone()
    };

    Ok(config)
}

#[derive(ClapArgs)]
pub struct Args {
    /// Use default configuration
    #[arg(short, long)]
    yes: bool,
}

pub fn run(args: Args, config: Option<&RootConfig>) -> miette::Result<()> {
    crate::telemetry::track_command_execution("init");

    let mut config = config.cloned().unwrap_or(default_config());

    if !args.yes {
        config = inquire_config(&config)?;
    };

    let devnet = config
        .resolve_profile("local")
        .ok()
        .map(|x| infer_devnet(&x));

    apply(config, devnet)?;

    Ok(())
}
