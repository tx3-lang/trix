use crate::{
    builder,
    config::{ProfileConfig, RootConfig},
};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let _ = builder::build_tii(config)?;

    Ok(())
}
