use miette::IntoDiagnostic as _;

pub mod convention;
pub mod model;
pub mod serde;

use std::path::PathBuf;

pub use convention::*;
pub use model::*;

impl RootConfig {
    pub fn load(path: &PathBuf) -> miette::Result<Self> {
        let contents = std::fs::read_to_string(path).into_diagnostic()?;
        let config: Self = toml::from_str(&contents).into_diagnostic()?;

        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> miette::Result<()> {
        let contents = toml::to_string_pretty(self).into_diagnostic()?;
        std::fs::write(path, contents).into_diagnostic()?;
        Ok(())
    }
}
