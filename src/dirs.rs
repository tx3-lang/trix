use std::path::PathBuf;

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
