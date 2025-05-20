use std::{fs, io::Write};

use crate::config::Config;
use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic};
use serde_json::json;
use tx3_lang::Protocol;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to save to a json file
    #[arg(short, long)]
    output: Option<String>,
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let main_path = config.protocol.main.clone();

    let protocol = Protocol::from_file(main_path)
        .load()
        .into_diagnostic()
        .context("parsing tx3 file")?;

    let values = protocol
        .txs()
        .map(|tx| {
            let prototx = protocol.new_tx(&tx.name).unwrap();

            let hex = hex::encode(prototx.ir_bytes());

            json!({
                "bytecode": hex,
                "encoding": "hex",
                "version": tx3_lang::ir::IR_VERSION
            })
        })
        .collect::<Vec<_>>();

    let json = serde_json::to_string_pretty(&values).into_diagnostic()?;

    if let Some(output) = args.output {
        let mut file = fs::File::create(&output)
            .into_diagnostic()
            .context("invalid output path")?;

        file.write_all(json.as_bytes()).into_diagnostic()?;
    } else {
        println!("{json}");
    }

    Ok(())
}
