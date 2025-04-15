use std::collections::HashMap;

use crate::config::Config;
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use tx3_lang::Protocol;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    for bindgen in config.bindings.iter() {
        let protocol = Protocol::from_file(config.protocol.main.clone()).load()?;

        std::fs::create_dir_all(&bindgen.output_dir).into_diagnostic()?;

        let job = tx3_bindgen::Job {
            name: config.protocol.name.clone(),
            protocol: protocol,
            dest_path: bindgen.output_dir.clone(),
            trp_endpoint: config.profiles.dev.trp.url.clone(),
            trp_headers: HashMap::new(),
            env_args: HashMap::new(),
        };

        match bindgen.plugin.as_str() {
            "rust" => tx3_bindgen::rust::generate(&job),
            "typescript" => tx3_bindgen::typescript::generate(&job),
            _ => return Err(miette::bail!("Unsupported plugin: {}", bindgen.plugin)),
        };
    }

    Ok(())
}
