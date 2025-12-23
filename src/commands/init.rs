use std::path::PathBuf;

use crate::config::{
    ActorConfig, BindingsConfig, BindingsTemplateConfig, Config, KeyConfig, ProfilesConfig,
    ProtocolConfig, RegistryConfig,
};
use clap::Args as ClapArgs;
use inquire::{Confirm, MultiSelect, Text};
use miette::{Context, IntoDiagnostic};

// Include template files at compile time
const TEMPLATE_MAIN_TX3: &str = include_str!("../../templates/tx3/main.tx3.tpl");
const TEMPLATE_TEST_TOML: &str = include_str!("../../templates/tx3/test.toml.tpl");
const DEFAULT_PROJECT_NAME: &str = "my-project";
const DEFAULT_ACTORS: [&str; 2] = ["alice", "bob"];
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

fn infer_devnet() -> crate::devnet::Config {
    let actors: Vec<_> = DEFAULT_ACTORS
        .iter()
        .map(|actor| ActorConfig {
            name: actor.to_string(),
            random_key: true,
            key_path: None,
        })
        .collect();

    let utxos = actors
        .iter()
        .map(|actor| {
            crate::devnet::UtxoSpec::Explicit(crate::devnet::ExplicitUtxoSpec {
                address: crate::devnet::AddressSpec::NamedWallet(actor.name.clone()),
                value: DEFAULT_DEVNET_WALLET_AMOUNT,
            })
        })
        .collect();

    crate::devnet::Config { utxos, actors }
}

fn apply(config: Config, devnet: crate::devnet::Config) -> miette::Result<()> {
    let devnet_toml = toml::to_string_pretty(&devnet).into_diagnostic()?;

    std::fs::write("devnet.toml", devnet_toml)
        .into_diagnostic()
        .context("writing devnet.toml")?;

    let trix_toml = toml::to_string_pretty(&config).into_diagnostic()?;

    std::fs::write("trix.toml", trix_toml)
        .into_diagnostic()
        .context("writing trix.toml")?;

    std::fs::write("main.tx3", TEMPLATE_MAIN_TX3)
        .into_diagnostic()
        .context("writing main.tx3")?;

    std::fs::create_dir_all("tests").into_diagnostic()?;

    std::fs::write("tests/basic.toml", TEMPLATE_TEST_TOML)
        .into_diagnostic()
        .context("writing tests/basic.toml")?;

    Ok(())
}

fn infer_keys() -> Vec<KeyConfig> {
    DEFAULT_ACTORS
        .iter()
        .map(|actor| KeyConfig {
            name: actor.to_string(),
            random: true,
            path: None,
        })
        .collect()
}

fn default_config() -> Config {
    Config {
        protocol: ProtocolConfig {
            name: infer_project_name(),
            scope: None,
            version: "0.0.0".into(),
            description: None,
            main: "main.tx3".into(),
            readme: None,
        },
        keys: infer_keys(),
        bindings: Vec::default(),
        profiles: ProfilesConfig::default().into(),
        registry: Some(RegistryConfig::default()),
    }
}

fn inquire_config(initial: &Config) -> miette::Result<Config> {
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

    let generate_bindings = MultiSelect::new(
        "Generate bindings for:",
        vec!["Typescript", "Rust", "Go", "Python"],
    )
    .prompt()
    .unwrap_or_default();

    let config = Config {
        protocol: ProtocolConfig {
            name: protocol_name,
            scope: owner_scope,
            version,
            description,
            main: "main.tx3".into(),
            readme: None,
        },
        bindings: generate_bindings
            .iter()
            .map(|binding| BindingsConfig {
                output_dir: PathBuf::from(format!("./gen/{}", binding.to_string().to_lowercase())),
                plugin: None, // Deprecated
                template: BindingsTemplateConfig::from_plugin(binding.to_lowercase().as_str()),
                options: None,
            })
            .collect(),
        profiles: ProfilesConfig::default().into(),
        registry: Some(RegistryConfig::default()),
        ..initial.clone()
    };

    let confirm = Confirm::new("Is this OK?")
        .with_default(true)
        .prompt()
        .into_diagnostic()?;

    if !confirm {
        return Err(miette::miette!("Nothing done"));
    }

    Ok(config)
}

#[derive(ClapArgs)]
pub struct Args {
    /// Use default configuration
    #[arg(short, long)]
    yes: bool,
}

pub fn run(args: Args, config: Option<&Config>) -> miette::Result<()> {
    let mut config = config.cloned().unwrap_or(default_config());

    if !args.yes {
        config = inquire_config(&config)?;
    };

    let devnet = infer_devnet();

    apply(config, devnet)?;

    Ok(())
}
