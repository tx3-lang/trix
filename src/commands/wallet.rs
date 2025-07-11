use clap::{Args as ClapArgs, Subcommand};
use cryptoxide::{digest::Digest, sha2::Sha256};
use miette::IntoDiagnostic;
use pallas::{crypto::key::ed25519::SecretKey, ledger::addresses::ShelleyAddress};

use crate::config::Config;

#[derive(ClapArgs)]
pub struct Args {
    /// Wallet name
    name: String,

    /// Command
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    PrivateKey,
    AddressTestnet,
}

fn generate_deterministic_key(wallet_name: &str) -> SecretKey {
    let mut hasher = Sha256::new();
    hasher.input(wallet_name.as_bytes());
    let hash = hasher.result_str();

    let entropy: [u8; 32] = hash[..32].as_bytes().try_into().unwrap();

    SecretKey::from(entropy)
}

pub fn run(args: Args, _config: &Config) -> miette::Result<()> {
    match args.command {
        Command::PrivateKey => {
            let private_key = generate_deterministic_key(&args.name);
            unsafe {
                let bytes = SecretKey::leak_into_bytes(private_key);
                println!("{}", hex::encode(bytes));
            }
        }
        Command::AddressTestnet => {
            let private_key = generate_deterministic_key(&args.name);
            let public_key = private_key.public_key();
            let mut hasher = pallas::crypto::hash::Hasher::<224>::new();
            hasher.input(public_key.as_ref());
            let hash = hasher.finalize();

            let address = ShelleyAddress::new(
                pallas::ledger::addresses::Network::Testnet,
                pallas::ledger::addresses::ShelleyPaymentPart::Key(hash),
                pallas::ledger::addresses::ShelleyDelegationPart::Null,
            );

            println!("{}", address.to_bech32().into_diagnostic()?);
        }
    }

    Ok(())
}
