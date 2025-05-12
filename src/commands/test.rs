use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread::sleep,
    time::Duration,
};

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;
use serde::{Deserialize, Deserializer, de};

use crate::config::Config;

use super::devnet::{
    ALONZO, BYRON, CONWAY, CSHELL, DOLOS, SHELLEY, get_home_path, map_genesis_path,
    map_shelley_initial_funds, write_file, write_file_toml,
};

const BLOCK_PRODUCTION_INTERVAL_SECONDS: u64 = 5;
const BOROS_SPAW_DELAY_SECONDS: u64 = 2;

#[derive(ClapArgs)]
pub struct Args {
    /// Test toml file
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Test {
    file: PathBuf,
    wallets: Vec<Wallet>,
    transactions: Vec<Transaction>,
    expect: Option<Vec<Expect>>,
}

#[derive(Debug, Deserialize)]
struct Wallet {
    name: String,
    balance: u64,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    description: String,
    template: String,
    args: HashMap<String, serde_json::Value>,
    #[serde(default)]
    wait_block: bool,
    signer: String,
}

#[derive(Debug, Deserialize)]
struct Expect {
    wallet: String,
    balance: u64,
}

#[derive(Debug, Deserialize)]
struct CshellWallet {
    name: String,
    addresses: CshellAddress,
}
#[derive(Debug, Deserialize)]
struct CshellAddress {
    testnet: String,
}
#[derive(Debug, Deserialize)]
struct CshellBalance {
    #[serde(deserialize_with = "string_to_u64")]
    coin: u64,
}

fn string_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    s.parse::<u64>().map_err(de::Error::custom)
}

pub fn run(args: Args, _config: &Config) -> miette::Result<()> {
    println!("== Starting tests ==\n");
    let test_content = std::fs::read_to_string(args.path).into_diagnostic()?;
    let test = toml::from_str::<Test>(&test_content).into_diagnostic()?;

    let mut devnet_process = handle_devnet_for_tests(&test).context("failed to spaw devnet")?;
    sleep(Duration::from_secs(BOROS_SPAW_DELAY_SECONDS));

    let mut failed = false;
    for transaction in &test.transactions {
        println!("--- Running transaction: {} ---", transaction.description);
        if let Err(err) = handle_cshell_transaction(&test.file, transaction) {
            eprintln!("Transaction `{}` failed.\n", transaction.description);
            eprintln!("Error: {err}\n");
            failed = true;
        }

        if transaction.wait_block {
            println!("Waiting next block...");
            sleep(Duration::from_secs(BLOCK_PRODUCTION_INTERVAL_SECONDS));
        }

        println!("Transaction completed \n");
    }

    sleep(Duration::from_secs(BLOCK_PRODUCTION_INTERVAL_SECONDS));

    if let Some(expect) = test.expect {
        for expect in &expect {
            // TODO: improve expect adding more options
            let output = handle_cshell_command(vec![
                "wallet",
                "balance",
                &expect.wallet,
                "--output-format",
                "json",
            ]);

            if let Err(err) = output {
                eprintln!("Error: {err}\n");
                failed = true;
                continue;
            }

            let output = output.unwrap();

            let balance = serde_json::from_slice::<CshellBalance>(&output);
            if let Err(err) = balance {
                eprintln!("Error: {err}\n");
                failed = true;
                continue;
            }

            let balance = balance.unwrap();

            if balance.coin != expect.balance {
                failed = true;

                eprintln!(
                    "Test Failed: `{}` Balance did not match the expected result.",
                    expect.wallet
                );
                eprintln!("Expected: {}", expect.balance);
                eprintln!("Received: {}", balance.coin);
                eprintln!("Hint: Check the tx3 file or the test file.");
                break;
            }
        }
    }

    if !failed {
        println!("Test Passed\n");
    }

    devnet_process
        .kill()
        .into_diagnostic()
        .context("failed to stop dolos devnet in background")?;

    Ok(())
}

