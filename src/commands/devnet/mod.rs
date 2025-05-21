use std::{collections::HashMap, path::PathBuf};

use miette::{Context, IntoDiagnostic as _};
use pallas::ledger::addresses::Address;

use crate::config::Config;

#[allow(clippy::module_inception)]
pub mod devnet;
pub mod explore;
pub mod invoke;

pub fn ensure_devnet_home(config: &Config) -> miette::Result<PathBuf> {
    let profile = config
        .profiles
        .as_ref()
        .map(|profiles| profiles.devnet.clone())
        .unwrap_or_default();

    let profile_hashable = serde_json::to_vec(&profile).into_diagnostic()?;

    let devnet_home = crate::home::consistent_tmp_dir("devnet", &profile_hashable)?;
    println!("devnet home initialized at: {}", devnet_home.display());

    let cshell_config = crate::spawn::cshell::initialize_config(&devnet_home)?;
    println!("cshell config initialized at: {}", cshell_config.display());

    let mut initial_funds = HashMap::new();

    for wallet in &profile.wallets {
        let output = crate::spawn::cshell::wallet_create(&devnet_home, &wallet.name)?;

        let address = output
            .get("addresses")
            .context("missing 'addresses' field in cshell JSON output")?
            .get("testnet")
            .context("missing 'testnet' field in cshell 'addresses'")?
            .as_str()
            .unwrap();

        let address = Address::from_bech32(address).into_diagnostic()?.to_hex();
        initial_funds.insert(address, wallet.initial_balance);
    }

    let dolos_config = crate::spawn::dolos::initialize_config(&devnet_home, &initial_funds)?;
    println!("dolos config initialized at: {}", dolos_config.display());

    Ok(devnet_home)
}
