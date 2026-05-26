//! Verifier for publisher attestations attached to a pulled OCI artifact.
//!
//! Two tiers — both stubbed today, ready to be filled in when the
//! registry-side OIDC + sigstore work lands. The surrounding wiring
//! (cache fields, trust-pin enforcement, CLI flags) is already in
//! place, so swapping the stubs for real verification touches only the
//! bodies of these functions and the constants in
//! [`crate::config::convention`].
//!
//! - [`verify_sigstore_bundle`] — OIDC tier. Verifies a sigstore bundle
//!   (`application/vnd.dev.sigstore.bundle.v0.3+json`, attached as an
//!   OCI 1.1 referrer) against the pinned Fulcio root, checking that
//!   the cert's `repository` / `repository_owner` claims match.
//! - [`verify_registry_attestation`] — App tier. Verifies a
//!   registry-signed attestation against
//!   [`crate::config::convention::TX3_REGISTRY_SIGNING_KEY`].
//!
//! Both return a [`VerificationFacts`] that the caller drops into the
//! cached [`crate::interfaces::ProtocolManifest`].

use miette::Result;

use super::VerificationTier;

/// What a successful verification yields. Mirrors the verification
/// fields on `ProtocolManifest`; the caller copies them across rather
/// than reaching into the manifest from inside the verifier.
#[derive(Debug, Clone)]
pub struct VerificationFacts {
    pub tier: VerificationTier,
    /// OIDC subject (workflow identity) for the OIDC tier; GitHub login
    /// for the App tier.
    pub subject: Option<String>,
    /// Repository the publisher attested to (short `owner/repo`).
    pub repository: Option<String>,
    /// Git ref the publish was triggered from. OIDC-tier only —
    /// registry attestations carry no `ref` claim.
    pub git_ref: Option<String>,
    /// Source commit SHA — OIDC tier reads it from the workflow claim;
    /// App tier inherits it from `ImageMetadata.commit_sha`.
    pub commit_sha: Option<String>,
    pub fulcio_issuer: Option<String>,
    /// Digest of the bundle / attestation that produced this result.
    pub bundle_digest: Option<String>,
}

/// Verify a sigstore bundle attached as an OCI referrer to an
/// OIDC-tier publish.
///
/// Today: a stub. The call sites are wired (`trix use --require=oidc`,
/// `verify_cached`'s trust-pin enforcement) so that landing the real
/// implementation does not require touching the consumer surface.
///
/// When this becomes real, it will:
/// 1. Parse the bundle JSON.
/// 2. Walk the certificate chain back to
///    [`crate::config::convention::FULCIO_ROOT_CERT_PEM`].
/// 3. Check that the cert's OIDC issuer matches
///    [`crate::config::convention::GITHUB_OIDC_ISSUER`].
/// 4. Read the `repository` / `repository_owner` claims off the cert
///    extensions and require them to equal `expected_owner` /
///    `expected_repo`.
/// 5. Return a [`VerificationFacts`] with `tier = GithubOidc`.
pub fn verify_sigstore_bundle(
    _bundle: &[u8],
    _expected_owner: &str,
    _expected_repo: &str,
) -> Result<VerificationFacts> {
    Err(miette::miette!(
        "sigstore verification is not yet wired — the registry has not shipped its OIDC + \
         referrer-API support. See design/003-protocol-interfaces.md → 'Identity & trust → \
         Deferred to follow-up'."
    ))
}

/// Verify a registry-signed attestation for an App-tier publish.
///
/// Today: a stub. Real implementation will verify an Ed25519 signature
/// against [`crate::config::convention::TX3_REGISTRY_SIGNING_KEY`] and
/// surface the recorded GitHub login as `subject`.
pub fn verify_registry_attestation(
    _attestation: &[u8],
    _expected_owner: &str,
) -> Result<VerificationFacts> {
    Err(miette::miette!(
        "App-tier (registry-attested) verification is not yet wired — the registry has not \
         published its signing key. See design/003-protocol-interfaces.md → 'Identity & trust \
         → Deferred to follow-up'."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigstore_verifier_is_a_stub() {
        let err = verify_sigstore_bundle(&[], "acme", "acme/widget").unwrap_err();
        assert!(format!("{err:?}").contains("not yet wired"));
    }

    #[test]
    fn registry_attestation_verifier_is_a_stub() {
        let err = verify_registry_attestation(&[], "acme").unwrap_err();
        assert!(format!("{err:?}").contains("not yet wired"));
    }
}
