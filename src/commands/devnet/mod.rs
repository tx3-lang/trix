use std::{collections::HashMap, path::PathBuf};

use clap::{Args as ClapArgs, Subcommand};
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;

use crate::config::{Config, ProfileConfig};
use crate::devnet::Config as DevnetConfig;

pub mod copy;
pub mod explore;
pub mod invoke;

pub fn ensure_devnet_home(config: &Config, profile: &ProfileConfig) -> miette::Result<PathBuf> {
    let profile_hashable = serde_json::to_vec(&profile).into_diagnostic()?;

    let devnet_home = crate::home::consistent_tmp_dir("devnet", &profile_hashable)?;
    // println!("devnet home initialized at: {}", devnet_home.display());

    let _cshell_config = crate::spawn::cshell::initialize_config(&devnet_home)?;
    // println!("cshell config initialized at: {}", cshell_config.display());

    let mut wallets = HashMap::new();

    for wallet in &config.wallets {
        let output = crate::spawn::cshell::wallet_create(&devnet_home, &wallet.name)?;

        let address = output
            .get("addresses")
            .context("missing 'addresses' field in cshell JSON output")?
            .get("testnet")
            .context("missing 'testnet' field in cshell 'addresses'")?
            .as_str()
            .unwrap();

        let address = Address::from_bech32(address).into_diagnostic()?.to_hex();

        wallets.insert(wallet.name.clone(), address);
    }

    // TODO: the actual devent file should be defined in the profile config
    let path = crate::dirs::protocol_root()?.join("devnet.toml");

    let devnet_config = DevnetConfig::load(&path)?;

    let initial_funds: Vec<(String, u64)> = devnet_config
        .iter_utxos_values(&wallets)
        .collect::<miette::Result<_>>()?;

    let initial_funds = HashMap::from_iter(initial_funds);

    let initial_utxos = devnet_config.iter_utxos_bytes()?;

    let _dolos_config =
        crate::spawn::dolos::initialize_config(&devnet_home, &initial_funds, &initial_utxos)?;
    // println!("dolos config initialized at: {}", dolos_config.display());

    Ok(devnet_home)
}

#[derive(Subcommand)]
pub enum Command {
    /// Retrieve the UTxO dependencies for one transaction
    Copy(copy::Args),
}

#[derive(ClapArgs)]
pub struct Args {
    #[clap(subcommand)]
    command: Option<Command>,

    /// run devnet as a background process
    #[arg(short, long, default_value_t = false)]
    background: bool,
}

pub fn run(args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    match args.command {
        Some(Command::Copy(args)) => copy::run(args, config, profile),
        None => run_devnet(args, config, profile),
    }
}

pub fn run_devnet(args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    let devnet_home = ensure_devnet_home(config, profile)?;

    let mut daemon = crate::spawn::dolos::daemon(&devnet_home, args.background)?;

    if args.background {
        println!("devnet started in background");
    } else {
        let status = daemon
            .wait()
            .into_diagnostic()
            .context("failed to wait for dolos devnet")?;

        if !status.success() {
            bail!("dolos devnet exited with code: {}", status);
        }
    }

    Ok(())
}
