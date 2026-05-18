//! Local dependency cache for protocols pulled from an OCI registry.
//!
//! Cache layout: `.tx3/protocols/<scope>/<name>/<version>/`
//!     ├── main.tx3       (application/tx3 layer)
//!     ├── main.tii       (application/tii+json layer)
//!     ├── README.md      (text/markdown layer, optional)
//!     └── metadata.json  (ProtocolManifest)
//!
//! Every command that needs deps (`check`, `build`, `codegen`, `inspect`,
//! `invoke`) calls `restore_all` first — it's a no-op when the cache is
//! consistent with `trix.toml`.

pub mod manifest;

pub use manifest::ProtocolManifest;

use std::path::PathBuf;

use miette::{IntoDiagnostic as _, Result};

use crate::config::{DependencyEntry, RootConfig};
use crate::oci;
use crate::refs::ProtocolRef;

pub const CACHE_SOURCE_FILE: &str = "main.tx3";
pub const CACHE_TII_FILE: &str = "main.tii";
pub const CACHE_README_FILE: &str = "README.md";
pub const CACHE_MANIFEST_FILE: &str = "metadata.json";

pub struct CachePaths {
    pub root: PathBuf,
    pub source: PathBuf,
    pub tii: PathBuf,
    pub readme: PathBuf,
    pub manifest: PathBuf,
}

fn registry_parts(entry: &DependencyEntry) -> Result<(&str, &str, &str)> {
    match &entry.reference {
        ProtocolRef::Registry {
            scope,
            name,
            version: Some(v),
        } => Ok((scope.as_str(), name.as_str(), v.as_str())),
        ProtocolRef::Registry { version: None, .. } => Err(miette::miette!(
            "dependency '{}' has no version pinned — run `trix use` to refresh",
            entry.alias
        )),
        ProtocolRef::Alias(a) => Err(miette::miette!(
            "dependency '{}' has alias-only ref '{}'; trix.toml requires a registry reference",
            entry.alias,
            a
        )),
    }
}

pub fn cache_paths(entry: &DependencyEntry) -> Result<CachePaths> {
    let (scope, name, version) = registry_parts(entry)?;
    let root = crate::dirs::protocol_cache_dir(scope, name, version)?;
    Ok(CachePaths {
        source: root.join(CACHE_SOURCE_FILE),
        tii: root.join(CACHE_TII_FILE),
        readme: root.join(CACHE_README_FILE),
        manifest: root.join(CACHE_MANIFEST_FILE),
        root,
    })
}

/// Outcome of inspecting a dependency's local cache.
pub enum CacheStatus {
    /// Present, parses, and digest matches `trix.toml`.
    Valid,
    /// A required file is absent — the caller may fetch it.
    Missing,
    /// Present but inconsistent (digest mismatch, corrupt metadata, or a
    /// malformed TII). The caller should surface this rather than refetch,
    /// since cache and lockfile genuinely disagree.
    Invalid(miette::Report),
}

/// Inspects the cache for `entry` in a single pass: at most one stat per
/// file and one parse of metadata.json / main.tii. The outer `Result` is
/// reserved for unexpected I/O failures.
pub fn verify_cached(entry: &DependencyEntry) -> Result<CacheStatus> {
    let paths = cache_paths(entry)?;

    let manifest_bytes = match std::fs::read(&paths.manifest) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(CacheStatus::Missing),
        Err(e) => return Err(e).into_diagnostic(),
    };
    if !paths.source.exists() {
        return Ok(CacheStatus::Missing);
    }

    let manifest: ProtocolManifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            return Ok(CacheStatus::Invalid(miette::miette!(
                "dependency '{}' cache has malformed metadata.json: {}",
                entry.alias,
                e
            )));
        }
    };
    if manifest.digest != entry.digest {
        return Ok(CacheStatus::Invalid(miette::miette!(
            "dependency '{}' cache digest '{}' does not match trix.toml digest '{}'. \
             Run `trix use --force {}` to refresh.",
            entry.alias,
            manifest.digest,
            entry.digest,
            entry.reference
        )));
    }

    let tii_bytes = match std::fs::read(&paths.tii) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(CacheStatus::Missing),
        Err(e) => return Err(e).into_diagnostic(),
    };
    if let Err(e) = serde_json::from_slice::<serde_json::Value>(&tii_bytes) {
        return Ok(CacheStatus::Invalid(miette::miette!(
            "dependency '{}' cached TII is not valid JSON: {}",
            entry.alias,
            e
        )));
    }

    Ok(CacheStatus::Valid)
}

/// Re-download and overwrite the cache for one dep.
pub fn fetch(entry: &DependencyEntry, config: &RootConfig) -> Result<()> {
    let registry_url = config.registry_url();

    let oci_reference = crate::oci::reference_for(&registry_url, &entry.reference)?;
    let client = crate::oci::client_for(&registry_url);

    let pulled =
        futures::executor::block_on(crate::oci::pull(&client, &oci_reference))?;

    if pulled.digest != entry.digest {
        return Err(miette::miette!(
            "dependency '{}' registry digest '{}' no longer matches trix.toml digest '{}'. \
             The published image has been rotated. Run `trix use --force {}` to repin.",
            entry.alias,
            pulled.digest,
            entry.digest,
            entry.reference
        ));
    }

    let paths = cache_paths(entry)?;
    write_cache(&paths, &pulled, entry)?;
    Ok(())
}

pub(crate) fn write_cache(
    paths: &CachePaths,
    pulled: &oci::PulledArtifact,
    entry: &DependencyEntry,
) -> Result<()> {
    let (scope, name, version) = registry_parts(entry)?;
    std::fs::write(&paths.source, &pulled.source).into_diagnostic()?;
    std::fs::write(&paths.tii, &pulled.tii).into_diagnostic()?;
    if let Some(readme_bytes) = &pulled.readme {
        std::fs::write(&paths.readme, readme_bytes).into_diagnostic()?;
    } else if paths.readme.exists() {
        std::fs::remove_file(&paths.readme).into_diagnostic()?;
    }
    let manifest = ProtocolManifest {
        scope: scope.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        digest: pulled.digest.clone(),
        published_date: pulled.metadata.published_date,
        description: pulled.metadata.description.clone(),
        repository_url: pulled.metadata.repository_url.clone(),
        fetched_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        has_readme: pulled.readme.is_some(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).into_diagnostic()?;
    std::fs::write(&paths.manifest, manifest_bytes).into_diagnostic()?;
    Ok(())
}

/// For every entry in `config.dependencies`, verify the cache. If a dep is
/// merely missing from disk we attempt to re-download it; if the cache is
/// present but inconsistent (digest mismatch, corrupt metadata, malformed
/// TII), we surface the verification error directly so the user knows their
/// cache and lockfile disagree. No-op when `dependencies` is empty.
pub fn restore_all(config: &RootConfig) -> Result<()> {
    if config.dependencies.is_empty() {
        return Ok(());
    }
    for entry in config.dependencies.values() {
        match verify_cached(entry)? {
            CacheStatus::Valid => {}
            CacheStatus::Invalid(report) => return Err(report),
            CacheStatus::Missing => {
                eprintln!("restoring dependency '{}'...", entry.alias);
                fetch(entry, config)?;
            }
        }
    }
    Ok(())
}
