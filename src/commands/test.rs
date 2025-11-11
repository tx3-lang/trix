use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, Result, bail};
use pallas::ledger::addresses::Address;
use serde::{Deserialize, Serialize};

use crate::{
    config::{Config, ProfileConfig, load_profile_env_vars},
    spawn::cshell::OutputWallet,
};

const BLOCK_PRODUCTION_INTERVAL_SECONDS: u64 = 5;
const BOROS_SPAW_DELAY_SECONDS: u64 = 2;

#[derive(ClapArgs)]
pub struct Args {
    /// Test toml file
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Test {
    file: PathBuf,
    wallets: Vec<Wallet>,
    transactions: Vec<Transaction>,
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
struct ExpectUtxo {
    from: String,
    datum_equals: Option<serde_json::Value>,
    min_amount: Option<Vec<ExpectMinAmount>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExpectMinAmount {
    policy: Option<String>,
    name: Option<String>,
    amount: u64,
}

fn ensure_test_home(test: &Test, hashable: &[u8]) -> Result<PathBuf> {
    let test_home = crate::home::consistent_tmp_dir("test", hashable)?;

    // if the test with the exact hash already exists, we assume it's already initialized
    if test_home.exists() {
        return Ok(test_home);
    }

    crate::spawn::cshell::initialize_config(&test_home)?;

    let mut initial_funds = HashMap::new();

    for wallet in &test.wallets {
        let output = crate::spawn::cshell::wallet_create(&test_home, &wallet.name)?;

        let address = output
            .get("addresses")
            .context("missing 'addresses' field in cshell JSON output")?
            .get("testnet")
            .context("missing 'testnet' field in cshell 'addresses'")?
            .as_str()
            .unwrap();

        let address = Address::from_bech32(address).into_diagnostic()?.to_hex();
        initial_funds.insert(address, wallet.balance);
    }

    crate::spawn::dolos::initialize_config(&test_home, &initial_funds, &vec![])?;

    Ok(test_home)
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

pub fn run(args: Args, _config: &Config, profile: &ProfileConfig) -> Result<()> {
    println!("== Starting tests ==\n");
    let test_content = std::fs::read_to_string(args.path).into_diagnostic()?;
    let test = toml::from_str::<Test>(&test_content).into_diagnostic()?;

    let test_home = ensure_test_home(&test, test_content.as_bytes())?;

    let mut dolos = crate::spawn::dolos::daemon(&test_home, true)?;
    println!("Dolos daemon started");

    sleep(Duration::from_secs(BOROS_SPAW_DELAY_SECONDS));

    let mut failed = false;
    for transaction in &test.transactions {
        println!("--- Running transaction: {} ---", transaction.description);
        if let Err(err) = trigger_transaction(&test_home, &test.file, transaction, &profile) {
            eprintln!("Transaction `{}` failed.\n", transaction.description);
            eprintln!("Error: {err}\n");
            failed = true;
        }

        println!("Waiting next block...");
        sleep(Duration::from_secs(BLOCK_PRODUCTION_INTERVAL_SECONDS));
    }

    for expect in test.expect.iter() {
        // let balance = crate::spawn::cshell::wallet_balance(&test_home, &expect.from)?;
        todo!();
        // let r#match = expect.amount.matches(balance.coin);

        // if !r#match {
        //     failed = true;

        //     eprintln!(
        //         "Test Failed: `{}` Balance did not match the expected result.",
        //         expect.from
        //     );
        //     eprintln!("Expected: {}", expect.amount);
        //     eprintln!("Received: {}", balance.coin);

        //     eprintln!("Hint: Check the tx3 file or the test file.");
        // }
    }

    if !failed {
        println!("Test Passed\n");
    }

    dolos
        .kill()
        .into_diagnostic()
        .context("failed to stop dolos devnet in background")?;

    if failed {
        bail!("Test failed, see the output above for details.");
    }

    Ok(())
}
