use clap::Args as ClapArgs;

use crate::config::{ProfileConfig, RootConfig};

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    let wallet = crate::wallet::setup(config, profile)?;

    wallet.explorer(profile.name.as_str())?;

    Ok(())
}
