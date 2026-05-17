use crate::{
    builder,
    config::{ProfileConfig, RootConfig},
    dependencies,
};
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    config.validate_dependencies()?;
    let _ = builder::build_tii(config)?;
    dependencies::restore_all(config)?;
    builder::validate_dependencies_tii(config)?;

    Ok(())
}
