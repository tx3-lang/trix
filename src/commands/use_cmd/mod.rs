use clap::Args as ClapArgs;

use crate::config::{DependencyEntry, ProfileConfig, RootConfig};
use crate::dependencies;
use crate::refs::ProtocolRef;

mod view;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Reference to pull, e.g. `acme/widget:0.1.0`. The version is optional
    /// and defaults to the `latest` tag; the resolved concrete version is
    /// pinned in trix.toml.
    #[arg(value_name = "REFERENCE", value_parser = ProtocolRef::parse_registry)]
    pub reference: ProtocolRef,

    /// Local alias for this dependency. Defaults to the reference's name.
    #[arg(long)]
    pub alias: Option<String>,

    /// Replace an existing entry with the same alias.
    #[arg(long)]
    pub force: bool,

    /// Resolve and cache the artifact but do not modify trix.toml.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(
    args: Args,
    config: &RootConfig,
    _profile: &ProfileConfig,
) -> miette::Result<()> {
    let registry_url = config.registry_url();

    // Default missing version to "latest" so the OCI client can resolve a tag.
    let request_ref = match args.reference.clone() {
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
    };

    let oci_reference = crate::oci::reference_for(&registry_url, &request_ref)?;
    let client = crate::oci::client_for(&registry_url);

    let pulled =
        futures::executor::block_on(crate::oci::pull(&client, &oci_reference))?;

    // Pin the concrete version. Prefer the publisher-recorded version from
    // the image config; fall back to the OCI tag if it was concrete; if both
    // are missing/`latest`, fall back to a short digest with a warning.
    let pinned_version = pin_version(&request_ref, &pulled.metadata, &pulled.digest);
    let pinned_ref = match request_ref {
        ProtocolRef::Registry { scope, name, .. } => ProtocolRef::Registry {
            scope,
            name,
            version: Some(pinned_version.clone()),
        },
        _ => unreachable!("parse_registry rejects aliases"),
    };

    let alias = args.alias.clone().unwrap_or_else(|| {
        pinned_ref
            .short_name()
            .to_string()
    });

    let scope_name = scope_and_name(&pinned_ref);

    // Trial-validate against a hypothetical config to surface alias-conflict
    // errors with the same diagnostics that load-time validation uses.
    let mut next_config = config.clone();
    let existing = next_config.dependencies.contains_key(&alias);
    if existing && !args.force {
        return Err(miette::miette!(
            "alias '{}' already exists. Pass --force to replace, or --alias <name> to use a different one.",
            alias
        ));
    }
    next_config.dependencies.insert(
        alias.clone(),
        DependencyEntry {
            alias: alias.clone(),
            reference: pinned_ref.clone(),
            digest: pulled.digest.clone(),
        },
    );
    next_config.validate_dependencies()?;

    let entry = next_config.dependencies.get(&alias).unwrap().clone();
    let paths = dependencies::cache_paths(&entry)?;
    dependencies::write_cache(&paths, &pulled, &entry)?;

    if !args.dry_run {
        let trix_toml = crate::dirs::protocol_root()?.join("trix.toml");
        next_config.save(&trix_toml)?;
    }

    let transactions = discover_transactions(&pulled.tii);

    view::render(&view::UseView {
        alias,
        reference: pinned_ref.to_string(),
        digest: pulled.digest,
        cache_path: paths.root.display().to_string(),
        transactions,
        replaced: existing,
        dry_run: args.dry_run,
    });

    let _ = scope_name; // reserved for future telemetry / structured logging
    Ok(())
}

fn pin_version(
    request: &ProtocolRef,
    metadata: &crate::oci::ImageMetadata,
    digest: &str,
) -> String {
    if let Some(v) = metadata.version.as_deref() {
        if !v.is_empty() && v != "latest" {
            return v.to_string();
        }
    }
    if let ProtocolRef::Registry {
        version: Some(tag), ..
    } = request
    {
        if tag != "latest" {
            return tag.clone();
        }
    }
    eprintln!(
        "warning: published image did not carry a concrete version; pinning by digest"
    );
    let short = digest
        .strip_prefix("sha256:")
        .map(|h| &h[..h.len().min(12)])
        .unwrap_or(digest);
    format!("sha256-{}", short)
}

fn scope_and_name(r: &ProtocolRef) -> (String, String) {
    match r {
        ProtocolRef::Registry { scope, name, .. } => (scope.clone(), name.clone()),
        ProtocolRef::Alias(_) => unreachable!(),
    }
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

