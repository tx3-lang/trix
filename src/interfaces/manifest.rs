use serde::{Deserialize, Serialize};

use crate::config::PublisherKind;

/// The verification level reached for a cached interface. `Unverified`
/// covers everything we know how to record today — the artifact was
/// pulled and digest-pinned, but no signature has been checked. The
/// `GithubOidc` and `GithubApp` variants are written only by the
/// (still-deferred) sigstore + registry-attestation paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationTier {
    Unverified,
    GithubOidc,
    GithubApp,
}

impl VerificationTier {
    /// True if the manifest's recorded tier satisfies a `trust` pin that
    /// expects `expected`. `Unverified` never satisfies a real-publisher
    /// pin — the caller decides whether to fail or warn.
    pub fn satisfies(self, expected: PublisherKind) -> bool {
        matches!(
            (self, expected),
            (VerificationTier::GithubOidc, PublisherKind::GithubOidc)
                | (VerificationTier::GithubApp, PublisherKind::GithubApp)
        )
    }
}

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

    // ---- Identity (publisher-supplied, recorded at publish time) ---------
    /// Short `owner/repo` handle, mirroring `ImageMetadata.repository`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Source commit SHA captured at publish time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,

    // ---- Verification (consumer-derived, set at restore/verify time) -----
    /// Tier the cached artifact has been verified at. Defaults to
    /// `Unverified` for old caches and for any publish that pre-dates the
    /// sigstore/App-attestation paths.
    #[serde(default = "default_tier")]
    pub tier: VerificationTier,
    /// OIDC subject (workflow identity) or GitHub login, depending on tier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Fulcio issuer pinned at verification time (OIDC tier only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fulcio_issuer: Option<String>,
    /// Digest of the sigstore bundle or registry attestation that was
    /// consumed to derive the verification result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
    /// Unix seconds when verification was performed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<i64>,
}

fn default_tier() -> VerificationTier {
    VerificationTier::Unverified
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> ProtocolManifest {
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
            repository: None,
            commit_sha: None,
            tier: VerificationTier::Unverified,
            subject: None,
            fulcio_issuer: None,
            bundle_digest: None,
            verified_at: None,
        }
    }

    #[test]
    fn round_trip_minimal() {
        let json = serde_json::to_string(&base()).unwrap();
        let back: ProtocolManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tier, VerificationTier::Unverified);
        assert_eq!(back.repository, None);
    }

    #[test]
    fn pre_identity_cache_deserializes() {
        // A metadata.json from before the identity fields existed must
        // still load — all new fields default sensibly.
        let legacy = r#"{
            "scope": "acme",
            "name": "widget",
            "version": "0.1.3",
            "digest": "sha256:abc",
            "published_date": 0,
            "fetched_at": 0,
            "has_readme": false
        }"#;
        let m: ProtocolManifest = serde_json::from_str(legacy).unwrap();
        assert_eq!(m.tier, VerificationTier::Unverified);
        assert!(m.repository.is_none() && m.commit_sha.is_none());
    }

    #[test]
    fn satisfies_only_on_exact_tier_match() {
        assert!(VerificationTier::GithubOidc.satisfies(PublisherKind::GithubOidc));
        assert!(VerificationTier::GithubApp.satisfies(PublisherKind::GithubApp));
        assert!(!VerificationTier::Unverified.satisfies(PublisherKind::GithubOidc));
        assert!(!VerificationTier::GithubOidc.satisfies(PublisherKind::GithubApp));
    }
}
