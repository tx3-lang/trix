/// Utility functions for the home directory
use std::path::PathBuf;

use cryptoxide::{digest::Digest as _, sha2::Sha256};
use miette::{Context as _, IntoDiagnostic as _};

pub fn tx3_dir() -> miette::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| miette::miette!("failed to get home directory"))?
        .join(".tx3");

    if !home.exists() {
        std::fs::create_dir_all(&home)
            .into_diagnostic()
            .context("failed to create tx3 home directory")?;
    }

    Ok(home)
}

pub fn bin_dir() -> miette::Result<PathBuf> {
    let home = tx3_dir()?;

    let channel = home.join("default");
    let bin = channel.join("bin");

    if !bin.exists() {
        std::fs::create_dir_all(&bin)
            .into_diagnostic()
            .context("failed to create tx3 bin directory")?;
    }

    Ok(bin)
}

pub fn default_tool_path(name: &str) -> miette::Result<PathBuf> {
    let bin = bin_dir()?;

    let mut file = bin.join(name);

    if cfg!(target_os = "windows") {
        file.set_extension("exe");
    }

    if !file.is_file() {
        miette::bail!(
            help = "please run tx3up or make sure your tx3 toolchain is correctly installed",
            "tool {} not found",
            name
        );
    }

    Ok(file)
}

pub fn custom_tool_path(name: &str) -> miette::Result<Option<PathBuf>> {
    let var = format!("TX3_{}_PATH", name.to_uppercase());

    let Ok(path) = std::env::var(var) else {
        return Ok(None);
    };

    Ok(Some(PathBuf::from(path)))
}

pub fn tool_path(name: &str) -> miette::Result<PathBuf> {
    match custom_tool_path(name)? {
        Some(path) => Ok(path),
        None => default_tool_path(name),
    }
}

pub fn tmp_dir() -> miette::Result<PathBuf> {
    let home = tx3_dir()?;

    let tmp = home.join("tmp");

    if !tmp.exists() {
        std::fs::create_dir_all(&tmp)
            .into_diagnostic()
            .context("failed to create tx3 tmp directory")?;
    }

    Ok(tmp)
}

pub fn consistent_tmp_dir(prefix: &str, hashable: &[u8]) -> miette::Result<PathBuf> {
    let tmp = tmp_dir()?;

    if !tmp.exists() {
        std::fs::create_dir_all(&tmp)
            .into_diagnostic()
            .context("failed to create tx3 tmp directory")?;
    }

    let mut hasher = Sha256::new();

    hasher.input(hashable);
    let hex = hasher.result_str();
    let truncated = &hex[..16];

    let path = tmp.join(format!("{prefix}_{truncated}"));

    Ok(path)
}
