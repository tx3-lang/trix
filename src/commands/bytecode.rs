use crate::config::Config;
use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic};
use tx3_lang::Protocol;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &Config) -> miette::Result<()> {
    let main_path = config.protocol.main.clone();

    let protocol = Protocol::from_file(main_path)
        .load()
        .into_diagnostic()
        .context("parsing tx3 file")?;

    for tx in protocol.txs() {
        let prototx = protocol.new_tx(&tx.name).unwrap();

        let hex = hex::encode(prototx.ir_bytes());

        println!("{} {hex}", tx.name);
    }

    Ok(())
}
