//! Parsing and normalization of source-repository URLs declared in
//! `[protocol].repository`. The URL anchors a published protocol to a
//! verifiable GitHub identity; downstream verification compares parsed
//! owners against the OIDC claim `repository_owner`.
//!
//! v1 accepts only github.com. Adding GitLab/Codeberg/etc. is a
//! one-line addition to `ALLOWED_HOSTS` plus the corresponding trust
//! chain in the verification layer.

/// Hosts whose OIDC issuer + claim shape `trix` knows how to verify.
const ALLOWED_HOSTS: &[&str] = &["github.com"];

/// A repository URL that's been normalized and validated against the
/// host allowlist. Decomposed into (host, owner, repo); the canonical
/// `https://` form is reconstructed on demand by [`RepositoryUrl::url`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUrl {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl RepositoryUrl {
    /// Parse the user-supplied `[protocol].repository` value. Accepts the
    /// shapes people actually paste:
    ///   * `https://github.com/owner/repo`
    ///   * `https://github.com/owner/repo.git`
    ///   * `https://github.com/owner/repo/`
    ///   * `git@github.com:owner/repo.git`
    ///   * `git+https://github.com/owner/repo` (cargo-style)
    ///
    /// Host must be in [`ALLOWED_HOSTS`]. v1 requires exactly two path
    /// segments — nested GitLab groups are deferred along with GitLab
    /// trust-chain support.
    pub fn parse(input: &str) -> miette::Result<Self> {
        let raw = input.trim();
        let stripped = raw.strip_prefix("git+").unwrap_or(raw);

        // Rewrite SCP-style SSH (`git@host:owner/repo`) into a real URL
        // so the `url` crate can parse it. RFC 3986 reads the `:` after
        // the user as a port separator, so the SCP form isn't valid on
        // its own.
        let normalized: std::borrow::Cow<'_, str> = if let Some(rest) =
            stripped.strip_prefix("git@")
        {
            let (host, path) = rest.split_once(':').ok_or_else(|| {
                miette::miette!("repository SSH form must be 'git@host:owner/repo', got '{raw}'")
            })?;
            format!("ssh://git@{host}/{path}").into()
        } else {
            stripped.into()
        };

        let url = url::Url::parse(&normalized).map_err(|e| {
            miette::miette!(
                "repository is not a valid URL ('{raw}'): {e}; expected something like 'https://github.com/owner/repo'"
            )
        })?;

        let host = url
            .host_str()
            .ok_or_else(|| miette::miette!("repository URL has no host ('{raw}')"))?;
        if !ALLOWED_HOSTS.contains(&host) {
            return Err(miette::miette!(
                "unsupported repository host '{host}' in '{raw}'; supported: {}",
                ALLOWED_HOSTS.join(", ")
            ));
        }

        let path = url.path().trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);

        let mut segments = path.split('/').filter(|seg| !seg.is_empty());
        let owner = segments
            .next()
            .ok_or_else(|| miette::miette!("repository missing owner segment in '{raw}'"))?;
        let repo = segments
            .next()
            .ok_or_else(|| miette::miette!("repository missing repo segment in '{raw}'"))?;
        if segments.next().is_some() {
            return Err(miette::miette!(
                "repository must have exactly two path segments (owner/repo), got extra in '{raw}'"
            ));
        }

        Ok(RepositoryUrl {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    /// Canonical `https://host/owner/repo` form. Written to
    /// `ImageMetadata.repository_url` and `org.opencontainers.image.source`.
    pub fn url(&self) -> String {
        format!("https://{}/{}/{}", self.host, self.owner, self.repo)
    }

    /// Short `owner/repo` handle. Written to the
    /// `land.tx3.protocol.repository` annotation and compared against
    /// the OIDC `repository` claim.
    pub fn short(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(host: &str, owner: &str, repo: &str) -> RepositoryUrl {
        RepositoryUrl {
            host: host.into(),
            owner: owner.into(),
            repo: repo.into(),
        }
    }

    #[test]
    fn canonical_https_form() {
        assert_eq!(
            RepositoryUrl::parse("https://github.com/acme/widget").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn strips_dot_git_suffix() {
        assert_eq!(
            RepositoryUrl::parse("https://github.com/acme/widget.git").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn strips_trailing_slash() {
        assert_eq!(
            RepositoryUrl::parse("https://github.com/acme/widget/").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn normalizes_ssh_form() {
        assert_eq!(
            RepositoryUrl::parse("git@github.com:acme/widget.git").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn accepts_cargo_style_git_plus_prefix() {
        assert_eq!(
            RepositoryUrl::parse("git+https://github.com/acme/widget").unwrap(),
            parsed("github.com", "acme", "widget")
        );
    }

    #[test]
    fn rejects_unknown_host() {
        let err = RepositoryUrl::parse("https://gitlab.com/acme/widget").unwrap_err();
        assert!(format!("{err:?}").contains("unsupported repository host"));
    }

    #[test]
    fn rejects_extra_path_segments() {
        let err = RepositoryUrl::parse("https://github.com/acme/widget/tree/main").unwrap_err();
        assert!(format!("{err:?}").contains("exactly two path segments"));
    }

    #[test]
    fn rejects_missing_repo() {
        let err = RepositoryUrl::parse("https://github.com/acme").unwrap_err();
        assert!(format!("{err:?}").contains("missing repo segment"));
    }

    #[test]
    fn rejects_bare_owner_repo_shorthand() {
        let err = RepositoryUrl::parse("acme/widget").unwrap_err();
        assert!(format!("{err:?}").contains("not a valid URL"));
    }

    #[test]
    fn url_round_trips() {
        let r = RepositoryUrl::parse("https://github.com/acme/widget.git").unwrap();
        assert_eq!(r.url(), "https://github.com/acme/widget");
    }

    #[test]
    fn short_form() {
        let r = RepositoryUrl::parse("https://github.com/acme/widget").unwrap();
        assert_eq!(r.short(), "acme/widget");
    }
}
