use crate::{
    builder,
    config::{Config, ProfileConfig},
};
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(args: Args, config: &Config, profile: &ProfileConfig) -> miette::Result<()> {
    let _ = builder::ensure_tii(config, profile)?;

    Ok(())
}
