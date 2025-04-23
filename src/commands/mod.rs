use anyhow::Result;
use std::path::PathBuf;

pub mod bindgen;
pub mod check;
pub mod devnet;
pub mod init;

pub(crate) fn find_config() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let config_path = current_dir.join("trix.toml");
    if !config_path.exists() {
        anyhow::bail!("No trix.toml found in current directory");
    }
    Ok(config_path)
}
