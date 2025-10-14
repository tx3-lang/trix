use miette::{Context as _, IntoDiagnostic as _};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub const DOLOS_TEMPLATE: &str = include_str!("../templates/configs/dolos/dolos.toml");
pub const ALONZO_TEMPLATE: &str = include_str!("../templates/configs/dolos/alonzo.json");
pub const BYRON_TEMPLATE: &str = include_str!("../templates/configs/dolos/byron.json");
pub const CONWAY_TEMPLATE: &str = include_str!("../templates/configs/dolos/conway.json");
pub const SHELLEY_TEMPLATE: &str = include_str!("../templates/configs/dolos/shelley.json");

fn initialize_shelley_config(initial_funds: &HashMap<String, u64>) -> miette::Result<String> {
    let mut original: Value = serde_json::from_str(SHELLEY_TEMPLATE)
        .into_diagnostic()
        .context("parsing shelley JSON")?;

    let object = original
        .get_mut("initialFunds")
        .context("missing 'initialFunds' field")?
        .as_object_mut()
        .context("'initialFunds' is not a JSON object")?;

    for (address, balance) in initial_funds {
        object.insert(
            address.clone(),
            serde_json::Value::Number(serde_json::Number::from(*balance)),
        );
    }

    serde_json::to_string_pretty(&original)
        .into_diagnostic()
        .context("serializing shelley JSON")
}

fn initialize_data_dir(home_dir: &Path) -> miette::Result<PathBuf> {
    let data_dir = home_dir.join("data");

    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)
            .into_diagnostic()
            .context("prunning data directory")?;
    }

    std::fs::create_dir_all(&data_dir)
        .into_diagnostic()
        .context("creating data directory")?;

    Ok(data_dir)
}

fn initialize_wal_store(data_dir: &Path) -> miette::Result<()> {
    let wal = dolos_redb::wal::RedbWalStore::open(&data_dir.join("wal"), None)
        .into_diagnostic()
        .context("creating wal store")?;

    wal.initialize_from_origin()
        .into_diagnostic()
        .context("initializing wal from origin")?;

    Ok(())
}

fn initialize_ledger_store(data_dir: &Path) -> miette::Result<dolos_redb::state::LedgerStore> {
    let state: dolos_redb::state::LedgerStore = dolos_redb::state::LedgerStore::open(&data_dir.join("ledger"), None)
        .map_err(dolos_core::StateError::from)
        .into_diagnostic()
        .context("creating ledger store")?;

    Ok(state)
}

fn initialize_chain_store(data_dir: &Path) -> miette::Result<dolos_redb::archive::ChainStore> {
    let archive = dolos_redb::archive::ChainStore::open(&data_dir.join("chain"), None)
        .map_err(dolos_core::ArchiveError::from)
        .into_diagnostic()
        .context("creating chain store")?;

    Ok(archive)
}

fn calculate_deltas(initial_utxos: &Vec<(String, Vec<u8>)>) -> miette::Result<Vec<dolos_core::LedgerDelta>> {
    use dolos_cardano::pallas::ledger::traverse::{MultiEraOutput, Era};

    let eras = vec![Era::Conway, Era::Babbage, Era::Alonzo, Era::Byron];

    let mut delta = dolos_core::LedgerDelta::default();

    println!("Applying initial UTxOs...");

    for (address, bytes) in initial_utxos {
        let utxo_hash = hex::decode(address.split('#').nth(0).unwrap_or_default())
            .into_diagnostic()
            .context("decoding tx hash")?;

        let utxo_id = address.split('#').nth(1).unwrap_or_default().parse::<u32>()
            .into_diagnostic()
            .context("parsing tx id")?;

        let utxo_ref = dolos_core::TxoRef(dolos_core::TxHash::from(utxo_hash.as_slice()), utxo_id);

        let mut output: Option<MultiEraOutput> = None;

        for era in &eras {
            let o = MultiEraOutput::decode(*era, &bytes);
            if o.is_ok() {
                output = o.ok();
                break;
            }
        }

        if let Some(output) = output {
            println!("UTxO {} found", address);
            delta.produced_utxo.insert(utxo_ref, dolos_core::EraCbor::from(output));
        }
    }

    println!("");

    Ok(vec![delta])
}

fn initialize_initial_utxos(home_dir: &Path, initial_utxos: &Vec<(String, Vec<u8>)>) -> miette::Result<()> {

    let data_dir = initialize_data_dir(home_dir)?;

    initialize_wal_store(&data_dir)?;

    let state = initialize_ledger_store(&data_dir)?;

    let archive = initialize_chain_store(&data_dir)?;

    let deltas = calculate_deltas(initial_utxos)?;

    state.apply(&deltas)
        .map_err(dolos_core::StateError::from)
        .into_diagnostic()
        .context("applying initial utxos to state")?;

    archive.apply(&deltas)
        .map_err(dolos_core::ArchiveError::from)
        .into_diagnostic()
        .context("applying initial utxos to archive")?;

    Ok(())
}

fn save_config(home: &Path, name: &str, content: &str) -> miette::Result<PathBuf> {
    let config = home.join(name);

    std::fs::write(&config, content)
        .into_diagnostic()
        .context("saving config file")?;

    Ok(config)
}

pub fn initialize_config(
    home: &Path,
    initial_funds: &HashMap<String, u64>,
    initial_utxos: &Vec<(String, Vec<u8>)>,
) -> miette::Result<PathBuf> {
    std::fs::create_dir_all(home).into_diagnostic()?;

    save_config(home, "byron.json", BYRON_TEMPLATE)?;

    let shelley_content = initialize_shelley_config(initial_funds)?;
    save_config(home, "shelley.json", &shelley_content)?;

    save_config(home, "alonzo.json", ALONZO_TEMPLATE)?;

    save_config(home, "conway.json", CONWAY_TEMPLATE)?;

    let config_path = save_config(home, "dolos.toml", DOLOS_TEMPLATE)?;

    initialize_initial_utxos(home, initial_utxos)?;

    Ok(config_path)
}

pub fn daemon(home: &Path, background: bool) -> miette::Result<Child> {
    let tool_path = crate::home::tool_path("dolos")?;

    let config_path = home.join("dolos.toml");

    let mut cmd = Command::new(tool_path.to_str().unwrap_or_default());

    cmd.args(["-c", config_path.to_str().unwrap(), "daemon"]);
    cmd.current_dir(home);

    if background {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }

    let child = cmd
        .spawn()
        .into_diagnostic()
        .context("failed to spawn dolos devnet")?;

    Ok(child)
}
