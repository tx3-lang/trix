use crate::{
    builder,
    config::{ProfileConfig, RootConfig},
};
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {}

/// `build` is strictly project-only: it produces the project's own TII and
/// nothing else. External protocol interfaces are an orthogonal concern, not
/// inputs to this build — they are materialized/verified lazily by the
/// commands that actually consume them (`invoke`, `codegen`, `inspect tir`).
pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let _ = builder::build_tii(config)?;

    Ok(())
}
