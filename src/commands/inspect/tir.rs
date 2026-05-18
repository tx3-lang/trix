use std::collections::HashMap;

use clap::Args as ClapArgs;
use miette::{Context as _, IntoDiagnostic as _};
use serde::Deserialize;

use crate::config::RootConfig;
use crate::interfaces::{self, ResolvedProtocol, Resolver};
use crate::refs::TxRef;

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

// Minimal mirror of the parts of the published TII we need to recover a
// transaction's TIR. The cached `.tii` is the *normative* artifact; we never
// re-derive IR from the (informative) cached `.tx3` source.
#[derive(Deserialize)]
struct TiiLite {
    transactions: HashMap<String, TxLite>,
}

#[derive(Deserialize)]
struct TxLite {
    tir: TirEnvelopeLite,
}

#[derive(Deserialize)]
struct TirEnvelopeLite {
    content: String,
    #[serde(rename = "contentType", alias = "encoding")]
    encoding: TirEncodingLite,
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum TirEncodingLite {
    Hex,
    Base64,
}

pub fn run(args: Args, config: &RootConfig) -> miette::Result<()> {
    // `inspect tir` is a consuming command: it may target an interface, so it
    // applies the interface integrity gate up front, exactly like `invoke`
    // and `codegen`. A no-op when no interfaces are declared.
    config.validate_interfaces()?;
    interfaces::restore_all(config)?;

    let resolver = Resolver::new(config);
    let (resolved, tx_name) = resolver.resolve_tx(&args.tx)?;

    let ir = match resolved {
        // The author's own source is normative for the project.
        ResolvedProtocol::Project => {
            let content =
                std::fs::read_to_string(&config.protocol.main).into_diagnostic()?;
            let mut ast = tx3_lang::parsing::parse_string(&content)?;
            tx3_lang::analyzing::analyze(&mut ast).ok()?;
            let lowered = tx3_lang::lowering::lower(&ast, tx_name)
                .into_diagnostic()
                .with_context(|| format!("lowering {}", tx_name))?;
            serde_json::to_value(&lowered).into_diagnostic()?
        }
        // An interface is consumed via its published TII; it carries the
        // encoded TIR per transaction, so no source compilation is involved.
        ResolvedProtocol::Interface(entry) => {
            tir_from_interface(interfaces::cache_paths(entry)?.tii, tx_name)?
        }
    };

    if args.pretty {
        println!("{}", serde_json::to_string_pretty(&ir).into_diagnostic()?);
    } else {
        println!("{}", serde_json::to_string(&ir).into_diagnostic()?);
    }

    Ok(())
}

/// Decode `tx_name`'s TIR straight out of an interface's cached, published
/// `.tii`. Returns the same JSON shape the project path emits (the inner
/// `v1beta0` tx), so callers can't tell which protocol it came from.
fn tir_from_interface(
    tii_path: std::path::PathBuf,
    tx_name: &str,
) -> miette::Result<serde_json::Value> {
    let bytes = std::fs::read(&tii_path).into_diagnostic()?;
    let tii: TiiLite = serde_json::from_slice(&bytes)
        .into_diagnostic()
        .context("parsing cached TII")?;

    let envelope = tii
        .transactions
        .get(tx_name)
        .map(|t| &t.tir)
        .ok_or_else(|| miette::miette!("transaction '{}' not found in interface", tx_name))?;

    let raw = match envelope.encoding {
        TirEncodingLite::Hex => hex::decode(&envelope.content)
            .into_diagnostic()
            .context("decoding hex TIR")?,
        TirEncodingLite::Base64 => {
            return Err(miette::miette!(
                "interface TIR is base64-encoded, which this trix does not support; \
                 ask the publisher to re-`trix publish`"
            ));
        }
    };

    let version = tx3_tir::encoding::TirVersion::try_from(envelope.version.as_str())
        .map_err(|e| miette::miette!("unsupported TIR version: {e}"))?;

    let any = tx3_tir::encoding::from_bytes(&raw, version)
        .map_err(|e| miette::miette!("decoding TIR: {e}"))?;

    let tx3_tir::encoding::AnyTir::V1Beta0(tx) = any;
    serde_json::to_value(&tx).into_diagnostic()
}
