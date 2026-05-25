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

/// Split `owner/repo` into its parts. The grammar matches what GitHub
/// accepts in its URL paths; the registry will re-validate against the
/// authenticated identity, this is just a shape check.
fn parse_repository(s: &str) -> miette::Result<(&str, &str)> {
    let (owner, repo) = s.split_once('/').ok_or_else(|| {
        miette::miette!("`[protocol].repository` must be 'owner/repo', got '{s}'")
    })?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return Err(miette::miette!(
            "`[protocol].repository` must be 'owner/repo', got '{s}'"
        ));
    }
    Ok((owner, repo))
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
    // that owns it, and that repo's owner segment MUST match `scope`. The
    // registry will independently verify the caller has push to the repo;
    // this preflight just catches typos before we attempt the push.
    let Some(repository) = config.protocol.repository.clone() else {
        return Err(miette::miette!(
            "`[protocol].repository` is required to publish — set it to 'owner/repo' (e.g. '{scope}/{}')",
            config.protocol.name
        ));
    };
    let (owner, _repo) = parse_repository(&repository)?;
    if owner != scope {
        return Err(miette::miette!(
            "`[protocol].repository` owner '{owner}' does not match `[protocol].scope` '{scope}'"
        ));
    }

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

    let repository_url = format!("https://github.com/{repository}");

    let image_config = oci_client::client::Config {
        data: serde_json::to_vec(&ImageMetadata {
            name: name.clone(),
            scope: scope.clone(),
            published_date,
            repository_url: Some(repository_url.clone()),
            description: config.protocol.description.clone(),
            version: Some(version.clone()),
            repository: Some(repository.clone()),
            license: config.protocol.license.clone(),
            authors: config.protocol.authors.clone(),
            homepage: config.protocol.homepage.clone(),
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
            repository.clone(),
        ),
    ]);
    if let Some(license) = &config.protocol.license {
        annotations.insert(
            "org.opencontainers.image.licenses".to_string(),
            license.clone(),
        );
    }
    if !config.protocol.authors.is_empty() {
        annotations.insert(
            "org.opencontainers.image.authors".to_string(),
            config.protocol.authors.join(", "),
        );
    }
    if let Some(homepage) = &config.protocol.homepage {
        annotations.insert(
            "org.opencontainers.image.url".to_string(),
            homepage.clone(),
        );
    }
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
