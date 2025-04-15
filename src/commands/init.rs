use crate::config::Config;
use anyhow::Result;
use clap::Args as ClapArgs;
use std::path::PathBuf;

#[derive(ClapArgs)]
pub struct Args {}

pub fn run(_args: Args, _config: &Config) -> Result<()> {
    todo!()
}
