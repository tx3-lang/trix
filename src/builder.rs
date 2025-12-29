use std::path::PathBuf;

use crate::{
    config::{ProfileConfig, RootConfig},
    spawn,
};

fn define_tii_output_path() -> miette::Result<PathBuf> {
    let out = crate::dirs::target_dir("tii")?.join("main.tii");

    Ok(out)
}

pub fn build_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let source = config.protocol.main.clone();

    let output_path = define_tii_output_path()?;

    spawn::tx3c::build_tii(&source, &output_path, &config)?;

    Ok(output_path)
}

pub fn ensure_tii(config: &RootConfig) -> miette::Result<PathBuf> {
    let output_path = define_tii_output_path()?;

    if !output_path.exists() {
        build_tii(config)?;
    }

    Ok(output_path)
}
