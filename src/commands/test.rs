use crate::config::Config;
use clap::Args as ClapArgs;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, _config: &Config) -> miette::Result<()> {
    Ok(())
}
