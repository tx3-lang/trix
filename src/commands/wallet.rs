use clap::{Args as ClapArgs, Subcommand};
use cryptoxide::{digest::Digest, sha2::Sha256};
use miette::IntoDiagnostic;
use pallas::{crypto::key::ed25519::SecretKey, ledger::addresses::ShelleyAddress};

use crate::{config::Config, spawn};

#[derive(ClapArgs)]
pub struct Args {
    /// Wallet name
    name: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Subcommand)]
pub enum Command {
    AddressTestnet,
    AddressMainnet,
    PublicKey,
    PublicKeyHash,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::commands::devnet::ensure_devnet_home(config)?;

    let info = spawn::cshell::wallet_info(&devnet_home, &args.name)?;

    let Some(command) = args.command else {
        let pretty = serde_json::to_string_pretty(&info).into_diagnostic()?;
        println!("{}", pretty);
        return Ok(());
    };

    match command {
        Command::AddressTestnet => {
            let x = info
                .addresses
                .get("testnet")
                .map(|x| x.as_str())
                .unwrap_or_default();

            println!("{x}");
        }
        Command::AddressMainnet => {
            let x = info
                .addresses
                .get("mainnet")
                .map(|x| x.as_str())
                .unwrap_or_default();

            println!("{x}");
        }
        Command::PublicKey => {
            let x = info.public_key;
            println!("{x}");
        }
        Command::PublicKeyHash => todo!(),
    }

    Ok(())
}
