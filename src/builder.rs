use std::path::PathBuf;

use miette::IntoDiagnostic as _;

use crate::{config::RootConfig, dependencies, spawn};

fn define_tii_output_path() -> miette::Result<PathBuf> {
    let out = crate::dirs::target_dir("tii")?.join("main.tii");

    Ok(out)
}

pub fn build_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let source = config.protocol.main.clone();

    let output_path = define_tii_output_path()?;

    spawn::tx3c::build_tii(&source, &output_path, config)?;

    Ok(output_path)
}

#[allow(dead_code)]
pub fn ensure_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let output_path = define_tii_output_path()?;

    if !output_path.exists() {
        build_tii(config)?;
    }

    Ok(output_path)
}

/// Validates that every dependency's cached TII parses as JSON. Called by
/// `trix build` after dependencies have been restored.
pub fn validate_dependencies_tii(config: &RootConfig) -> miette::Result<()> {
    for entry in config.dependencies.values() {
        let paths = dependencies::cache_paths(entry)?;
        let bytes = std::fs::read(&paths.tii).into_diagnostic()?;
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .into_diagnostic()
            .map_err(|e| {
                miette::miette!(
                    "dependency '{}' has malformed TII at {}: {}",
                    entry.alias,
                    paths.tii.display(),
                    e
                )
            })?;
    }
    Ok(())
}
