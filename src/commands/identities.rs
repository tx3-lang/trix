use clap::{Args as ClapArgs, Subcommand};
use miette::IntoDiagnostic;

use crate::config::{ProfileConfig, RootConfig};

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

pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    let wallet = crate::wallet::setup(config, profile)?;

    let info = wallet.info(&args.name)?;

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
