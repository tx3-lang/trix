use std::path::PathBuf;

use crate::{config::RootConfig, spawn};

/// The project's own TII lands in the same `.tx3/tii/<scope>/<name>/<version>/`
/// tree as fetched interfaces — one uniform layout. Falls back to the `local`
/// scope when `[protocol] scope` is absent.
fn define_tii_output_path(config: &RootConfig) -> miette::Result<PathBuf> {
    let scope = config
        .protocol
        .scope
        .as_deref()
        .unwrap_or(crate::dirs::LOCAL_SCOPE);

    let dir = crate::dirs::tii_dir(scope, &config.protocol.name, &config.protocol.version)?;

    Ok(dir.join("main.tii"))
}

pub fn build_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let source = config.protocol.main.clone();

    let output_path = define_tii_output_path(config)?;

    spawn::tx3c::build_tii(&source, &output_path, config)?;

    Ok(output_path)
}

#[allow(dead_code)]
pub fn ensure_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let output_path = define_tii_output_path(config)?;

    if !output_path.exists() {
        build_tii(config)?;
    }

    Ok(output_path)
}
