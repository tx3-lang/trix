use clap::{Args as ClapArgs, ValueEnum};

use crate::config::{ProfileConfig, RootConfig};
use crate::interfaces::{self, AddRequest, TrustPolicy};
use crate::refs::ProtocolRef;

mod view;

/// The verification tier a `trix use` invocation requires. Today only
/// `oidc` is meaningful as a *requirement* (the App tier is a strictly
/// weaker assertion); both reject with "verification not yet available"
/// until the sigstore / App-attestation paths land.
#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum RequiredTier {
    Oidc,
    App,
}

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Reference to pull, e.g. `acme/widget:0.1.0`. The version is optional
    /// and defaults to the `latest` tag; the resolved concrete version is
    /// pinned in trix.toml.
    #[arg(value_name = "REFERENCE", value_parser = ProtocolRef::parse_registry)]
    pub reference: ProtocolRef,

    /// Local alias for this interface. Defaults to the reference's name.
    #[arg(long)]
    pub alias: Option<String>,

    /// Replace an existing entry with the same alias.
    #[arg(long)]
    pub force: bool,

    /// Resolve and cache the artifact but do not modify trix.toml.
    #[arg(long)]
    pub dry_run: bool,

    /// Pin without checking publisher attestations. Use only when the
    /// publisher has not yet adopted OIDC publishing and you have other
    /// out-of-band confidence in the artifact. Conflicts with `--require`.
    #[arg(long)]
    pub insecure: bool,

    /// Require a specific publisher tier. `oidc` is the canonical choice;
    /// `app` is accepted for completeness but is a weaker assertion. Until
    /// the registry-side verifier ships, any explicit requirement fails
    /// with a clear "verification not yet available" error.
    #[arg(long, value_name = "TIER")]
    pub require: Option<RequiredTier>,

    /// Accept a publisher-subject change (e.g. a GitHub repo or org
    /// rename) without failing the verification step. No-op until the
    /// verifier records previous subjects to compare against.
    #[arg(long)]
    pub accept_rename: bool,

    /// Refuse to auto-bootstrap a consumer project when no `trix.toml` is
    /// found. Use in CI or scripted setups where a missing project
    /// indicates a configuration mistake rather than an empty workspace.
    #[arg(long)]
    pub no_init: bool,
}

pub fn run(
    args: Args,
    config: &RootConfig,
    _config_path: &std::path::Path,
    _profile: &ProfileConfig,
) -> miette::Result<()> {
    let dry_run = args.dry_run;

    if args.insecure && args.require.is_some() {
        return Err(miette::miette!(
            "--insecure cannot be combined with --require — they ask for opposite things",
        ));
    }

    let trust_policy = match (args.insecure, args.require) {
        (true, _) => TrustPolicy::Insecure,
        (false, Some(RequiredTier::Oidc)) => TrustPolicy::RequireOidc,
        (false, Some(RequiredTier::App)) => TrustPolicy::RequireApp,
        (false, None) => TrustPolicy::Default,
    };

    let outcome = interfaces::add(
        config,
        AddRequest {
            reference: args.reference,
            alias: args.alias,
            force: args.force,
            dry_run,
            trust_policy,
            accept_rename: args.accept_rename,
        },
    )?;

    view::render(&view::UseView {
        alias: outcome.alias,
        reference: outcome.reference.to_string(),
        digest: outcome.digest,
        cache_path: outcome.cache_root.display().to_string(),
        transactions: outcome.transactions,
        replaced: outcome.replaced,
        dry_run,
    });

    Ok(())
}
