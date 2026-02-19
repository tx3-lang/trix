use clap::Args as ClapArgs;
use miette::Result;

use crate::config::{ProfileConfig, RootConfig};

#[derive(ClapArgs)]
pub struct Args {
    /// Path where the incremental analysis state JSON will be written.
    #[arg(long, default_value = ".tx3/aiken-analysis/state.json")]
    pub state_out: String,

    /// Path where the final vulnerability report markdown will be written.
    #[arg(long, default_value = ".tx3/aiken-analysis/vulnerabilities.md")]
    pub report_out: String,

    /// Path to vulnerability skill definitions.
    #[arg(long, default_value = "skills/vulnerabilities")]
    pub skills_dir: String,
}

pub fn run(_args: Args, _config: &RootConfig, _profile: &ProfileConfig) -> Result<()> {
    println!("⚠️  EXPERIMENTAL: Aiken vulnerability analysis scaffolding is not implemented yet.");
    println!("See design/003-ai-aiken-vulnerability-scaffolding.md for architecture and contracts.");
    Ok(())
}
