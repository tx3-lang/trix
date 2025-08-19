use std::path::PathBuf;

use clap::Args as ClapArgs;
use miette::{Context, IntoDiagnostic, bail};

use crate::config::Config;

#[derive(ClapArgs)]
pub struct Args {
    /// Args for the TX3 transaction as a raw JSON string.
    #[arg(long)]
    args_json: Option<String>,

    /// Path to a JSON file with arguments for the TX3 transaction.
    #[arg(long)]
    args_json_path: Option<PathBuf>,

    /// Skip submitting the transaction.
    #[arg(long)]
    skip_submit: bool,
}

fn load_args_json(args: &Args) -> miette::Result<Option<serde_json::Value>> {
    if let Some(args_json) = &args.args_json {
        let value = serde_json::from_str(args_json).into_diagnostic()?;
        return Ok(Some(value));
    }

    if let Some(path) = &args.args_json_path {
        let args_json = std::fs::read_to_string(path).into_diagnostic()?;
        let value = serde_json::from_str(&args_json).into_diagnostic()?;
        return Ok(Some(value));
    }

    Ok(None)
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::commands::devnet::ensure_devnet_home(config)?;

    let cononical = config.protocol.main.canonicalize().into_diagnostic()?;

    if !cononical.is_file() {
        bail!(
            "The main protocol file is not a file: {}",
            cononical.display()
        );
    }

    let args_json = load_args_json(&args)?;

    crate::spawn::cshell::tx_invoke_interactive(
        &devnet_home,
        &cononical,
        &args_json,
        None,
        vec![],
        true,
        args.skip_submit,
    )?;

    Ok(())
}
