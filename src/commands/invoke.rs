use std::path::PathBuf;

use clap::Args as ClapArgs;
use miette::IntoDiagnostic;

use crate::{
    builder,
    config::{ProfileConfig, RootConfig},
    dependencies,
    refs::{ResolvedProtocol, Resolver, TxRef},
};

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Transaction reference. Bare names target the project's own protocol;
    /// `<alias>::<tx>` or `<scope>/<name>:<version>::<tx>` target a
    /// dependency. The tx portion may be omitted to keep the wallet's
    /// interactive picker.
    #[arg(long, value_parser = parse_tx)]
    tx: Option<TxRef>,

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

fn parse_tx(s: &str) -> Result<TxRef, String> {
    TxRef::parse(s).map_err(|e| e.to_string())
}

pub type ArgMap = serde_json::Map<String, serde_json::Value>;

fn merge_json_maps_mut(a: &mut ArgMap, b: &ArgMap) {
    for (key, value) in b {
        a.insert(key.clone(), value.clone());
    }
}

fn string_to_json_map(s: &str) -> miette::Result<ArgMap> {
    let value = serde_json::from_str::<serde_json::Value>(s).into_diagnostic()?;

    let value = value
        .as_object()
        .ok_or(miette::miette!("json args should be an object"))?;

    Ok(value.to_owned())
}

fn load_args_json(args: &Args) -> miette::Result<serde_json::Value> {
    let mut all = serde_json::Map::new();

    if let Some(args_json) = &args.args_json {
        let value = string_to_json_map(args_json)?;
        merge_json_maps_mut(&mut all, &value);
    }

    if let Some(path) = &args.args_json_path {
        let args_json = std::fs::read_to_string(path).into_diagnostic()?;
        let value = string_to_json_map(&args_json)?;
        merge_json_maps_mut(&mut all, &value);
    }

    Ok(serde_json::Value::Object(all))
}

pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    config.validate_dependencies()?;
    dependencies::restore_all(config)?;

    let wallet = crate::wallet::setup(config, profile)?;

    let tii_file = resolve_tii_path(&args, config)?;

    let args_json = load_args_json(&args)?;

    wallet.invoke_interactive(&tii_file, &args_json, &profile.name, args.skip_submit)?;

    Ok(())
}

fn resolve_tii_path(args: &Args, config: &RootConfig) -> miette::Result<PathBuf> {
    let Some(tx) = &args.tx else {
        return builder::build_tii(config);
    };

    let resolver = Resolver::new(config);
    let (resolved, _) = resolver.resolve_tx(tx)?;
    match resolved {
        ResolvedProtocol::Project => builder::build_tii(config),
        ResolvedProtocol::Dependency(entry) => {
            let paths = dependencies::cache_paths(entry)?;
            Ok(paths.tii)
        }
    }
}
