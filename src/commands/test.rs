use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread::sleep,
    time::Duration,
};

use clap::Args as ClapArgs;
use jsonrpsee::core::params::ObjectParams;
use miette::{Context, IntoDiagnostic, bail};
use pallas::ledger::addresses::Address;
use serde::Deserialize;
use tx3_lang::analyzing::Analyzable;

use crate::config::Config;

use super::devnet::{
    ALONZO, BYRON, CONWAY, CSHELL, DOLOS, SHELLEY, get_home_path, map_genesis_path,
    map_shelley_initial_funds, write_file, write_file_toml,
};

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
    expects: Vec<Expect>,
}

#[derive(Debug, Deserialize)]
struct Wallet {
    name: String,
    lovelace: u64,
}

#[derive(Debug, Deserialize)]
struct Transaction {
    description: String,
    template: String,
    args: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Expect {
    wallet: String,
    lovelace: u64,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let test_content = std::fs::read_to_string(args.path).into_diagnostic()?;
    let test = toml::from_str::<Test>(&test_content).into_diagnostic()?;

    // let mut devnet_process = run_devnet(&test)?;

    let protocol = tx3_lang::Protocol::from_file(&test.file)
        .load()
        .into_diagnostic()
        .context("parsing tx3 file")?;

    for transaction in test.transactions {
        let Some(tx) = protocol.txs().find(|tx| tx.name == transaction.template) else {
            bail!("invalid transaction template")
        };

        let prototx = protocol.new_tx(&tx.name).unwrap();
        let params = prototx.find_params();

        let mut builder = ObjectParams::new();
        builder
            .insert(
                "tir",
                serde_json::json!({
                    "version": "v1alpha1",
                    "encoding": "hex",
                    "bytecode": hex::encode(prototx.ir_bytes())
                }),
            )
            .unwrap();
        builder.insert("args", transaction.args).unwrap();

        // let response = provider.trp_resolve(&builder).await?;
    }

    // let tx_def = program.txs.first().unwrap();
    // let res = program.analyze(None);

    // dbg!(tx_def.is_resolved());
    // tx3_lang::apply_args(x, Default::default());

    // let value = tx3_lang::apply_args(template, args)

    // sleep(Duration::from_secs(5));

    // devnet_process
    //     .kill()
    //     .into_diagnostic()
    //     .context("failed to stop dolos devnet in background")?;

    Ok(())
}

fn run_devnet(test: &Test) -> miette::Result<Child> {
    let home_path = get_home_path()?;
    let tmp_path = handle_devnet_for_tests(&home_path, test)?;

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

fn handle_devnet_for_tests(home_path: &PathBuf, test: &Test) -> miette::Result<PathBuf> {
    if !home_path.join(".tx3").exists() {
        bail!("run tx3up to prepare the environment first")
    }

    let mut tmp_path = home_path.clone();
    tmp_path.push(format!(".tx3"));
    if !tmp_path.exists() {
        bail!("run tx3up to prepare the environment first")
    }
    tmp_path.push(format!("tmp/test_devnet"));
    let tmp_path_str = tmp_path.to_str().unwrap();

    fs::create_dir_all(&tmp_path)
        .into_diagnostic()
        .context("failed to create target directory")?;

    let mut cshell_config_path = tmp_path.clone();
    cshell_config_path.push("cshell.toml");
    let mut cshell_path = home_path.clone();
    if cfg!(target_os = "windows") {
        cshell_path.push(".tx3/default/bin/cshell.exe");
    } else {
        cshell_path.push(".tx3/default/bin/cshell");
    };
    let cshell = serde_json::to_value(toml::from_str::<toml::Value>(CSHELL).into_diagnostic()?)
        .into_diagnostic()?;
    write_file_toml(&format!("{tmp_path_str}/cshell.toml").into(), &cshell)?;

    let mut initial_funds = HashMap::new();
    for wallet in &test.wallets {
        let mut cmd = Command::new(cshell_path.to_str().unwrap_or_default());
        cmd.args([
            "-s",
            cshell_config_path.to_str().unwrap_or_default(),
            "wallet",
            "create",
            "--name",
            &wallet.name,
            "--password",
            &wallet.name,
            "--output-format",
            "json",
        ]);
        let output = cmd
            .output()
            .into_diagnostic()
            .expect("fail to create devnet wallets");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("cshell failed to create wallet:\n{}", stderr.trim());
        }
        let output: serde_json::Value = serde_json::from_slice(&output.stdout).into_diagnostic()?;
        let address = output
            .get("addresses")
            .context("missing 'addresses' field in cshell JSON output")?
            .get("testnet")
            .context("missing 'testnet' field in cshell 'addresses'")?
            .as_str()
            .unwrap();
        let address = Address::from_bech32(&address).into_diagnostic()?.to_hex();
        initial_funds.insert(address, wallet.lovelace);
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

    return Ok(tmp_path);
}
