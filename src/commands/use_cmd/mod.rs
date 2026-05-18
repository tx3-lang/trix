use clap::Args as ClapArgs;

use crate::config::{ProfileConfig, RootConfig};
use crate::dependencies::{self, AddRequest};
use crate::refs::ProtocolRef;

mod view;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Reference to pull, e.g. `acme/widget:0.1.0`. The version is optional
    /// and defaults to the `latest` tag; the resolved concrete version is
    /// pinned in trix.toml.
    #[arg(value_name = "REFERENCE", value_parser = ProtocolRef::parse_registry)]
    pub reference: ProtocolRef,

    /// Local alias for this dependency. Defaults to the reference's name.
    #[arg(long)]
    pub alias: Option<String>,

    /// Replace an existing entry with the same alias.
    #[arg(long)]
    pub force: bool,

    /// Resolve and cache the artifact but do not modify trix.toml.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let dry_run = args.dry_run;

    let outcome = dependencies::add(
        config,
        AddRequest {
            reference: args.reference,
            alias: args.alias,
            force: args.force,
            dry_run,
        },
    )?;

    view::render(&view::UseView {
        alias: outcome.alias,
        reference: outcome.reference.to_string(),
        digest: outcome.digest,
        cache_path: outcome.cache_root.display().to_string(),
        transactions: outcome.transactions,
        replaced: outcome.replaced,
        dry_run,
    });

    Ok(())
}
