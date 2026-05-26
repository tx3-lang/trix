//! Local cache for external protocol **interfaces** pulled from an OCI
//! registry.
//!
//! An interface is an *orthogonal* link, not a build dependency: the
//! project's own protocol compiles in complete isolation and never ingests
//! one. Interfaces exist purely so the *consuming* commands (`invoke`,
//! `codegen`, `inspect tir`) can interact with already-published protocols.
//!
//! The **normative** artifact is the published TII (`main.tii`); the cached
//! `main.tx3` and `README.md` are retained only as human-readable
//! references — informative, never compiled or otherwise treated as
//! authoritative.
//!
//! Cache layout (shared with the project's own built TII):
//! `.tx3/tii/<scope>/<name>/<version>/`
//!     ├── main.tx3       (application/tx3 layer — informative)
//!     ├── main.tii       (application/tii+json layer — normative)
//!     ├── README.md      (text/markdown layer, optional — informative)
//!     └── metadata.json  (ProtocolManifest)
//!
//! Each consuming command calls `restore_all` first — a no-op when the cache
//! is consistent with `trix.toml`.

pub mod attestation;
pub mod manifest;
pub mod oci;
pub mod repository;
pub mod resolve;

pub use attestation::{VerificationFacts, verify_registry_attestation, verify_sigstore_bundle};
pub use manifest::{ProtocolManifest, VerificationTier};
pub use oci::{
    ImageMetadata, MARKDOWN_MEDIA_TYPE, PROTOCOL_MEDIA_TYPE, PulledArtifact, TII_MEDIA_TYPE,
};
pub use repository::RepositoryUrl;
pub use resolve::{ResolveError, ResolvedProtocol, Resolver};

use std::path::PathBuf;

use miette::{IntoDiagnostic as _, Result};

use crate::config::{InterfaceEntry, RootConfig};
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

fn registry_parts(entry: &InterfaceEntry) -> Result<(&str, &str, &str)> {
    match &entry.reference {
        ProtocolRef::Registry {
            scope,
            name,
            version: Some(v),
        } => Ok((scope.as_str(), name.as_str(), v.as_str())),
        ProtocolRef::Registry { version: None, .. } => Err(miette::miette!(
            "interface '{}' has no version pinned — run `trix use` to refresh",
            entry.alias
        )),
        ProtocolRef::Alias(a) => Err(miette::miette!(
            "interface '{}' has alias-only ref '{}'; trix.toml requires a registry reference",
            entry.alias,
            a
        )),
    }
}

pub fn cache_paths(entry: &InterfaceEntry) -> Result<CachePaths> {
    let (scope, name, version) = registry_parts(entry)?;
    let root = crate::dirs::tii_dir(scope, name, version)?;
    Ok(CachePaths {
        source: root.join(CACHE_SOURCE_FILE),
        tii: root.join(CACHE_TII_FILE),
        readme: root.join(CACHE_README_FILE),
        manifest: root.join(CACHE_MANIFEST_FILE),
        root,
    })
}

/// Outcome of inspecting an interface's local cache.
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
pub fn verify_cached(entry: &InterfaceEntry) -> Result<CacheStatus> {
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
                "interface '{}' cache has malformed metadata.json: {}",
                entry.alias,
                e
            )));
        }
    };
    if manifest.digest != entry.digest {
        return Ok(CacheStatus::Invalid(miette::miette!(
            "interface '{}' cache digest '{}' does not match trix.toml digest '{}'. \
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
            "interface '{}' cached TII is not valid JSON: {}",
            entry.alias,
            e
        )));
    }

    if let Some(report) = check_trust(entry, &manifest) {
        return Ok(CacheStatus::Invalid(report));
    }

    Ok(CacheStatus::Valid)
}

