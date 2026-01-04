use crate::{
    builder,
    config::{ProfileConfig, RootConfig},
};
use clap::Args as ClapArgs;

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    crate::telemetry::track_command_execution("build");

    let _ = builder::build_tii(config)?;

    Ok(())
}
