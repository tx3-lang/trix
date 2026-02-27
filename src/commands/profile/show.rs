use askama::Template;
use termimad::MadSkin;

use crate::config::{NetworkConfig, ProfileConfig, RootConfig};

use super::{
    ConfigSource, EndpointView, EnvFileStatus, EnvFileView, IdentityView, NetworkView, ProfileView,
    load_and_mask_env_vars, mask_value, resolve_network_source, resolve_profile_source,
};

// ============================================================================
// Askama Template
// ============================================================================

#[derive(Template)]
#[template(path = "profile/show.md")]
struct ProfileShowTemplate<'a> {
    view: &'a ProfileView,
}

impl<'a> ProfileShowTemplate<'a> {
    fn render_view(view: &'a ProfileView) -> String {
        ProfileShowTemplate { view }
            .render()
            .expect("Template rendering failed")
    }
}

// ============================================================================
// Command Entry Point
// ============================================================================

pub fn run(
    args: super::ShowArgs,
    config: &RootConfig,
    _profile: &ProfileConfig,
) -> miette::Result<()> {
    let view = build_profile_view(config, &args.name)?;
    render_profile_view(&view);
    Ok(())
}

// ============================================================================
// View Building (Materialization)
// ============================================================================

fn build_profile_view(config: &RootConfig, profile_name: &str) -> miette::Result<ProfileView> {
    let profile = config.resolve_profile(profile_name)?;
    let network = config.resolve_profile_network(profile_name)?;

    let profile_source = resolve_profile_source(profile_name, config);
    let network_source = resolve_network_source(&network.name, config);

    Ok(ProfileView {
        name: profile.name.clone(),
        source: profile_source,
        network: build_network_view(&network, network_source),
        identities: build_identities_view(&profile),
        env_file: build_env_file_view(&profile),
    })
}

fn build_network_view(network: &NetworkConfig, source: ConfigSource) -> NetworkView {
    NetworkView {
        name: network.name.clone(),
        source: source.clone(),
        is_testnet: network.is_testnet,
        trp: build_endpoint_view(&network.trp.url, &network.trp.headers, source.clone()),
        u5c: build_endpoint_view(&network.u5c.url, &network.u5c.headers, source.clone()),
    }
}

fn build_endpoint_view(
    url: &str,
    headers: &std::collections::HashMap<String, String>,
    source: ConfigSource,
) -> EndpointView {
    EndpointView {
        url: url.to_string(),
        url_source: source,
        headers: headers
            .iter()
            .map(|(k, v)| (k.clone(), mask_value(v)))
            .collect(),
    }
}

fn build_identities_view(profile: &ProfileConfig) -> Vec<IdentityView> {
    use crate::config::serde::Named;

    profile
        .identities
        .values()
        .map(|identity| IdentityView {
            name: identity.name(),
            kind: match identity {
                crate::config::IdentityConfig::RandomKey(_) => "random-key".to_string(),
                crate::config::IdentityConfig::ExplicitKey(_) => "explicit-key".to_string(),
            },
        })
        .collect()
}

fn build_env_file_view(profile: &ProfileConfig) -> EnvFileView {
    let env_file_path = profile.env_file_path();
    let file_name = env_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".env.{profile}")
        .to_string();

    if !env_file_path.is_file() {
        return EnvFileView {
            file_name,
            status: EnvFileStatus::NotFound,
            variables: vec![],
        };
    }

    match load_and_mask_env_vars(&env_file_path) {
        Ok(vars) => EnvFileView {
            file_name,
            status: EnvFileStatus::Found,
            variables: vars,
        },
        Err(e) => EnvFileView {
            file_name,
            status: EnvFileStatus::Error(e.to_string()),
            variables: vec![],
        },
    }
}

// ============================================================================
// Rendering
// ============================================================================

fn render_profile_view(view: &ProfileView) {
    let markdown = ProfileShowTemplate::render_view(view);
    let skin = MadSkin::default();
    skin.print_text(&markdown);
}
