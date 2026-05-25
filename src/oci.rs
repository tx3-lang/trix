//! Shared OCI helpers for pulling and pushing tx3 protocol artifacts.
//!
//! Used by both `trix publish` (push) and `trix use` (pull), plus the
//! interface-cache restore path. All string-level reference parsing happens
//! in `crate::refs` — this module only deals with already-typed references.

use miette::{IntoDiagnostic as _, Result};
use serde::{Deserialize, Serialize};

use crate::refs::ProtocolRef;

pub const PROTOCOL_MEDIA_TYPE: &str = "application/tx3";
pub const TII_MEDIA_TYPE: &str = "application/tii+json";
pub const MARKDOWN_MEDIA_TYPE: &str = "text/markdown";

/// JSON shape of the OCI image config blob written by `trix publish`.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageMetadata {
    pub name: String,
    pub scope: String,
    pub published_date: i64,
    /// GitHub `https://github.com/<owner>/<repo>` URL. Mirrors the
    /// `org.opencontainers.image.source` annotation.
    pub repository_url: Option<String>,
    pub description: Option<String>,
    /// Optional concrete version; `trix use` prefers this over the OCI tag
    /// when pinning, falling back to the tag if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// `owner/repo` from `[protocol].repository` at publish time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Source commit SHA captured at publish time. Best-effort: populated
    /// from `git rev-parse HEAD` in the publishing working tree. Future
    /// OIDC-tier publishes will overwrite this from the workflow claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

pub fn client_for(registry_url: &str) -> oci_client::Client {
    let registry_protocol = registry_url.split("://").next().unwrap_or("https");
    let client_config = oci_client::client::ClientConfig {
        protocol: if registry_protocol == "http" {
            oci_client::client::ClientProtocol::Http
        } else {
            oci_client::client::ClientProtocol::Https
        },
        ..Default::default()
    };
    oci_client::Client::new(client_config)
}

fn registry_host(registry_url: &str) -> &str {
    registry_url
        .split_once("://")
        .map(|(_, host)| host)
        .unwrap_or(registry_url)
}

/// Build an OCI Reference for a registry-style protocol ref.
/// Missing versions become the tag `"latest"` so the OCI client can resolve.
pub fn reference_for(
    registry_url: &str,
    protocol_ref: &ProtocolRef,
) -> Result<oci_client::Reference> {
    let (scope, name, version) = match protocol_ref {
        ProtocolRef::Registry {
            scope,
            name,
            version,
        } => (scope.as_str(), name.as_str(), version.as_deref()),
        ProtocolRef::Alias(a) => {
            return Err(miette::miette!(
                "cannot build an OCI reference from alias-only protocol '{}'",
                a
            ));
        }
    };
    let tag = version.unwrap_or("latest");
    let host = registry_host(registry_url);
    oci_client::Reference::try_from(format!("{}/{}/{}:{}", host, scope, name, tag))
        .into_diagnostic()
}

/// Result of a successful pull. Bytes are owned so the caller can write them
/// to the cache without holding the OCI client open.
pub struct PulledArtifact {
    pub source: Vec<u8>,
    pub tii: Vec<u8>,
    pub readme: Option<Vec<u8>>,
    pub metadata: ImageMetadata,
    /// Manifest digest as returned by the registry. Stored in trix.toml as
    /// the lockfile-style hash.
    pub digest: String,
}

/// Anonymous pull of a tx3 protocol image. Validates that the required
/// layers are present and returns owned bytes.
pub async fn pull(
    client: &oci_client::Client,
    reference: &oci_client::Reference,
) -> Result<PulledArtifact> {
    let accepted = vec![PROTOCOL_MEDIA_TYPE, TII_MEDIA_TYPE, MARKDOWN_MEDIA_TYPE];
    let image = client
        .pull(
            reference,
            &oci_client::secrets::RegistryAuth::Anonymous,
            accepted,
        )
        .await
        .into_diagnostic()?;

    let mut source: Option<Vec<u8>> = None;
    let mut tii: Option<Vec<u8>> = None;
    let mut readme: Option<Vec<u8>> = None;
    // Move the layer bytes out rather than cloning — layers can be large and
    // `image` is owned here.
    for mut layer in image.layers {
        match layer.media_type.as_str() {
            PROTOCOL_MEDIA_TYPE => source = Some(std::mem::take(&mut layer.data)),
            TII_MEDIA_TYPE => tii = Some(std::mem::take(&mut layer.data)),
            MARKDOWN_MEDIA_TYPE => readme = Some(std::mem::take(&mut layer.data)),
            _ => {}
        }
    }
    let Some(source) = source else {
        return Err(miette::miette!(
            "published artifact is malformed — missing application/tx3 layer"
        ));
    };
    let Some(tii) = tii else {
        return Err(miette::miette!(
            "published artifact is malformed — missing application/tii+json layer"
        ));
    };

    let metadata: ImageMetadata = serde_json::from_slice(&image.config.data)
        .into_diagnostic()
        .map_err(|e| miette::miette!("malformed OCI image config: {e}"))?;

    let digest = image
        .digest
        .ok_or_else(|| miette::miette!("OCI registry did not return a manifest digest"))?;

    Ok(PulledArtifact {
        source,
        tii,
        readme,
        metadata,
        digest,
    })
}
