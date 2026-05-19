use clap::Args as ClapArgs;
use miette::IntoDiagnostic as _;

use crate::config::RootConfig;
use crate::interfaces::{self, ResolvedProtocol, Resolver};
use crate::refs::TxRef;
use crate::spawn::tx3c;

#[derive(ClapArgs)]
pub struct Args {
    /// Transaction reference. Forms accepted:
    ///   "transfer"                         (project's own protocol)
    ///   "widget::transfer"                 (interface by alias)
    ///   "acme/widget:0.1.0::transfer"      (fully qualified registry ref)
    #[arg(short, long, value_parser = TxRef::parse)]
    tx: TxRef,

    #[arg(long)]
    pretty: bool,
}

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    // `inspect tir` is a consuming command: it may target an interface, so it
    // applies the interface integrity gate up front, exactly like `invoke`
    // and `codegen`. A no-op when no interfaces are declared.
    interfaces::validate(config)?;
    interfaces::restore_all(config)?;

    let resolver = Resolver::new(config);
    let (resolved, tx_name) = resolver.resolve_tx(&args.tx)?;

    let ir = match resolved {
        // The author's own source is normative for the project; `tx3c` lowers
        // it. An interface is consumed via its published TII, whose encoded
        // TIR `tx3c` decodes. Both paths yield the same JSON shape, so the
        // caller can't tell which protocol it came from.
        ResolvedProtocol::Project => {
            tx3c::tir_from_source(&config.protocol.main, tx_name)?
        }
        ResolvedProtocol::Interface(entry) => {
            tx3c::decode_tir(&interfaces::cache_paths(entry)?.tii, tx_name)?
        }
    };

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&ir).into_diagnostic()?);
    } else {
        println!("{}", serde_json::to_string(&ir).into_diagnostic()?);
    }

    Ok(())
}
