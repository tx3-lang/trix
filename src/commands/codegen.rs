use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config::{
    CodegenConfig, CodegenPlugin, CodegenPluginConfig, KNOWN_CODEGEN_PLUGINS, KnownCodegenPlugin,
    ProfileConfig, RootConfig,
};
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use reqwest::Client;
use tempfile::TempDir;
use zip::ZipArchive;

#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Codegen plugin to use, e.g. `ts-client`, `rust-client`,
    /// `python-client`, `go-client`. If no `[[codegen]]` entry exists for
    /// this plugin yet, one is appended to `trix.toml` before generation
    /// runs. With this flag, `trix codegen` is the only path that needs
    /// to know plugin names; hand-editing `trix.toml` stays supported for
    /// custom plugins and bespoke `output_dir`s.
    #[arg(long, value_name = "NAME")]
    pub plugin: Option<String>,

    /// Generate without persisting a newly-seeded `[[codegen]]` entry
    /// back to `trix.toml`. Intended for CI / one-shot scripts that emit
    /// bindings without mutating the project file.
    #[arg(long)]
    pub no_save: bool,
}

async fn extract_github_templates(
    github_url: &str,
    temp_dir: &TempDir,
    path: &str,
) -> miette::Result<PathBuf> {
    let local_root = PathBuf::from(github_url);
    if local_root.is_dir() {
        let template_root = local_root.join(path);
        if !template_root.is_dir() {
            return Err(miette::miette!(
                "Template path '{}' does not exist",
                template_root.display()
            ));
        }
        return Ok(template_root);
    }

    let parts: Vec<&str> = github_url.split('/').collect();
    if parts.len() < 2 {
        return Err(miette::miette!(
            "Invalid GitHub URL format. Use 'owner/repo' or 'owner/repo/branch'"
        ));
    }

    let owner = parts[0];
    let repo = parts[1];
    let branch = if parts.len() > 2 { parts[2] } else { "main" };

    let zip_url = format!(
        "https://github.com/{}/{}/archive/{}.zip",
        owner, repo, branch
    );

    println!(
        "Reading template from https://github.com/{}/{} (ref: {})",
        owner, repo, branch
    );

    let client = Client::new();
    let response = client.get(&zip_url).send().await.into_diagnostic()?;

    if !response.status().is_success() {
        return Err(miette::miette!(
            "Failed to download GitHub repository: HTTP {}",
            response.status()
        ));
    }

    let zip_path = temp_dir.path().join("bindgen-template.zip");
    let content = response.bytes().await.into_diagnostic()?;
    std::fs::write(&zip_path, &content).into_diagnostic()?;

    let file = std::fs::File::open(&zip_path).into_diagnostic()?;
    let mut archive = ZipArchive::new(file).into_diagnostic()?;

    let mut bindgen_path = PathBuf::new();
    let root_dir_name = archive.name_for_index(0).unwrap_or("");
    bindgen_path.push(root_dir_name);
    bindgen_path.push(path);
    bindgen_path.push("");

    let bindgen_path_string = bindgen_path.to_string_lossy().to_string();
    let archive_bindgen_index = archive.index_for_name(&bindgen_path_string).unwrap_or(0);

    let template_root = temp_dir.path().join("templates");
    std::fs::create_dir_all(&template_root).into_diagnostic()?;

    for i in archive_bindgen_index..archive.len() {
        let mut file = archive.by_index(i).into_diagnostic()?;
        let name = file.name().to_owned();

        if !name.starts_with(&bindgen_path_string) {
            break;
        }

        if file.is_dir() || name.ends_with("trix-bindgen.toml") {
            continue;
        }

        let relative = name.strip_prefix(&bindgen_path_string).unwrap_or(&name);
        let dest_path = template_root.join(relative);
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent).into_diagnostic()?;
        }

        let mut out_file = std::fs::File::create(&dest_path).into_diagnostic()?;
        std::io::copy(&mut file, &mut out_file).into_diagnostic()?;
    }

    Ok(template_root)
}

/// Output-subdir names, in generation order: the project (if it has a `tx3`
/// source on disk) first, then each interface alias. The name doubles as
/// the per-protocol output subdir — the layout is unconditional, so the
/// path a binding lands at never depends on interface count.
fn codegen_targets(project_name: Option<&str>, dep_aliases: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(usize::from(project_name.is_some()) + dep_aliases.len());
    if let Some(name) = project_name {
        out.push(name.to_string());
    }
    out.extend(dep_aliases.iter().cloned());
    out
}

/// Resolves each codegen target to `(subdir_name, tii_path)`. The project's
/// TII is built from source; each interface's TII is the cached, pre-built
/// published one (not recompiled), consistent with `trix build`.
///
/// Consumer projects (those bootstrapped by `trix use` with no `main.tx3`)
/// don't have a project of their own to generate for — only interfaces. We
/// detect that by looking for `protocol.main` on disk relative to the
/// project root; if it's missing, the project is silently skipped.
fn collect_codegen_targets(
    config: &RootConfig,
    project_root: &Path,
) -> miette::Result<Vec<(String, PathBuf)>> {
    let dep_aliases: Vec<String> = config
        .interfaces
        .values()
        .map(|e| e.alias.clone())
        .collect();

    let project_source = project_root.join(&config.protocol.main);
    let project_name = project_source
        .is_file()
        .then(|| config.protocol.name.as_str());

    let order = codegen_targets(project_name, &dep_aliases);
    if order.is_empty() {
        return Err(miette::miette!(
            "nothing to generate: no `{}` found and no interfaces declared in trix.toml",
            config.protocol.main.display()
        ));
    }

    let mut targets = Vec::with_capacity(order.len());
    for name in order {
        // `validate` guarantees no interface alias equals the project
        // name, so name == protocol.name ⇒ the project.
        let tii = if name == config.protocol.name {
            crate::builder::build_tii(config)?
        } else {
            let entry = config
                .interfaces
                .values()
                .find(|e| e.alias == name)
                .expect("alias originates from config.interfaces");
            crate::interfaces::cache_paths(entry)?.tii
        };
        targets.push((name, tii));
    }

    Ok(targets)
}

