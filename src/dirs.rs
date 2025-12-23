use std::path::PathBuf;

use miette::{Context as _, IntoDiagnostic as _};

use crate::config::ProtocolConfig;

// crawl up the directory tree until we find a trix.toml file
pub fn protocol_root() -> miette::Result<PathBuf> {
    let mut cwd = std::env::current_dir().unwrap();

    loop {
        if cwd.join("trix.toml").exists() {
            return Ok(cwd);
        }

        let Some(parent) = cwd.parent() else {
            return Err(miette::miette!("No trix.toml found in current directory"));
        };

        cwd = parent.to_path_buf();
    }
}

pub fn toolchain_owned_dir() -> miette::Result<PathBuf> {
    let root = protocol_root()?;

    let target = root.join(".tx3");

    if !target.exists() {
        std::fs::create_dir_all(&target)
            .into_diagnostic()
            .context("creating tx3 target directory")?;
    }

    Ok(target)
}

pub fn target_dir(artifact_kind: &str) -> miette::Result<PathBuf> {
    let mut target = toolchain_owned_dir()?;

    target.push(artifact_kind);

    if !target.exists() {
        std::fs::create_dir_all(&target)
            .into_diagnostic()
            .context("creating tx3 target directory")?;
    }

    Ok(target)
}
