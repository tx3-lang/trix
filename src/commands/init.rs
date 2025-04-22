use std::path::PathBuf;

use crate::config::{Config, ProtocolConfig, BindingsConfig};
use clap::Args as ClapArgs;
use inquire::{Confirm, MultiSelect, Text};
use miette::IntoDiagnostic;
use serde::Serialize;

// Include template files at compile time
const TEMPLATE_MAIN_TX3: &str = include_str!("templates/main.tx3.tpl");

#[derive(ClapArgs)]
pub struct Args {}

#[derive(Debug, Serialize)]
pub struct SimpleConfig {
    pub protocol: ProtocolConfig,

    #[serde(default)]
    pub bindings: Option<Vec<BindingsConfig>>,
}

pub fn run(_args: Args, _config: &Config) -> miette::Result<()> {
    // Get current working directory
    let current_dir = std::env::current_dir().unwrap();
    
    let protocol_name = Text::new("Protocol name")
        .with_default(&current_dir.file_name().unwrap().to_string_lossy())
        .prompt()
        .unwrap_or_default();

    let owner_scope = match Text::new("Owner scope")
        .prompt_skippable()
        .unwrap() {
            Some(s) if !s.trim().is_empty() => Some(s),
            _ => None,
        };

    let description = match Text::new("Description")
        .prompt_skippable()
        .unwrap() {
            Some(s) if !s.trim().is_empty() => Some(s),
            _ => None,
        };

    let version = Text::new("Version")
        .with_default("0.1.0")
        .prompt()
        .unwrap_or_default();

    let generate_bindings = MultiSelect::new("Generate bindings", vec!["Typescript", "Rust", "Go", "Python"])
        .prompt()
        .unwrap_or_default();

    let config = SimpleConfig {
        protocol: ProtocolConfig {
            name: protocol_name,
            scope: owner_scope,
            version,
            description,
            main: "main.tx3".into(),
        },
        bindings: if generate_bindings.len() > 0 {
            Some(generate_bindings.iter()
                .map(|binding| {
                    let plugin = binding.to_string().to_lowercase();
                    BindingsConfig {
                        output_dir: PathBuf::from(format!("./gen/{}", plugin)),
                        plugin,
                    }
                })
                .collect())
        } else {
            None
        },
    };

    let toml_string = toml::to_string_pretty(&config).into_diagnostic()?;

    println!("\n{}", toml_string);

    let confirm = Confirm::new("Is this OK?")
        .with_default(true)
        .prompt()
        .unwrap_or_default();

    if confirm {
        std::fs::write("trix.toml", toml_string).into_diagnostic()?;
        std::fs::write("main.tx3", TEMPLATE_MAIN_TX3).into_diagnostic()?;
    }

    Ok(())
}