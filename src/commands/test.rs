use std::{
    collections::HashMap,
    fmt::Display,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;
use serde::{Deserialize, Serialize};

use crate::config::Config;

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
    expect: Vec<Expect>,
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
#[serde(tag = "type")]
enum Expect {
    Balance(ExpectBalance),
    // TODO: improve expect adding more options
}

#[derive(Debug, Serialize, Deserialize)]
struct ExpectBalance {
    wallet: String,
    amount: ExpectAmount,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum ExpectAmount {
    Absolute(u64),
    Aprox(ExpectAmountAprox),
}

impl ExpectAmount {
    pub fn matches(&self, value: u64) -> bool {
        match self {
            ExpectAmount::Absolute(x) => x.eq(&value),
            ExpectAmount::Aprox(x) => {
                let lower = x.target.saturating_sub(x.threshold);
                let upper = x.target + x.threshold;
                value >= lower && value <= upper
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ExpectAmountAprox {
    target: u64,
    threshold: u64,
}

impl Display for ExpectAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpectAmount::Absolute(value) => write!(f, "{value}"),
            ExpectAmount::Aprox(value) => {
                write!(f, "target: ~{} (+/- {})", value.target, value.threshold)
            }
        }
    }
}

fn ensure_test_home(test: &Test, hashable: &[u8]) -> miette::Result<PathBuf> {
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

fn trigger_transaction(
    home: &Path,
    tx3_file: &Path,
    transaction: &Transaction,
) -> miette::Result<()> {
    let wallets = crate::spawn::cshell::wallet_list(home)?;

    let args: HashMap<String, serde_json::Value> = transaction
        .args
        .clone()
        .into_iter()
        .map(|mut arg| {
            if let serde_json::Value::String(s) = &arg.1 {
                if s.starts_with('@') {
                    if let Some(wallet) = wallets
                        .iter()
                        .find(|w| w.name.eq(s.trim_start_matches('@')))
                    {
                        arg.1 = serde_json::Value::String(wallet.addresses.testnet.clone());
                    }
                }
            };
            arg
        })
        .collect();

    let signer = match transaction.signers.len() {
        1 => transaction.signers[0].clone(),
        _ => {
            bail!("only one signer is supported at the moment")
        }
    };

    let output = crate::spawn::cshell::tx_invoke_json(
        home,
        tx3_file,
        &Some(serde_json::json!(args)),
        Some(&transaction.template),
        vec![&signer],
        true,
        false,
    )?;

    println!("Invoke output: {:#?}", output);

    Ok(())
}

pub fn run(args: Args, _config: &Config) -> miette::Result<()> {
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
        if let Err(err) = trigger_transaction(&test_home, &test.file, transaction) {
            eprintln!("Transaction `{}` failed.\n", transaction.description);
            eprintln!("Error: {err}\n");
            failed = true;
        }

        println!("Waiting next block...");
        sleep(Duration::from_secs(BLOCK_PRODUCTION_INTERVAL_SECONDS));
    }

    for expect in test.expect.iter() {
        match expect {
            Expect::Balance(expect) => {
                let balance = crate::spawn::cshell::wallet_balance(&test_home, &expect.wallet)?;

                let r#match = expect.amount.matches(balance.coin);

                if !r#match {
                    failed = true;

                    eprintln!(
                        "Test Failed: `{}` Balance did not match the expected result.",
                        expect.wallet
                    );
                    eprintln!("Expected: {}", expect.amount);
                    eprintln!("Received: {}", balance.coin);

                    eprintln!("Hint: Check the tx3 file or the test file.");
                }
            }
        }
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
