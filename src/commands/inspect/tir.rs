use std::path::PathBuf;

use clap::Args as ClapArgs;
use miette::{Context as _, IntoDiagnostic as _};

use crate::config::RootConfig;
use crate::dependencies::{self, ResolvedProtocol, Resolver};
use crate::refs::TxRef;

#[derive(ClapArgs)]
pub struct Args {
    /// Transaction reference. Forms accepted:
    ///   "transfer"                         (project's own protocol)
    ///   "widget::transfer"                 (dependency by alias)
    ///   "acme/widget:0.1.0::transfer"      (fully qualified registry ref)
    #[arg(short, long, value_parser = TxRef::parse)]
    tx: TxRef,

    #[arg(long)]
    pretty: bool,
}

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    config.validate_dependencies()?;
    dependencies::restore_all(config)?;

    let resolver = Resolver::new(config);
    let (resolved, tx_name) = resolver.resolve_tx(&args.tx)?;

    let source_path: PathBuf = match resolved {
        ResolvedProtocol::Project => config.protocol.main.clone(),
        ResolvedProtocol::Dependency(entry) => dependencies::cache_paths(entry)?.source,
    };

    let content = std::fs::read_to_string(&source_path).into_diagnostic()?;
    let mut ast = tx3_lang::parsing::parse_string(&content)?;
    tx3_lang::analyzing::analyze(&mut ast).ok()?;

    let ir = tx3_lang::lowering::lower(&ast, tx_name)
        .into_diagnostic()
        .with_context(|| format!("lowering {}", tx_name))?;

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&ir).into_diagnostic()?);
    } else {
        println!("{}", serde_json::to_string(&ir).into_diagnostic()?);
    }

    Ok(())
}