/// Compare an `InterfaceEntry.trust` pin against the cached manifest's
/// verification facts. Absent pin → no enforcement (TOFU). Present pin
/// with the manifest still at `tier = Unverified` → warn and accept,
/// because the verifier hasn't landed yet (see "Identity & trust" in
/// `design/003-protocol-interfaces.md`); promoting this to a hard error
/// would block every consumer until the registry implements its half.
/// Present pin with a real tier → strict comparison.
fn check_trust(entry: &InterfaceEntry, manifest: &ProtocolManifest) -> Option<miette::Report> {
    let pin = entry.trust.as_ref()?;

    if manifest.tier == VerificationTier::Unverified {
        eprintln!(
            "warning: interface '{}' has trust pin '{}' but the cached artifact is unverified \
             (sigstore / GitHub App verification is not yet wired). Accepting for now — \
             this will become a hard error once the verifier lands.",
            entry.alias, pin
        );
        return None;
    }

    if !manifest.tier.satisfies(pin.tier) {
        return Some(miette::miette!(
            "interface '{}' trust pin expects tier '{}', but the cached artifact was \
             verified at a different tier. Run `trix use --force {}` to repin, or update \
             the trust pin in trix.toml.",
            entry.alias,
            pin,
            entry.reference,
        ));
    }

    if let Some(expected_repo) = pin.repository.as_deref()
        && let Some(actual_repo) = manifest.repository.as_deref()
        && expected_repo != actual_repo
    {
        return Some(miette::miette!(
            "interface '{}' trust pin expects repository '{}', but the cached artifact \
             reports '{}'. Run `trix use --force {}` to repin, or update the trust pin.",
            entry.alias,
            expected_repo,
            actual_repo,
            entry.reference,
        ));
    }

    // `git_ref` narrowing is meaningful only once the sigstore tier records
    // the workflow's `ref` claim. Until then, ignore it silently — the
    // tier-`Unverified` early return already covers the user.
    let _ = pin.git_ref.as_deref();

    None
}

/// Resolve + anonymously pull an OCI artifact for `reference` against the
/// configured (or default) registry. Shared by `fetch` and `add`.
fn pull_ref(config: &RootConfig, reference: &ProtocolRef) -> Result<oci::PulledArtifact> {
    let registry_url = config.registry_url();
    let oci_reference = oci::reference_for(&registry_url, reference)?;
    let client = oci::client_for(&registry_url);
    futures::executor::block_on(oci::pull(&client, &oci_reference))
}

