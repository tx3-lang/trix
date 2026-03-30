use clap::Args as ClapArgs;
use miette::{Context as _, IntoDiagnostic as _};

use crate::config::RootConfig;
use crate::dirs;

#[derive(ClapArgs)]
pub struct Args {
    /// Path to plutus.json (relative to project root or absolute)
    #[arg(long)]
    path: String,

    /// Optional alias (same as in `import "…" as alias;`)
    #[arg(long)]
    alias: Option<String>,
}

pub fn run(args: Args, _config: &RootConfig) -> miette::Result<()> {
    let root = dirs::protocol_root()?;
    let loader = tx3_lang::importing::FsLoader::new(root);

    let type_defs = tx3_lang::importing::types_from_plutus(
        &args.path,
        args.alias.as_deref(),
        &loader,
    )
    .into_diagnostic()
    .with_context(|| format!("loading {}", args.path))?;

    let alias_label = args
        .alias
        .as_deref()
        .map(|a| format!(" (alias: {})", a))
        .unwrap_or_default();
    println!("Types from {}{}:", args.path, alias_label);
    for type_def in &type_defs {
        println!("{}", type_def.to_tx3_source());
    }

    Ok(())
}
