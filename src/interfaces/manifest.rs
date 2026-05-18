use serde::{Deserialize, Serialize};

/// Cache-side metadata.json, written into
/// `.tx3/protocols/<scope>/<name>/<version>/metadata.json` by `trix use`.
/// Superset of the OCI image config: also captures the resolved digest and
/// when we fetched it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolManifest {
    pub scope: String,
    pub name: String,
    pub version: String,
    pub digest: String,
    pub published_date: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_url: Option<String>,
    pub fetched_at: i64,
    pub has_readme: bool,
}