/// Re-download and overwrite the cache for one already-pinned interface.
pub fn fetch(entry: &InterfaceEntry, config: &RootConfig) -> Result<()> {
    let pulled = pull_ref(config, &entry.reference)?;

    if pulled.digest != entry.digest {
        return Err(miette::miette!(
            "interface '{}' registry digest '{}' no longer matches trix.toml digest '{}'. \
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
    entry: &InterfaceEntry,
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
        repository: pulled.metadata.repository.clone(),
        commit_sha: pulled.metadata.commit_sha.clone(),
        // Until the sigstore / App-attestation paths land, every fetched
        // artifact is recorded as Unverified. `verify_cached` reads this
        // when comparing against an `InterfaceEntry.trust` pin.
        tier: VerificationTier::Unverified,
        subject: None,
        fulcio_issuer: None,
        bundle_digest: None,
        verified_at: None,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).into_diagnostic()?;
    std::fs::write(&paths.manifest, manifest_bytes).into_diagnostic()?;
    Ok(())
}

/// Verification policy for a `trix use` invocation. Drives whether
/// publisher attestations are required, optional, or skipped. The
/// non-`Default` variants surface a clear "verification not yet
/// available" error until the sigstore / App-attestation paths land
/// (see `crate::interfaces::attestation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicy {
    /// Today's behavior: pull, cache, record `tier = Unverified`.
    Default,
    /// `--insecure`: explicit acknowledgement that no verification will
    /// be performed even when it becomes available.
    Insecure,
    /// `--require=oidc`: refuse the pull unless an OIDC-tier attestation
    /// verifies. Today's stub verifier always errors.
    RequireOidc,
    /// `--require=app`: refuse unless an App-tier attestation verifies.
    /// Strictly weaker than OIDC; accepted for completeness.
    RequireApp,
}

impl Default for TrustPolicy {
    fn default() -> Self {
        TrustPolicy::Default
    }
}

/// A request to add an interface, mapped straight from the CLI.
pub struct AddRequest {
    /// As provided by the user; version is optional (defaults to `latest`).
    pub reference: ProtocolRef,
    /// `None` → derived from the reference's name.
    pub alias: Option<String>,
    /// Replace an existing entry with the same alias.
    pub force: bool,
    /// Pull + cache but do not modify `trix.toml`.
    pub dry_run: bool,
    /// What level of publisher verification to require.
    pub trust_policy: TrustPolicy,
    /// Allow a publisher-subject change without failing. No-op until the
    /// verifier records previous subjects.
    pub accept_rename: bool,
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

    // Enforce `--require=…`. Both tiers reject with the stub's
    // "not yet wired" error until the registry-side verifier ships.
    // `Insecure` and `Default` skip the verifier entirely — there is
    // nothing to verify against today, so `Default` behaves like
    // `Insecure` in practice; the distinction starts mattering when
    // the verifier becomes real and `Default` flips to "TOFU verify".
    let _ = req.accept_rename; // wired through for forward-compat; consumed by the future verifier
    match req.trust_policy {
        TrustPolicy::RequireOidc => {
            attestation::verify_sigstore_bundle(
                &[],
                pulled.metadata.repository.as_deref().unwrap_or(""),
                pulled.metadata.repository.as_deref().unwrap_or(""),
            )?;
        }
        TrustPolicy::RequireApp => {
            attestation::verify_registry_attestation(
                &[],
                pulled.metadata.repository.as_deref().unwrap_or(""),
            )?;
        }
        TrustPolicy::Insecure | TrustPolicy::Default => {}
    }

    let alias = req
        .alias
        .unwrap_or_else(|| pinned_ref.short_name().to_string());

    let replaced = config.interfaces.contains_key(&alias);
    if replaced && !req.force {
        return Err(miette::miette!(
            "alias '{}' already exists. Pass --force to replace, or --alias <name> to use a different one.",
            alias
        ));
    }

    let entry = InterfaceEntry {
        alias: alias.clone(),
        reference: pinned_ref.clone(),
        digest: pulled.digest.clone(),
        trust: None,
    };

    // Validate the prospective config so a bad alias/ref is rejected with the
    // same diagnostics as on-load validation, before we touch disk.
    let mut next_config = config.clone();
    next_config.interfaces.insert(alias.clone(), entry.clone());
    validate(&next_config)?;

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

/// Validate the `[interfaces]` table against the project. Interface-domain
/// semantics (alias rules, registry-ref pinning, no duplicate `(scope, name)`)
/// live here, not in the config layer — `config` only models the schema.
///
/// Run by the *consuming* commands (`invoke`, `codegen`, `inspect tir`) before
/// they touch an interface, and inside `add` before writing. `build`/`check`
/// are project-only and never call this.
pub fn validate(config: &RootConfig) -> Result<()> {
    use std::collections::HashSet;

    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (alias, entry) in config.interfaces.iter() {
        if alias == &config.protocol.name {
            return Err(miette::miette!(
                "interface alias '{}' conflicts with the project's own protocol name",
                alias
            ));
        }
        crate::refs::validate_ident(alias).map_err(|_| {
            miette::miette!(
                "interface alias '{}' is not a valid identifier (must match [a-zA-Z_][a-zA-Z0-9_.-]*)",
                alias
            )
        })?;
        let (scope, name, version) = match &entry.reference {
            ProtocolRef::Registry {
                scope,
                name,
                version,
            } => (scope, name, version),
            ProtocolRef::Alias(a) => {
                return Err(miette::miette!(
                    "interface '{}' has alias-only ref '{}'; must be a full registry reference (e.g. acme/widget:0.1.0)",
                    alias,
                    a
                ));
            }
        };
        let Some(v) = version else {
            return Err(miette::miette!(
                "interface '{}' has no version pinned in trix.toml; run `trix use {}` to pin",
                alias,
                entry.reference
            ));
        };
        if v == "latest" {
            return Err(miette::miette!(
                "interface '{}' is pinned to 'latest'; trix.toml must reference a concrete version",
                alias
            ));
        }
        if !seen.insert((scope.clone(), name.clone())) {
            return Err(miette::miette!(
                "two interface entries map to the same '{}/{}'; aliases must point to distinct protocols",
                scope,
                name
            ));
        }
    }
    Ok(())
}

/// For every entry in `config.interfaces`, verify the cache. If an interface is
/// merely missing from disk we attempt to re-download it; if the cache is
/// present but inconsistent (digest mismatch, corrupt metadata, malformed
/// TII), we surface the verification error directly so the user knows their
/// cache and lockfile disagree. No-op when `interfaces` is empty.
pub fn restore_all(config: &RootConfig) -> Result<()> {
    if config.interfaces.is_empty() {
        return Ok(());
    }
    for entry in config.interfaces.values() {
        match verify_cached(entry)? {
            CacheStatus::Valid => {}
            CacheStatus::Invalid(report) => return Err(report),
            CacheStatus::Missing => {
                eprintln!("restoring interface '{}'...", entry.alias);
                fetch(entry, config)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PublisherKind, TrustedPublisher};

    fn manifest(tier: VerificationTier, repository: Option<&str>) -> ProtocolManifest {
        ProtocolManifest {
            scope: "acme".into(),
            name: "widget".into(),
            version: "0.1.3".into(),
            digest: "sha256:abc".into(),
            published_date: 0,
            description: None,
            repository_url: None,
            fetched_at: 0,
            has_readme: false,
            repository: repository.map(str::to_string),
            commit_sha: None,
            tier,
            subject: None,
            fulcio_issuer: None,
            bundle_digest: None,
            verified_at: None,
        }
    }

    fn entry(trust: Option<TrustedPublisher>) -> InterfaceEntry {
        InterfaceEntry {
            alias: "widget".into(),
            reference: ProtocolRef::Registry {
                scope: "acme".into(),
                name: "widget".into(),
                version: Some("0.1.3".into()),
            },
            digest: "sha256:abc".into(),
            trust,
        }
    }

    #[test]
    fn no_pin_means_no_enforcement() {
        let m = manifest(VerificationTier::Unverified, None);
        assert!(check_trust(&entry(None), &m).is_none());
    }

    #[test]
    fn pin_against_unverified_warns_but_passes() {
        let m = manifest(VerificationTier::Unverified, None);
        let pin = TrustedPublisher {
            tier: PublisherKind::GithubOidc,
            repository: Some("acme/widget".into()),
            git_ref: None,
        };
        assert!(check_trust(&entry(Some(pin)), &m).is_none());
    }

    #[test]
    fn tier_mismatch_is_rejected() {
        let m = manifest(VerificationTier::GithubApp, Some("acme/widget"));
        let pin = TrustedPublisher {
            tier: PublisherKind::GithubOidc,
            repository: Some("acme/widget".into()),
            git_ref: None,
        };
        let err = check_trust(&entry(Some(pin)), &m).expect("expected mismatch report");
        assert!(format!("{err:?}").contains("tier"));
    }

    #[test]
    fn repo_mismatch_is_rejected() {
        let m = manifest(VerificationTier::GithubOidc, Some("acme/widget"));
        let pin = TrustedPublisher {
            tier: PublisherKind::GithubOidc,
            repository: Some("acme/imposter".into()),
            git_ref: None,
        };
        let err = check_trust(&entry(Some(pin)), &m).expect("expected mismatch report");
        assert!(format!("{err:?}").contains("repository"));
    }

    #[test]
    fn matching_pin_accepts() {
        let m = manifest(VerificationTier::GithubOidc, Some("acme/widget"));
        let pin = TrustedPublisher {
            tier: PublisherKind::GithubOidc,
            repository: Some("acme/widget".into()),
            git_ref: Some("main".into()),
        };
        assert!(check_trust(&entry(Some(pin)), &m).is_none());
    }
}
