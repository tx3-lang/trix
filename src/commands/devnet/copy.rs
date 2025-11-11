use crate::config::{Config, ProfileConfig, U5cConfig};

use utxorpc::{
    Cardano, ChainUtxo, ClientBuilder, QueryClient,
    spec::{cardano::TxOutput, query::TxoRef},
};

use clap::Args as ClapArgs;
use miette::{IntoDiagnostic, bail};

#[derive(ClapArgs)]
pub struct Args {
    /// Profile for network selection
    #[arg(long)]
    profile: String,

    /// Transaction hash to search the UTxO dependencies
    #[arg(long)]
    utxo_deps: String,

    /// Path to save the devnet config file
    #[arg(long)]
    output: Option<String>,
}

pub fn run(args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    let u5c = profile
        .u5c
        .as_ref()
        .ok_or_else(|| miette::miette!("missing u5c config for profile"))?;

    let tx_hash = args.utxo_deps;

    let mut output = crate::dirs::protocol_root()?.join("devnet.toml");
    if let Some(output_arg) = args.output {
        output = std::path::PathBuf::from(output_arg);
    }

    let utxos = futures::executor::block_on(fetch_utxo_deps(u5c, &tx_hash))?;

    let mut devnet = crate::devnet::Config::default();

    for utxo in utxos {
        if let Some(txo_ref) = utxo.txo_ref {
            devnet.utxos.push(crate::devnet::UtxoSpec::Bytes(
                crate::devnet::UtxoSpecBytes {
                    r#ref: format!("{}#{}", hex::encode(txo_ref.hash), txo_ref.index),
                    raw_bytes: hex::encode(utxo.native),
                },
            ));
        }
    }

    let devnet_toml = toml::to_string_pretty(&devnet).into_diagnostic()?;

    std::fs::write(output, devnet_toml).into_diagnostic()?;

    Ok(())
}

async fn fetch_utxo_deps(
    u5c: &U5cConfig,
    tx_hash: &str,
) -> miette::Result<Vec<ChainUtxo<TxOutput>>> {
    let mut client_builder = ClientBuilder::new().uri(&u5c.url).into_diagnostic()?;

    for (key, value) in u5c.headers.iter() {
        client_builder = client_builder.metadata(&key, &value).into_diagnostic()?;
    }

    let mut client = client_builder.build::<QueryClient<Cardano>>().await;

    let tx_hash_bytes = hex::decode(tx_hash).into_diagnostic()?;

    let tx = client
        .read_tx(tx_hash_bytes.into())
        .await
        .into_diagnostic()?;

    if let Some(tx) = tx {
        if let Some(tx) = tx.parsed {
            let utxos = client
                .read_utxos(
                    tx.inputs
                        .iter()
                        .map(|r| TxoRef {
                            hash: r.tx_hash.clone(),
                            index: r.output_index,
                        })
                        .collect(),
                )
                .await
                .into_diagnostic()?;

            return Ok(utxos);
        }
    }

    Ok(vec![])
}
