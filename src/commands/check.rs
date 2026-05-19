use crate::config::{ProfileConfig, RootConfig};
use crate::spawn::tx3c;
use clap::Args as ClapArgs;
use miette::Diagnostic;
use thiserror::Error;

/// A single analyzer diagnostic, reconstructed from `tx3c`'s JSON contract.
/// `trix` owns the rendering (message + diagnostic code), so the human output
/// is unchanged even though the analysis now runs out-of-process.
#[derive(Debug, Error)]
#[error("{message}")]
struct Diag {
    message: String,
    code: Option<String>,
}

impl Diagnostic for Diag {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.code
            .as_ref()
            .map(|c| Box::new(c.clone()) as Box<dyn std::fmt::Display>)
    }
}

#[derive(Debug, Error, Diagnostic)]
#[error("check failed")]
struct Error {
    #[related]
    results: Vec<Diag>,
}

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let diagnostics = tx3c::check(&config.protocol.main)?;

    if !diagnostics.is_empty() {
        let results = diagnostics
            .into_iter()
            .map(|d| Diag {
                message: d.message,
                code: d.code,
            })
            .collect();
        return Err(Error { results }.into());
    }

    println!("check passed, no errors found");

    Ok(())
}
