use std::path::PathBuf;

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic};

#[derive(ClapArgs)]
pub struct Args {
    /// Path to save the devnet config file
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub fn run(
    args: Args,
    config: &crate::config::RootConfig,
    _profile: &crate::config::ProfileConfig,
) -> miette::Result<()> {
    let output_path = match args.output {
        Some(path) => path,
        None => crate::dirs::protocol_root()?.join("devnet.toml"),
    };

    let local_profile = config
        .resolve_profile("local")
        .context("failed to resolve local profile")?;

    let devnet_config = crate::devnet::inquire_config(&local_profile)?;

    let toml = toml::to_string_pretty(&devnet_config)
        .into_diagnostic()
        .context("serializing devnet config to TOML")?;

    std::fs::write(&output_path, toml)
        .into_diagnostic()
        .context(format!(
            "writing devnet config to {}",
            output_path.to_string_lossy()
        ))?;

    Ok(())
}
