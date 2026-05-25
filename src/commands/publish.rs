use crate::config::RootConfig;
use crate::oci::{
    self, ImageMetadata, MARKDOWN_MEDIA_TYPE, PROTOCOL_MEDIA_TYPE, TII_MEDIA_TYPE,
};
use crate::refs::ProtocolRef;
use clap::Args as ClapArgs;
use miette::IntoDiagnostic as _;

#[derive(ClapArgs)]
/// Arguments for the publish command (UNSTABLE - experimental feature)
pub struct Args {}

#[allow(dead_code)]
fn get_image_url(config: &RootConfig) -> String {
    let registry_url = config.registry_url();
    format!(
        "{}/image/{}%2F{}/tag/{}",
        registry_url,
        config.protocol.scope.clone().unwrap(),
        config.protocol.name.clone(),
        config.protocol.version.clone()
    )
}

#[allow(unused_variables)]
pub fn run(_args: Args, config: &RootConfig) -> miette::Result<()> {
    #[cfg(feature = "unstable")]
    {
        _run(_args, config)
    }
    #[cfg(not(feature = "unstable"))]
    {
        let _ = config;
        Err(miette::miette!(
            "The publish command is currently unstable and requires the `unstable` feature to be enabled."
        ))
    }
}

/// Hosts whose OIDC issuer + claim shape `trix` knows how to verify.
/// v1 is GitHub-only; add a match arm here (and the corresponding trust
/// chain) when extending to GitLab, Codeberg, etc.
const ALLOWED_REPOSITORY_HOSTS: &[&str] = &["github.com"];

#[derive(Debug, PartialEq, Eq)]
struct ParsedRepositoryUrl {
    host: String,
    owner: String,
    repo: String,
    /// Normalized `https://host/owner/repo` form, written to
    /// `ImageMetadata.repository_url` and `org.opencontainers.image.source`.
    canonical_url: String,
}

/// Parse the user-supplied `[protocol].repository` value. Accepts the
/// shapes people actually paste:
///   * `https://github.com/owner/repo`
///   * `https://github.com/owner/repo.git`
///   * `https://github.com/owner/repo/`
///   * `git@github.com:owner/repo.git`
///   * `git+https://github.com/owner/repo` (cargo-style)
///
/// Returns a normalized canonical URL plus the extracted (host, owner, repo)
/// triple. Host must be in `ALLOWED_REPOSITORY_HOSTS`. v1 requires exactly
/// two path segments — nested GitLab groups are deferred along with GitLab
/// trust-chain support.
fn parse_repository_url(input: &str) -> miette::Result<ParsedRepositoryUrl> {
    let raw = input.trim();
    let stripped = raw.strip_prefix("git+").unwrap_or(raw);

    // Rewrite SCP-style SSH (`git@host:owner/repo`) into a real URL so the
    // `url` crate can parse it. RFC 3986 reads the `:` after the user as a
    // port separator, so the SCP form isn't a valid URL on its own.
    let normalized: std::borrow::Cow<'_, str> = if let Some(rest) = stripped.strip_prefix("git@") {
        let (host, path) = rest.split_once(':').ok_or_else(|| {
            miette::miette!(
                "`[protocol].repository` SSH form must be 'git@host:owner/repo', got '{raw}'"
            )
        })?;
        format!("ssh://git@{host}/{path}").into()
    } else {
        stripped.into()
    };

    let url = url::Url::parse(&normalized).map_err(|e| {
        miette::miette!(
            "`[protocol].repository` is not a valid URL ('{raw}'): {e}; expected something like 'https://github.com/owner/repo'"
        )
    })?;

    let host = url.host_str().ok_or_else(|| {
        miette::miette!("`[protocol].repository` URL has no host ('{raw}')")
    })?;
    if !ALLOWED_REPOSITORY_HOSTS.contains(&host) {
        return Err(miette::miette!(
            "unsupported repository host '{host}' in '{raw}'; supported: {}",
            ALLOWED_REPOSITORY_HOSTS.join(", ")
        ));
    }

    let path = url.path().trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let mut segments = path.split('/').filter(|seg| !seg.is_empty());
    let owner = segments.next().ok_or_else(|| {
        miette::miette!("`[protocol].repository` missing owner segment in '{raw}'")
    })?;
    let repo = segments.next().ok_or_else(|| {
        miette::miette!("`[protocol].repository` missing repo segment in '{raw}'")
    })?;
    if segments.next().is_some() {
        return Err(miette::miette!(
            "`[protocol].repository` must have exactly two path segments (owner/repo), got extra in '{raw}'"
        ));
    }

    Ok(ParsedRepositoryUrl {
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        canonical_url: format!("https://{host}/{owner}/{repo}"),
    })
}

