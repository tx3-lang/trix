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
}

pub fn run(args: Args, config: &Config) -> miette::Result<()> {
    let devnet_home = crate::commands::devnet::ensure_devnet_home(config)?;

    let args_json = if let Some(args_json) = args.args_json {
        Some(args_json)
    } else if let Some(path) = args.args_json_path {
        Some(
            std::fs::read_to_string(path)
                .into_diagnostic()
                .context("failed to read args json file")?,
        )
    } else {
        None
    };

    let cononical = config.protocol.main.canonicalize().into_diagnostic()?;

    if !cononical.is_file() {
        bail!(
            "The main protocol file is not a file: {}",
            cononical.display()
        );
    }

    let mut child =
        crate::spawn::cshell::transation_interactive(&devnet_home, &cononical, args_json)?;

    let status = child
        .wait()
        .into_diagnostic()
        .context("failed to wait for cshell explorer")?;

    if !status.success() {
        bail!("cshell exited with code: {}", status);
    }

    Ok(())
}
