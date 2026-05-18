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

/// Resolve + anonymously pull an OCI artifact for `reference` against the
/// configured (or default) registry. Shared by `fetch` and `add`.
fn pull_ref(config: &RootConfig, reference: &ProtocolRef) -> Result<oci::PulledArtifact> {
    let registry_url = config.registry_url();
    let oci_reference = oci::reference_for(&registry_url, reference)?;
    let client = oci::client_for(&registry_url);
    futures::executor::block_on(oci::pull(&client, &oci_reference))
}

/// Re-download and overwrite the cache for one already-pinned dep.
pub fn fetch(entry: &DependencyEntry, config: &RootConfig) -> Result<()> {
    let pulled = pull_ref(config, &entry.reference)?;

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

fn write_cache(
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

/// A request to add a dependency, mapped straight from the CLI.
pub struct AddRequest {
    /// As provided by the user; version is optional (defaults to `latest`).
    pub reference: ProtocolRef,
    /// `None` → derived from the reference's name.
    pub alias: Option<String>,
    /// Replace an existing entry with the same alias.
    pub force: bool,
    /// Pull + cache but do not modify `trix.toml`.
    pub dry_run: bool,
}

/// Everything the command/view layer needs to report a completed `add`.
pub struct AddOutcome {
    pub alias: String,
    pub reference: ProtocolRef,
    pub digest: String,
    pub cache_root: PathBuf,
    pub transactions: Vec<String>,
    pub replaced: bool,
}

/// Pull a protocol, pin it to a concrete version, cache it, and (unless
/// `dry_run`) record it in `trix.toml`. This is the whole `trix use`
/// operation; the command layer only maps CLI args in and a view out.
pub fn add(config: &RootConfig, req: AddRequest) -> Result<AddOutcome> {
    let request_ref = default_to_latest(req.reference);
    let pulled = pull_ref(config, &request_ref)?;
    let pinned_ref = pin_reference(&request_ref, &pulled.metadata)?;

    let alias = req
        .alias
        .unwrap_or_else(|| pinned_ref.short_name().to_string());

    let replaced = config.dependencies.contains_key(&alias);
    if replaced && !req.force {
        return Err(miette::miette!(
            "alias '{}' already exists. Pass --force to replace, or --alias <name> to use a different one.",
            alias
        ));
    }

    let entry = DependencyEntry {
        alias: alias.clone(),
        reference: pinned_ref.clone(),
        digest: pulled.digest.clone(),
    };

    // Validate the prospective config so a bad alias/ref is rejected with the
    // same diagnostics as on-load validation, before we touch disk.
    let mut next_config = config.clone();
    next_config.dependencies.insert(alias.clone(), entry.clone());
    next_config.validate_dependencies()?;

    let paths = cache_paths(&entry)?;
    write_cache(&paths, &pulled, &entry)?;

    if !req.dry_run {
        let trix_toml = crate::dirs::protocol_root()?.join("trix.toml");
        next_config.save(&trix_toml)?;
    }

    Ok(AddOutcome {
        alias,
        transactions: discover_transactions(&pulled.tii),
        reference: pinned_ref,
        digest: pulled.digest,
        cache_root: paths.root,
        replaced,
    })
}

fn default_to_latest(reference: ProtocolRef) -> ProtocolRef {
    match reference {
        ProtocolRef::Registry {
            scope,
            name,
            version: None,
        } => ProtocolRef::Registry {
            scope,
            name,
            version: Some("latest".to_string()),
        },
        other => other,
    }
}

/// Pin `request` to a concrete-version `ProtocolRef::Registry`.
fn pin_reference(
    request: &ProtocolRef,
    metadata: &oci::ImageMetadata,
) -> Result<ProtocolRef> {
    let (scope, name) = match request {
        ProtocolRef::Registry { scope, name, .. } => (scope.clone(), name.clone()),
        ProtocolRef::Alias(a) => {
            return Err(miette::miette!(
                "'trix use' requires a registry reference (e.g. acme/widget:0.1.0), got alias '{}'",
                a
            ));
        }
    };
    Ok(ProtocolRef::Registry {
        scope,
        name,
        version: Some(pin_version(request, metadata)?),
    })
}

/// Resolve the concrete version to pin (and to use as the cache directory
/// name). Prefer the publisher-recorded version, then a concrete request
/// tag. We deliberately do NOT fall back to a digest-based pseudo-version:
/// that would leak an opaque `sha256-…` string into trix.toml and the cache
/// layout. A protocol with no concrete version is a publishing error the
/// consumer can't paper over.
fn pin_version(request: &ProtocolRef, metadata: &oci::ImageMetadata) -> Result<String> {
    if let Some(v) = metadata
        .version
        .as_deref()
        .filter(|v| !v.is_empty() && *v != "latest")
    {
        return Ok(v.to_string());
    }
    if let ProtocolRef::Registry {
        version: Some(tag), ..
    } = request
        && tag != "latest"
    {
        return Ok(tag.clone());
    }
    Err(miette::miette!(
        "the published image does not carry a concrete version and was requested by a mutable tag; \
         ask the publisher to `trix publish` a concretely-versioned release, then `trix use <scope>/<name>:<version>`"
    ))
}

fn discover_transactions(tii_bytes: &[u8]) -> Vec<String> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(tii_bytes) else {
        return Vec::new();
    };
    let Some(map) = json.get("transactions").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = map.keys().cloned().collect();
    names.sort();
    names
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
