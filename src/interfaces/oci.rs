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
pub const LOGO_PNG_MEDIA_TYPE: &str = "image/png";

/// Maximum encoded logo size accepted by `trix publish`. See
/// `design/005-protocol-logos.md`.
pub const LOGO_MAX_BYTES: usize = 256 * 1024;

/// PNG file signature (8 bytes). Validated at publish time so we never
/// attach a layer whose declared media type lies about its bytes.
pub const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// JSON shape of the OCI image config blob written by `trix publish`.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageMetadata {
    pub name: String,
    pub scope: String,
    pub published_date: i64,
    /// OCI-standard creation time (RFC3339), derived from `published_date` via
    /// [`created_timestamp`]. Registries such as zot order a repo's tags by the
    /// config blob's `created` field to pick the "newest" image; without it they
    /// fall back to zero time and the first-pushed tag stays newest forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// GitHub `https://github.com/<owner>/<repo>` URL. Mirrors the
    /// `org.opencontainers.image.source` annotation.
    pub repository_url: Option<String>,
    pub description: Option<String>,
    /// Optional concrete version; `trix use` prefers this over the OCI tag
    /// when pinning, falling back to the tag if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// `owner/repo` from `[protocol].repository` at publish time. Short
    /// handle used for OIDC-claim comparison; the human-readable URL
    /// lives in `repository_url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// Source commit SHA captured at publish time. Best-effort: populated
    /// from `git rev-parse HEAD` in the publishing working tree. Future
    /// OIDC-tier publishes will overwrite this from the workflow claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

/// OCI-standard `created` timestamp (RFC3339) for an image config, derived from
/// a Unix-seconds `published_date`. This is the field registries read to order a
/// repo's tags; see [`ImageMetadata::created`]. Returns `None` only if the
/// timestamp is out of representable range.
pub fn created_timestamp(published_date: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(published_date, 0).map(|dt| dt.to_rfc3339())
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
    // OCI repository paths must be lowercase, but `scope` doubles as the
    // GitHub owner (e.g. "SundaeSwap-finance") which legitimately carries
    // capitals. Lowercase only the path segments here so registry addressing
    // stays OCI-compliant while the original-case scope is preserved for
    // identity/metadata everywhere else. Tags allow uppercase, so leave `tag`.
    oci_client::Reference::try_from(format!(
        "{}/{}/{}:{}",
        host,
        scope.to_lowercase(),
        name.to_lowercase(),
        tag
    ))
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
    let accepted = vec![
        PROTOCOL_MEDIA_TYPE,
        TII_MEDIA_TYPE,
        MARKDOWN_MEDIA_TYPE,
        LOGO_PNG_MEDIA_TYPE,
    ];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_blob_carries_oci_created_for_version_ordering() {
        // Regression guard. Registries like zot pick a repo's "newest" tag from
        // the config blob's OCI `created` field (falling back to `history`, else
        // zero time). With no `created`, every version reads as zero-time and the
        // FIRST-pushed tag stays "newest" forever — so a GraphQL query for the
        // latest version of a protocol never advances past the first publish.
        // The fix: derive an OCI-standard `created` from `published_date` and
        // serialize it into the image config.
        let published_date = 1_782_145_366; // 2026-06-22T16:22:46+00:00

        let created = created_timestamp(published_date);
        assert_eq!(created.as_deref(), Some("2026-06-22T16:22:46+00:00"));

        let meta = ImageMetadata {
            name: "prueba_zot".into(),
            scope: "local".into(),
            published_date,
            created: created.clone(),
            repository_url: None,
            description: None,
            version: Some("0.2.0".into()),
            repository: None,
            commit_sha: None,
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["created"], "2026-06-22T16:22:46+00:00");
    }

    #[test]
    fn config_without_created_still_deserializes() {
        // Artifacts published before this fix have no `created` field; pulling
        // them must keep working with `created` defaulting to None.
        let legacy = r#"{
            "name": "widget",
            "scope": "acme",
            "published_date": 0,
            "version": "0.1.0"
        }"#;
        let meta: ImageMetadata = serde_json::from_str(legacy).unwrap();
        assert_eq!(meta.created, None);
        assert_eq!(meta.version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn reference_lowercases_scope_and_name_for_oci_compliance() {
        // `scope` mirrors the GitHub owner, which may carry capitals (e.g.
        // "SundaeSwap-finance"). OCI repository paths must be lowercase, so the
        // reference is lowercased while identity/metadata keep the original case.
        let protocol_ref = ProtocolRef::Registry {
            scope: "SundaeSwap-finance".to_string(),
            name: "Sundae-V3".to_string(),
            version: Some("0.1.0".to_string()),
        };
        let reference = reference_for("https://registry.tx3.dev", &protocol_ref).unwrap();
        assert_eq!(reference.repository(), "sundaeswap-finance/sundae-v3");
        assert_eq!(reference.tag(), Some("0.1.0"));
    }

    #[test]
    fn reference_defaults_missing_version_to_latest() {
        let protocol_ref = ProtocolRef::Registry {
            scope: "txpipe".to_string(),
            name: "faucet".to_string(),
            version: None,
        };
        let reference = reference_for("https://registry.tx3.dev", &protocol_ref).unwrap();
        assert_eq!(reference.tag(), Some("latest"));
    }
}
