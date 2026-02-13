use askama::Template;
use termimad::MadSkin;

use crate::config::RootConfig;

use super::{
    resolve_network_source, resolve_profile_source, NetworkListItem, ProfileListItem,
    ProfileListView,
};

// ============================================================================
// Askama Template
// ============================================================================

#[derive(Template)]
#[template(path = "profile/list.md")]
struct ProfileListTemplate<'a> {
    view: &'a ProfileListView,
}

impl<'a> ProfileListTemplate<'a> {
    fn render_view(view: &'a ProfileListView) -> String {
        ProfileListTemplate { view }
            .render()
            .expect("Template rendering failed")
    }
}

// ============================================================================
// Command Entry Point
// ============================================================================

pub fn run(
    _args: super::ListArgs,
    config: &RootConfig,
    _profile: &crate::config::ProfileConfig,
) -> miette::Result<()> {
    let view = build_profile_list_view(config)?;
    render_profile_list_view(&view);
    Ok(())
}

// ============================================================================
// View Building (Materialization)
// ============================================================================

fn build_profile_list_view(config: &RootConfig) -> miette::Result<ProfileListView> {
    let all_profiles = config.available_profiles();
    let all_networks = config.available_networks();

    let profile_items: Vec<_> = all_profiles
        .iter()
        .map(|name| {
            let source = resolve_profile_source(name, config);
            let network_name = config
                .resolve_profile(name)
                .map(|p| p.network.clone())
                .unwrap_or_default();
            let network_source = resolve_network_source(&network_name, config);

            ProfileListItem {
                name: name.clone(),
                source,
                network: network_name,
                network_source,
            }
        })
        .collect();

    let network_items: Vec<_> = all_networks
        .iter()
        .map(|name| {
            let source = resolve_network_source(name, config);
            NetworkListItem {
                name: name.clone(),
                source,
            }
        })
        .collect();

    Ok(ProfileListView {
        profiles: profile_items,
        networks: network_items,
    })
}

// ============================================================================
// Rendering
// ============================================================================

fn render_profile_list_view(view: &ProfileListView) {
    let markdown = ProfileListTemplate::render_view(view);
    let skin = MadSkin::default();
    skin.print_text(&markdown);
}
