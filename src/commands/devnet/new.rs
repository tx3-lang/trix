use std::path::PathBuf;

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Path to save the devnet config file
    #[arg(long)]
    pub output: Option<PathBuf>,
}

const DEFAULT_DEVNET_WALLET_AMOUNT: u64 = 100_000_000_000;

pub fn inquire_config(
    profile: &crate::config::ProfileConfig,
) -> miette::Result<crate::devnet::Config> {
    let mut utxos = Vec::new();

    for (identity_name, _identity) in profile.identities.iter() {
        let balance_str = inquire::Text::new(&format!("Initial balance for '{}':", identity_name))
            .with_default(&DEFAULT_DEVNET_WALLET_AMOUNT.to_string())
            .prompt()
            .into_diagnostic()
            .context(format!("failed to read balance for {}", identity_name))?;

        let balance = balance_str
            .parse::<u64>()
            .map_err(|e| miette::miette!("Invalid balance: {}. Must be a valid number.", e))
            .context("parsing balance")?;

        utxos.push(crate::devnet::UtxoSpec::Explicit(
            crate::devnet::ExplicitUtxoSpec {
                address: crate::devnet::AddressSpec::NamedWallet(identity_name.clone()),
                value: balance,
            },
        ));
    }

    Ok(crate::devnet::Config { utxos })
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

    let devnet_config = inquire_config(&local_profile)?;

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