fn handle_devnet_for_tests(test: &Test) -> miette::Result<Child> {
    let home_path = get_home_path()?;
    let mut tmp_path = home_path.clone();
    tmp_path.push(".tx3");
    if !tmp_path.exists() {
        bail!("run tx3up to prepare the environment first")
    }
    tmp_path.push("tmp/test_devnet");
    let tmp_path_str = tmp_path.to_str().unwrap();

    fs::create_dir_all(&tmp_path)
        .into_diagnostic()
        .context("failed to create target directory")?;

    let cshell = serde_json::to_value(toml::from_str::<toml::Value>(CSHELL).into_diagnostic()?)
        .into_diagnostic()?;
    write_file_toml(&format!("{tmp_path_str}/cshell.toml").into(), &cshell)?;

    let mut initial_funds = HashMap::new();
    for wallet in &test.wallets {
        let output = handle_cshell_command(vec![
            "wallet",
            "create",
            "--name",
            &wallet.name,
            "--unsafe",
            "--output-format",
            "json",
        ])
        .context("fail to create devnet wallets")?;

        let output: serde_json::Value = serde_json::from_slice(&output).into_diagnostic()?;

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

    let mut shelley = serde_json::from_str::<serde_json::Value>(SHELLEY).into_diagnostic()?;
    map_shelley_initial_funds(initial_funds, &mut shelley)?;
    write_file(&format!("{tmp_path_str}/shelley.json").into(), &shelley)?;

    let mut dolos = serde_json::to_value(toml::from_str::<toml::Value>(DOLOS).into_diagnostic()?)
        .into_diagnostic()?;
    map_genesis_path(tmp_path_str, &mut dolos)?;
    write_file_toml(&format!("{tmp_path_str}/dolos.toml").into(), &dolos)?;

    let byron = serde_json::from_str::<serde_json::Value>(BYRON).into_diagnostic()?;
    write_file(&format!("{tmp_path_str}/byron.json").into(), &byron)?;

    let alonzo = serde_json::from_str::<serde_json::Value>(ALONZO).into_diagnostic()?;
    write_file(&format!("{tmp_path_str}/alonzo.json").into(), &alonzo)?;

    let conway = serde_json::from_str::<serde_json::Value>(CONWAY).into_diagnostic()?;
    write_file(&format!("{tmp_path_str}/conway.json").into(), &conway)?;

    let mut dolos_config_path = tmp_path.clone();
    dolos_config_path.push("dolos.toml");

    let mut dolos_path = home_path.clone();
    if cfg!(target_os = "windows") {
        dolos_path.push(".tx3/default/bin/dolos.exe");
    } else {
        dolos_path.push(".tx3/default/bin/dolos");
    };

    let mut cmd = Command::new(dolos_path.to_str().unwrap_or_default());
    let child = cmd
        .args([
            "-c",
            dolos_config_path.to_str().unwrap_or_default(),
            "daemon",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .into_diagnostic()
        .context("failed to spawn dolos devnet in background")?;

    Ok(child)
}

fn handle_cshell_transaction(file: &Path, transaction: &Transaction) -> miette::Result<()> {
    let output = handle_cshell_command(vec!["wallet", "list", "--output-format", "json"])?;

    let wallets: Vec<CshellWallet> = serde_json::from_slice(&output).into_diagnostic()?;

    let args: HashMap<String, serde_json::Value> = transaction
        .args
        .clone()
        .into_iter()
        .map(|mut arg| {
            if let serde_json::Value::String(s) = &arg.1 {
                if let Some(wallet) = wallets.iter().find(|w| w.name.eq(s)) {
                    arg.1 = serde_json::Value::String(wallet.addresses.testnet.clone());
                }
            };
            arg
        })
        .collect();

    let transaction_args = serde_json::to_string(&args).into_diagnostic()?;

    handle_cshell_command(vec![
        "transaction",
        "--tx3-file",
        file.to_str().unwrap(),
        "--tx3-args-json",
        &transaction_args,
        "--tx3-template",
        &transaction.template,
        "--signer",
        &transaction.signer,
    ])?;

    Ok(())
}

fn handle_cshell_command(extra_args: Vec<&str>) -> miette::Result<Vec<u8>> {
    let home_path = get_home_path()?;
    let mut tmp_path = home_path.clone();
    tmp_path.push(".tx3");
    tmp_path.push("tmp/test_devnet");

    let mut cshell_config_path = tmp_path.clone();
    cshell_config_path.push("cshell.toml");
    let mut cshell_path = home_path.clone();
    if cfg!(target_os = "windows") {
        cshell_path.push(".tx3/default/bin/cshell.exe");
    } else {
        cshell_path.push(".tx3/default/bin/cshell");
    };

    let mut cmd = Command::new(cshell_path.to_str().unwrap_or_default());

    let mut args = vec!["-s", cshell_config_path.to_str().unwrap_or_default()];
    args.extend(extra_args);

    cmd.args(args);

    let output = cmd.output().into_diagnostic()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(miette::Error::msg(stderr.to_string()));
    }

    Ok(output.stdout)
}
