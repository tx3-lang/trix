use std::path::PathBuf;

use crate::config::{BindingsConfig, BindingsTemplateConfig, Config, ProfilesConfig, ProtocolConfig, RegistryConfig};
use clap::Args as ClapArgs;
use inquire::{Confirm, MultiSelect, Text};
use miette::IntoDiagnostic;

// Include template files at compile time
const TEMPLATE_MAIN_TX3: &str = include_str!("../templates/tx3/main.tx3.tpl");
const TEMPLATE_TEST_TOML: &str = include_str!("../templates/tx3/test.toml.tpl");

const DEFAULT_PROJECT_NAME: &str = "my-project";

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

fn apply(config: Config) -> miette::Result<()> {
    let toml_string = toml::to_string_pretty(&config).into_diagnostic()?;

    println!("\n{}", toml_string);

    std::fs::write("trix.toml", toml_string).into_diagnostic()?;
    std::fs::write("main.tx3", TEMPLATE_MAIN_TX3).into_diagnostic()?;
    std::fs::create_dir("tests").into_diagnostic()?;
    std::fs::write("tests/basic.toml", TEMPLATE_TEST_TOML).into_diagnostic()?;

    Ok(())
}

#[derive(ClapArgs)]
pub struct Args {
    /// Use default configuration
    #[arg(short, long)]
    yes: bool,
}

pub fn run(args: Args, config: Option<&Config>) -> miette::Result<()> {
    let default_name = infer_project_name();

    if args.yes {
        let config = Config {
            protocol: ProtocolConfig {
                name: default_name.clone(),
                scope: None,
                version: "0.0.0".into(),
                description: None,
                main: "main.tx3".into(),
                readme: None,
            },
            bindings: Vec::default(),
            profiles: ProfilesConfig::default().into(),
            registry: Some(RegistryConfig::default()),
        };

        return apply(config);
    }

    let protocol_name = prompt(
        "Protocol name:",
        Some(&default_name),
        config.map(|c| c.protocol.name.as_ref()),
    )
    .prompt()
    .into_diagnostic()?;

    let owner_scope = prompt(
        "Owner scope:",
        None,
        config.and_then(|c| c.protocol.scope.as_deref()),
    )
    .prompt_skippable()
    .into_diagnostic()?;

    let description = prompt(
        "Description:",
        None,
        config.and_then(|c| c.protocol.description.as_deref()),
    )
    .prompt_skippable()
    .into_diagnostic()?;

    let version = prompt(
        "Version:",
        Some("0.0.0"),
        config.map(|c| c.protocol.version.as_ref()),
    )
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
    };

    let confirm = Confirm::new("Is this OK?")
        .with_default(true)
        .prompt()
        .unwrap_or_default();

    if confirm {
        apply(config)?
    }

    Ok(())
}
