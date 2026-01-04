use crate::config::{ProfileConfig, RootConfig};
use clap::Args as ClapArgs;
use miette::Diagnostic;
use miette::IntoDiagnostic as _;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("check failed")]
struct Error {
    #[related]
    results: Vec<tx3_lang::analyzing::Error>,
}

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    crate::telemetry::track_command_execution("check");

    let main_path = config.protocol.main.clone();

    let content = std::fs::read_to_string(main_path).into_diagnostic()?;

    let mut program = tx3_lang::parsing::parse_string(&content)?;

    let diagnostic = tx3_lang::analyzing::analyze(&mut program);

    if !diagnostic.errors.is_empty() {
        return Err(Error {
            results: diagnostic.errors,
        }
        .into());
    }

    println!("check passed, no errors found");
    Ok(())
}