/// Best-effort capture of the publishing working tree's HEAD commit. Used
/// for the `org.opencontainers.image.revision` annotation and the
/// `ImageMetadata.commit_sha` field. Returns `None` if the directory isn't
/// a git repo or `git` isn't on PATH — the publish still succeeds.
fn capture_commit_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[allow(dead_code)]
pub fn _run(_args: Args, config: &RootConfig) -> miette::Result<()> {
    let Some(scope) = config.protocol.scope.clone() else {
        return Err(miette::miette!("No scope found in trix.toml"));
    };

    // GitHub-anchored identity: a published protocol MUST declare the repo
    // that owns it as a URL, and that repo's owner segment MUST match
    // `scope`. The registry will independently verify the caller has push
    // to the repo; this preflight just catches typos before we push.
    let Some(repository) = config.protocol.repository.clone() else {
        return Err(miette::miette!(
            "`[protocol].repository` is required to publish — set it to a repository URL (e.g. 'https://github.com/{scope}/{}')",
            config.protocol.name
        ));
    };
    let parsed = parse_repository_url(&repository)?;
    if parsed.owner != scope {
        return Err(miette::miette!(
            "`[protocol].repository` owner '{}' does not match `[protocol].scope` '{scope}'",
            parsed.owner
        ));
    }
    // Short `owner/repo` handle for the tx3-specific annotation and any
    // string-comparison checks against OIDC claims downstream.
    let repository_short = format!("{}/{}", parsed.owner, parsed.repo);
    let repository_url = parsed.canonical_url.clone();

    let name = config.protocol.name.clone();
    let version = config.protocol.version.clone();
    let published_date = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let commit_sha = capture_commit_sha();

    let protocol = std::fs::read_to_string(config.protocol.main.clone()).into_diagnostic()?;

    let tii_path = crate::builder::build_tii(config)?;
    let tii_content = std::fs::read_to_string(&tii_path).into_diagnostic()?;

    let mut image_layers = vec![oci_client::client::ImageLayer::new(
        protocol.as_bytes().to_vec(),
        PROTOCOL_MEDIA_TYPE.to_string(),
        None,
    )];

    image_layers.push(oci_client::client::ImageLayer::new(
        tii_content.as_bytes().to_vec(),
        TII_MEDIA_TYPE.to_string(),
        None,
    ));

    if config.protocol.readme.is_some() {
        let readme =
            std::fs::read_to_string(config.protocol.readme.clone().unwrap()).into_diagnostic()?;
        image_layers.push(oci_client::client::ImageLayer::new(
            readme.as_bytes().to_vec(),
            MARKDOWN_MEDIA_TYPE.to_string(),
            None,
        ));
    }

    let image_config = oci_client::client::Config {
        data: serde_json::to_vec(&ImageMetadata {
            name: name.clone(),
            scope: scope.clone(),
            published_date,
            repository_url: Some(repository_url.clone()),
            description: config.protocol.description.clone(),
            version: Some(version.clone()),
            repository: Some(repository_short.clone()),
            commit_sha: commit_sha.clone(),
        })
        .into_diagnostic()?,
        media_type: oci_client::manifest::IMAGE_CONFIG_MEDIA_TYPE.to_string(),
        annotations: None,
    };

    let mut annotations = std::collections::BTreeMap::from([
        (
            "org.opencontainers.image.created".to_string(),
            chrono::DateTime::from_timestamp(published_date, 0)
                .unwrap()
                .to_rfc3339(),
        ),
        ("org.opencontainers.image.vendor".to_string(), scope.clone()),
        ("org.opencontainers.image.title".to_string(), name.clone()),
        (
            "org.opencontainers.image.version".to_string(),
            version.clone(),
        ),
        (
            "org.opencontainers.image.description".to_string(),
            config.protocol.description.clone().unwrap_or_default(),
        ),
        (
            "org.opencontainers.image.source".to_string(),
            repository_url.clone(),
        ),
        (
            "land.tx3.protocol.repository".to_string(),
            repository_short.clone(),
        ),
    ]);
    if let Some(sha) = &commit_sha {
        annotations.insert(
            "org.opencontainers.image.revision".to_string(),
            sha.clone(),
        );
    }

    let image_manifest = oci_client::manifest::OciImageManifest::build(
        &image_layers,
        &image_config,
        Some(annotations),
    );

    let registry_url = config.registry_url();

    let protocol_ref = ProtocolRef::Registry {
        scope: scope.clone(),
        name: name.clone(),
        version: Some(version.clone()),
    };
    let image_reference = oci::reference_for(&registry_url, &protocol_ref)?;
    let oci_client = oci::client_for(&registry_url);

    let digest = futures::executor::block_on(oci_client.push(
        &image_reference,
        &image_layers,
        image_config,
        &oci_client::secrets::RegistryAuth::Anonymous,
        Some(image_manifest),
    ))
    .into_diagnostic()?;

    println!("Image pushed successfully!");
    println!("Image URL: {}", get_image_url(config));
    println!("Manifest URL: {}", digest.manifest_url);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(host: &str, owner: &str, repo: &str) -> ParsedRepositoryUrl {
        ParsedRepositoryUrl {
            host: host.into(),
            owner: owner.into(),
            repo: repo.into(),
            canonical_url: format!("https://{host}/{owner}/{repo}"),
        }
    }

    #[test]
    fn canonical_https_form() {
        assert_eq!(
            parse_repository_url("https://github.com/acme/widget").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn strips_dot_git_suffix() {
        assert_eq!(
            parse_repository_url("https://github.com/acme/widget.git").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn strips_trailing_slash() {
        assert_eq!(
            parse_repository_url("https://github.com/acme/widget/").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn normalizes_ssh_form() {
        assert_eq!(
            parse_repository_url("git@github.com:acme/widget.git").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn accepts_cargo_style_git_plus_prefix() {
        assert_eq!(
            parse_repository_url("git+https://github.com/acme/widget").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn rejects_unknown_host() {
        let err = parse_repository_url("https://gitlab.com/acme/widget").unwrap_err();
        assert!(format!("{err:?}").contains("unsupported repository host"));
    }

    #[test]
    fn rejects_extra_path_segments() {
        let err =
            parse_repository_url("https://github.com/acme/widget/tree/main").unwrap_err();
        assert!(format!("{err:?}").contains("exactly two path segments"));
    }

    #[test]
    fn rejects_missing_repo() {
        let err = parse_repository_url("https://github.com/acme").unwrap_err();
        assert!(format!("{err:?}").contains("missing repo segment"));
    }

    #[test]
    fn rejects_bare_owner_repo_shorthand() {
        let err = parse_repository_url("acme/widget").unwrap_err();
        assert!(format!("{err:?}").contains("not a valid URL"));
    }
}
