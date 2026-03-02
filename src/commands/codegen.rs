use std::path::PathBuf;

use crate::config::{CodegenPluginConfig, ProfileConfig, RootConfig};
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use reqwest::Client;
use tempfile::TempDir;
use zip::ZipArchive;

#[derive(ClapArgs, Debug)]
pub struct Args {}

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

pub async fn run(_args: Args, config: &RootConfig, _profile: &ProfileConfig) -> miette::Result<()> {
    let tii_temp = TempDir::new().into_diagnostic()?;
    let tii_path = tii_temp.path().join("protocol.tii");

    crate::spawn::tx3c::build_tii(&config.protocol.main, &tii_path, config)?;

    for codegen in config.codegen.iter() {
        let output_dir = codegen.output_dir()?;
        std::fs::create_dir_all(&output_dir).into_diagnostic()?;

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

        let template_temp = TempDir::new().into_diagnostic()?;
        let templates_dir = extract_github_templates(&github_url, &template_temp, &plugin.path).await?;

        crate::spawn::tx3c::codegen(&tii_path, &templates_dir, &output_dir)?;
        println!("Bindgen successful");
    }

    Ok(())
}
