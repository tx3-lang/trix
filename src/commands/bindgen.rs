use std::collections::HashMap;

use crate::config::{Config, KnownChain, TrpConfig};
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use tx3_lang::Protocol;

#[derive(ClapArgs)]
pub struct Args {}

// Include template files at compile time for TYPESCRIPT
const TEMPLATE_TYPESCRIPT_PACKAGEJSON: &str =
    include_str!("templates/gen-typescript/package.json.tpl");
const TEMPLATE_TYPESCRIPT_TESTTS: &str = include_str!("templates/gen-typescript/test.ts.tpl");
const TEMPLATE_TYPESCRIPT_TSCONFIGJSON: &str =
    include_str!("templates/gen-typescript/tsconfig.json.tpl");

fn generate_typescript(job: &tx3_bindgen::Job, version: &str) -> miette::Result<()> {
    tx3_bindgen::typescript::generate(job);

    let dest_path = job.dest_path.clone();
    std::fs::write(
        dest_path.join("package.json"),
        TEMPLATE_TYPESCRIPT_PACKAGEJSON
            .replace("{{project_name}}", &job.name)
            .replace("{{protocol_version}}", version),
    )
    .into_diagnostic()?;

    std::fs::write(
        dest_path.join("test.ts"),
        TEMPLATE_TYPESCRIPT_TESTTS.replace("{{protocol_name}}", &job.name),
    )
    .into_diagnostic()?;
    std::fs::write(
        dest_path.join("tsconfig.json"),
        TEMPLATE_TYPESCRIPT_TSCONFIGJSON,
    )
    .into_diagnostic()?;

    Ok(())
}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    for bindgen in config.bindings.iter() {
        let protocol = Protocol::from_file(config.protocol.main.clone()).load()?;

        std::fs::create_dir_all(&bindgen.output_dir).into_diagnostic()?;

        let profile = config
            .profiles
            .clone()
            .unwrap_or_default()
            .devnet
            .trp
            .unwrap_or_else(|| TrpConfig::from(KnownChain::CardanoDevnet));

        let job = tx3_bindgen::Job {
            name: config.protocol.name.clone(),
            protocol: protocol,
            dest_path: bindgen.output_dir.clone(),
            trp_endpoint: profile.url.clone(),
            trp_headers: profile.headers.clone(),
            env_args: HashMap::new(),
        };

        match bindgen.plugin.as_str() {
            "rust" => {
                tx3_bindgen::rust::generate(&job);
                println!("Rust bindgen successful");
            }
            "typescript" => {
                generate_typescript(&job, &config.protocol.version)?;
                println!("Typescript bindgen successful");
            }
            _ => return Err(miette::bail!("Unsupported plugin: {}", bindgen.plugin)),
        };
    }

    Ok(())
}