/// Resolve which plugin (if any) the user requested for this invocation,
/// either through `--plugin <name>` or — when no `[[codegen]]` is configured
/// and stdin is a TTY — an interactive prompt. Returns `None` when the
/// project already has at least one target and the user passed no flag
/// (i.e. the existing non-interactive behavior).
fn resolve_requested_plugin(
    explicit: Option<&str>,
    config: &RootConfig,
) -> miette::Result<Option<KnownCodegenPlugin>> {
    if let Some(name) = explicit {
        let plugin: KnownCodegenPlugin = name.parse().map_err(|e: String| miette::miette!("{e}"))?;
        return Ok(Some(plugin));
    }

    if !config.codegen.is_empty() {
        return Ok(None);
    }

    if !std::io::stdin().is_terminal() {
        return Err(miette::miette!(
            "no [[codegen]] targets configured; pass --plugin <{}> to seed one",
            KNOWN_CODEGEN_PLUGINS
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("|")
        ));
    }

    let choice = inquire::Select::new(
        "Generate bindings for:",
        KNOWN_CODEGEN_PLUGINS.to_vec(),
    )
    .prompt()
    .into_diagnostic()?;

    Ok(Some(choice))
}

/// Seed-if-absent: if `config` has no `[[codegen]]` entry matching `plugin`,
/// append a minimal one and persist (unless `no_save`). Returns the
/// possibly-mutated config to use for the rest of the run. Comparison goes
/// through the enum so the verbose `KnownOrCustom::Known` form in TOML still
/// dedups against the short form.
fn seed_plugin_if_absent(
    mut config: RootConfig,
    plugin: KnownCodegenPlugin,
    config_path: &Path,
    no_save: bool,
) -> miette::Result<RootConfig> {
    let already = config.codegen.iter().any(|c| match c.plugin {
        CodegenPlugin::Known(known) => std::mem::discriminant(&known) == std::mem::discriminant(&plugin),
        CodegenPlugin::Custom(_) => false,
    });

    if already {
        return Ok(config);
    }

    config.codegen.push(CodegenConfig {
        plugin: CodegenPlugin::Known(plugin),
        job_id: None,
        output_dir: None,
        options: None,
    });

    if !no_save {
        config.save(&config_path.to_path_buf())?;
        eprintln!("Added [[codegen]] plugin = \"{plugin}\" to trix.toml.");
    }

    Ok(config)
}

pub async fn run(
    args: Args,
    config: &RootConfig,
    config_path: &Path,
    _profile: &ProfileConfig,
) -> miette::Result<()> {
    let requested = resolve_requested_plugin(args.plugin.as_deref(), config)?;
    let config = match requested {
        Some(plugin) => seed_plugin_if_absent(config.clone(), plugin, config_path, args.no_save)?,
        None => config.clone(),
    };
    let config = &config;

    crate::interfaces::validate(config)?;
    crate::interfaces::restore_all(config)?;

    let project_root = config_path.parent().unwrap_or_else(|| Path::new("."));
    let targets = collect_codegen_targets(config, project_root)?;

    for codegen in config.codegen.iter() {
        let base_output_dir = codegen.output_dir()?;
        std::fs::create_dir_all(&base_output_dir).into_diagnostic()?;

        let plugin = CodegenPluginConfig::from(codegen.plugin.clone());
        let github_url = if PathBuf::from(&plugin.repo).is_dir() {
            plugin.repo.clone()
        } else {
            format!(
                "{}/{}",
                &plugin.repo,
                plugin.r#ref.as_deref().unwrap_or("main")
            )
        };

        // Extract templates once per [[codegen]] entry, reuse across protocols.
        let template_temp = TempDir::new().into_diagnostic()?;
        let templates_dir =
            extract_github_templates(&github_url, &template_temp, &plugin.path).await?;

        for (name, tii_path) in &targets {
            let dest = base_output_dir.join(name);
            std::fs::create_dir_all(&dest).into_diagnostic()?;
            crate::spawn::tx3c::codegen(tii_path, &templates_dir, &dest)?;
            println!("Bindgen successful for '{}'", name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::codegen_targets;

    #[test]
    fn targets_without_deps_still_nest_project() {
        assert_eq!(codegen_targets(Some("proj"), &[]), vec!["proj".to_string()]);
    }

    #[test]
    fn targets_project_first_then_deps_in_order() {
        assert_eq!(
            codegen_targets(Some("proj"), &["a".to_string(), "b".to_string()]),
            vec!["proj".to_string(), "a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn consumer_project_targets_skip_own_protocol() {
        assert_eq!(
            codegen_targets(None, &["a".to_string(), "b".to_string()]),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn consumer_project_with_no_deps_is_empty() {
        assert!(codegen_targets(None, &[]).is_empty());
    }
}
