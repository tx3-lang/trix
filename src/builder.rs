use std::path::PathBuf;

use crate::{
    config::{Config, ProfileConfig},
    spawn,
};

fn define_tii_output_path() -> miette::Result<PathBuf> {
    let out = crate::dirs::target_dir("tii")?.join("main.tii");

    Ok(out)
}

pub fn build_tii(config: &Config, profile: &ProfileConfig) -> miette::Result<PathBuf> {
    let source = config.protocol.main.clone();

    let output_path = define_tii_output_path()?;

    let env_file = profile.env_file_path();

    let env_file = if std::path::Path::exists(&env_file) {
        Some(env_file.as_path())
    } else {
        None
    };

    spawn::tx3c::build_tii(&source, &output_path, &config.protocol, env_file)?;

    Ok(output_path)
}

pub fn ensure_tii(config: &Config, profile: &ProfileConfig) -> miette::Result<PathBuf> {
    let output_path = define_tii_output_path()?;

    if !output_path.exists() {
        build_tii(config, profile)?;
    }

    Ok(output_path)
}
