use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use miette::Context;

use crate::config::{Config, KeyConfig, ProfileConfig};

fn setup_wallet_key(home: &Path, key: &KeyConfig) -> miette::Result<String> {
    let output = crate::spawn::cshell::wallet_create(home, &key.name)?;

    let address = output
        .get("addresses")
        .context("missing 'addresses' field in cshell JSON output")?
        .get("testnet")
        .context("missing 'testnet' field in cshell 'addresses'")?
        .as_str()
        .unwrap();

    Ok(address.to_string())
}

pub struct WalletProxy {
    pub home: PathBuf,
    pub addresses: HashMap<String, String>,
}

pub fn setup(config: &Config, profile: &ProfileConfig) -> miette::Result<WalletProxy> {
    let home = crate::home::consistent_tmp_dir("protocol", config.protocol.name.as_bytes())?;

    let _ = crate::spawn::cshell::initialize_config(&home, profile)?;

    let mut addresses = HashMap::new();

    for key in config.keys.iter() {
        let address = setup_wallet_key(&home, key)?;
        addresses.insert(key.name.clone(), address);
    }

    Ok(WalletProxy { home, addresses })
}
