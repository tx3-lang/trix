use std::path::Path;

use crate::config::{DependencyEntry, ProfileConfig, RootConfig};
use crate::dependencies;
use clap::Args as ClapArgs;
use miette::Diagnostic;
use miette::IntoDiagnostic as _;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("check failed in protocol '{name}'")]
struct ProtocolErrors {
    name: String,
    #[related]
    results: Vec<tx3_lang::analyzing::Error>,
}

#[derive(Debug, Error, Diagnostic)]
#[error("check failed")]
struct AggregateError {
    #[related]
    protocols: Vec<ProtocolErrors>,
}

#[derive(ClapArgs, Debug)]
pub struct Args {}

pub fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    config.validate_dependencies()?;
    dependencies::restore_all(config)?;

    let mut all_errors: Vec<ProtocolErrors> = Vec::new();

    let own_errors = check_source(&config.protocol.main)?;
    if !own_errors.is_empty() {
        all_errors.push(ProtocolErrors {
            name: config.protocol.name.clone(),
            results: own_errors,
        });
    }

    for entry in config.dependencies.values() {
        match check_dep(entry) {
            Ok(errs) => {
                if !errs.is_empty() {
                    all_errors.push(ProtocolErrors {
                        name: entry.alias.clone(),
                        results: errs,
                    });
                }
            }
            Err(e) => return Err(e),
        }
    }

    if !all_errors.is_empty() {
        return Err(AggregateError {
            protocols: all_errors,
        }
        .into());
    }

    println!("check passed, no errors found");
    Ok(())
}

fn check_source(path: &Path) -> miette::Result<Vec<tx3_lang::analyzing::Error>> {
    let content = std::fs::read_to_string(path).into_diagnostic()?;
    let mut program = tx3_lang::parsing::parse_string(&content)?;
    let diagnostic = tx3_lang::analyzing::analyze(&mut program);
    Ok(diagnostic.errors)
}

fn check_dep(entry: &DependencyEntry) -> miette::Result<Vec<tx3_lang::analyzing::Error>> {
    let paths = dependencies::cache_paths(entry)?;
    check_source(&paths.source)
}
