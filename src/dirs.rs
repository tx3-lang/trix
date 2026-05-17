use std::path::PathBuf;

use miette::{Context as _, IntoDiagnostic as _};

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

/// Root of the local dependency cache: `.tx3/protocols/`.
pub fn protocols_cache_dir() -> miette::Result<PathBuf> {
    target_dir("protocols")
}

/// Cache directory for a single protocol artifact:
/// `.tx3/protocols/<scope>/<name>/<version>/`.
/// Creates the directory tree if missing.
pub fn protocol_cache_dir(scope: &str, name: &str, version: &str) -> miette::Result<PathBuf> {
    let mut p = protocols_cache_dir()?;
    p.push(scope);
    p.push(name);
    p.push(version);
    if !p.exists() {
        std::fs::create_dir_all(&p)
            .into_diagnostic()
            .context("creating protocol cache directory")?;
    }
    Ok(p)
}
