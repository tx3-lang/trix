use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use clap::Args as ClapArgs;
use miette::{Context as _, IntoDiagnostic, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, ProfileConfig, load_profile_env_vars},
    devnet::Config as DevnetConfig,
    spawn::cshell::OutputWallet,
};

const BLOCK_PRODUCTION_INTERVAL_SECONDS: u64 = 5;
const DOLOS_SPAWN_DELAY_SECONDS: u64 = 2;

#[derive(ClapArgs)]
pub struct Args {
    /// Test toml file
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Context {
    protocol: PathBuf,
    devnet: PathBuf,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            protocol: PathBuf::from("./main.tx3"),
            devnet: PathBuf::from("./devnet.toml"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Test {
    #[serde(default)]
    context: Context,

    #[serde(default)]
    wallets: Vec<Wallet>,

    #[serde(default)]
    transactions: Vec<Transaction>,

    #[serde(default)]
    expect: Vec<ExpectUtxo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Wallet {
    name: String,
    balance: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Transaction {
    description: String,
    template: String,
    args: HashMap<String, serde_json::Value>,
    signers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExpectUtxo {
    pub(crate) from: String,
    pub(crate) datum_equals: Option<serde_json::Value>,
    pub(crate) min_amount: Vec<ExpectMinAmount>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExpectMinAmount {
    pub(crate) policy: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) amount: u64,
}

fn replace_placeholder_args(args: &mut ArgMap, wallets: &Vec<OutputWallet>) {
    for (_, value) in args.iter_mut() {
        if let serde_json::Value::String(s) = value {
            if s.starts_with('@') {
                if let Some(wallet) = wallets
                    .iter()
                    .find(|w| w.name.eq(s.trim_start_matches('@')))
                {
                    *value = serde_json::Value::String(wallet.addresses.testnet.clone());
                }
            }
        }
    }
}

pub type ArgMap = serde_json::Map<String, serde_json::Value>;

fn merge_json_maps_mut(a: &mut ArgMap, b: &ArgMap) {
    for (key, value) in b {
        a.insert(key.clone(), value.clone());
    }
}

fn define_args(
    transaction: &Transaction,
    wallets: &Vec<OutputWallet>,
    profile: &ProfileConfig,
) -> Result<serde_json::Value> {
    let mut all = ArgMap::new();

    let explicit = serde_json::to_value(&transaction.args).into_diagnostic()?;
    let explicit = explicit.as_object().unwrap();

    merge_json_maps_mut(&mut all, explicit);

    let env = serde_json::to_value(&load_profile_env_vars(profile)?).into_diagnostic()?;
    let env = env.as_object().unwrap();
    merge_json_maps_mut(&mut all, &env);

    replace_placeholder_args(&mut all, wallets);

    Ok(serde_json::json!(all))
}

fn trigger_transaction(
    home: &Path,
    tx3_file: &Path,
    transaction: &Transaction,
    profile: &ProfileConfig,
) -> Result<()> {
    let wallets = crate::spawn::cshell::wallet_list(home)?;

    let args = define_args(transaction, &wallets, profile)?;

    dbg!(&args);

    let signer = match transaction.signers.len() {
        1 => transaction.signers[0].clone(),
        _ => {
            bail!("only one signer is supported at the moment")
        }
    };

    let output = crate::spawn::cshell::tx_invoke_json(
        home,
        tx3_file,
        &serde_json::json!(args),
        Some(&transaction.template),
        vec![&signer],
        true,
        false,
    )?;

    println!("Invoke output: {:#?}", output);

    Ok(())
}

pub fn run(args: Args, config: &Config, profile: &ProfileConfig) -> Result<()> {
    println!("== Starting tests ==\n");
    let test_content = std::fs::read_to_string(args.path).into_diagnostic()?;
    let test = toml::from_str::<Test>(&test_content).into_diagnostic()?;

    let wallet = crate::wallet::setup(config, profile)?;

    let devnet = DevnetConfig::load(&test.context.devnet)?;

    let ctx = crate::devnet::Context::from_wallet(&wallet);

    let mut devnet = crate::devnet::start_daemon(&devnet, &ctx, true)?;

    println!("Dolos daemon started");

    sleep(Duration::from_secs(DOLOS_SPAWN_DELAY_SECONDS));

    let mut failed = false;
    for transaction in &test.transactions {
        println!("--- Running transaction: {} ---", transaction.description);

        let result =
            trigger_transaction(&devnet.home, &test.context.protocol, transaction, &profile);

        if let Err(err) = result {
            eprintln!("Transaction `{}` failed.\n", transaction.description);
            eprintln!("Error: {err}\n");
            failed = true;
        }

        println!("Waiting next block...");
        sleep(Duration::from_secs(BLOCK_PRODUCTION_INTERVAL_SECONDS));
    }

    failed |= crate::commands::expect::expect_utxo(&test.expect, &devnet.home)?;

    if !failed {
        println!("Test Passed\n");
    }

    devnet
        .daemon
        .kill()
        .into_diagnostic()
        .context("failed to stop dolos devnet in background")?;

    if failed {
        bail!("Test failed, see the output above for details.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_expect_utxo_toml() {
        let toml = r#"
            [context]
            protocol = "./main.tx3"
            devnet = "./devnet.toml"
            
            [[transactions]]
            description = "Simple Oracle"
            template = "create"
            signers = ["operator"]
            args = { rate = 42, operator = "@operator", oracle = "@oracle" }

            [[expect]]
            from = "@oracle"
            datum_equals = 42

            [[expect.min_amount]]
            amount = 123

            [[expect.min_amount]]
            policy = "xyz"
            name = "abc"
            amount = 456
        "#;

        let parsed: Test = toml::from_str(toml).expect("parse toml");

        assert_eq!(parsed.context.protocol, PathBuf::from("./main.tx3"));
        assert_eq!(parsed.context.devnet, PathBuf::from("./devnet.toml"));
        assert_eq!(parsed.wallets.len(), 2);

        assert_eq!(parsed.transactions.len(), 1);

        assert_eq!(parsed.expect.len(), 1);
        let e = &parsed.expect[0];
        assert_eq!(e.from, "@oracle");

        assert!(e.datum_equals.is_some());
        let datum = e.datum_equals.as_ref().unwrap();
        match datum {
            serde_json::Value::Number(n) => {
                assert_eq!(n.as_i64(), Some(42));
            }
            other => panic!("unexpected datum kind: {other:?}"),
        }

        let mins = &e.min_amount;
        assert_eq!(mins.len(), 2);

        assert_eq!(mins[0].amount, 123);
        assert!(mins[0].policy.is_none() && mins[0].name.is_none());

        assert_eq!(mins[1].policy.as_ref().unwrap(), "xyz");
        assert_eq!(mins[1].name.as_ref().unwrap(), "abc");
        assert_eq!(mins[1].amount, 456);
    }
}
