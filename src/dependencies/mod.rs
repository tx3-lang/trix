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

use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic as _, Result};

use crate::config::{DependencyEntry, RootConfig};
use crate::oci;
use crate::refs::ProtocolRef;

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
        source: root.join("main.tx3"),
        tii: root.join("main.tii"),
        readme: root.join("README.md"),
        manifest: root.join("metadata.json"),
        root,
    })
}

/// Returns `Ok(())` only when the cache exists, parses, and the digest
/// matches `entry.digest`. Returns `Err` (variant in the message) otherwise
/// so the caller can decide whether to fetch or surface the error.
pub fn verify_cached(entry: &DependencyEntry) -> Result<()> {
    let paths = cache_paths(entry)?;
    if !paths.source.exists() {
        return Err(miette::miette!(
            "dependency '{}' cache missing: {}",
            entry.alias,
            paths.source.display()
        ));
    }
    if !paths.tii.exists() {
        return Err(miette::miette!(
            "dependency '{}' cache missing TII: {}",
            entry.alias,
            paths.tii.display()
        ));
    }
    if !paths.manifest.exists() {
        return Err(miette::miette!(
            "dependency '{}' cache missing manifest: {}",
            entry.alias,
            paths.manifest.display()
        ));
    }
    let manifest_bytes = std::fs::read(&paths.manifest).into_diagnostic()?;
    let manifest: ProtocolManifest = serde_json::from_slice(&manifest_bytes)
        .into_diagnostic()
        .map_err(|e| {
            miette::miette!(
                "dependency '{}' cache has malformed metadata.json: {}",
                entry.alias,
                e
            )
        })?;
    if manifest.digest != entry.digest {
        return Err(miette::miette!(
            "dependency '{}' cache digest '{}' does not match trix.toml digest '{}'. \
             Run `trix use --force {}` to refresh.",
            entry.alias,
            manifest.digest,
            entry.digest,
            entry.reference
        ));
    }
    // Make sure the TII layer is at least well-formed JSON.
    let tii_bytes = std::fs::read(&paths.tii).into_diagnostic()?;
    serde_json::from_slice::<serde_json::Value>(&tii_bytes)
        .into_diagnostic()
        .map_err(|e| {
            miette::miette!(
                "dependency '{}' cached TII is not valid JSON: {}",
                entry.alias,
                e
            )
        })?;
    Ok(())
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
        if verify_cached(entry).is_ok() {
            continue;
        }
        let paths = cache_paths(entry)?;
        let cache_present = paths.source.exists() && paths.tii.exists() && paths.manifest.exists();
        if cache_present {
            // Re-verify to bubble the precise diagnostic (digest mismatch,
            // malformed metadata, etc.) instead of obscuring it behind a fetch.
            return verify_cached(entry);
        }
        eprintln!("restoring dependency '{}'...", entry.alias);
        fetch(entry, config)?;
    }
    Ok(())
}

/// Convenience: load and parse a cached TII as raw JSON for callers that
/// don't need the whole `tx3_tir` model.
pub fn load_tii_json(tii_path: &Path) -> Result<serde_json::Value> {
    let bytes = std::fs::read(tii_path).into_diagnostic()?;
    serde_json::from_slice(&bytes).into_diagnostic()
}
