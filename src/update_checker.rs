use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use octocrab::Octocrab;
use reqwest::Client;

const CHECK_INTERVAL: u64 = 24 * 60 * 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateInfo {
    last_check: u64,
    tools: Vec<ManifestTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestTool {
    repo_name: String,
    repo_owner: String,
    min_version: String,
    max_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    tools: Vec<ManifestTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Tx3Tool {
    repo_name: String,
    repo_owner: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Tx3Tools {
    tools: Vec<Tx3Tool>,
}

pub struct UpdateChecker {
    update_file: PathBuf,
    tx3_file: PathBuf,
}

impl UpdateChecker {
    pub fn new() -> miette::Result<Self> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| miette::miette!("Failed to get cache directory"))?
            .join("trix");
        
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| miette::miette!("Failed to create cache directory: {}", e))?;
        
        let update_file = cache_dir.join("update.json");
        let tx3_file = Self::get_tx3_file()
            .ok_or_else(|| miette::miette!("Failed to get tx3 file"))?;

        Ok(Self {
            update_file,
            tx3_file,
        })
    }

    fn get_tx3_file() -> Option<PathBuf> {
        let path = if cfg!(target_os = "windows") {
            dirs::data_local_dir()
        } else {
            dirs::home_dir()
        };

        if let Some(mut path) = path {
            path.push(".tx3");
            path.push("versions.json");
            Some(path)
        } else {
            None
        }
    }

    pub async fn run(&self) {
        // Spawn a background task and wait for it to complete
        let checker = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(_) = checker.check_and_notify().await {
                // Silently ignore errors to avoid disrupting the main command
            }
        });
        
        // Wait for the update check to complete
        let _ = handle.await;
    }

    async fn check_and_notify(&self) -> miette::Result<()> {
        let update_info = self.load_update_info().map_or(None, |info| Some(info));
        let should_check = self.should_check_for_updates(update_info.as_ref())?;
        let tx3_tools = self.load_tx3_tools()?;
        
        if !should_check {
            // Check if we have a cached newer version to show
            if self.needs_to_update(&update_info.as_ref().unwrap(), &tx3_tools) {
                self.show_update_notification(&update_info.as_ref().unwrap(), &tx3_tools);
            }
            return Ok(());
        }

        // Perform the actual version check with timeout
        match timeout(REQUEST_TIMEOUT, self.fetch_manifest()).await {
            Ok(Ok(manifest)) => {
                let latest_update_info = self.save_update_info(manifest)?;
                if self.needs_to_update(&latest_update_info, &tx3_tools) {
                    self.show_update_notification(&latest_update_info, &tx3_tools);
                }
            }
            Ok(Err(_)) | Err(_) => {
                // Silently ignore network errors or timeouts
                // Just update the last check time to avoid frequent failed attempts
                self.update_last_check_time()?;
            }
        }

        Ok(())
    }

    fn load_update_info(&self) -> miette::Result<UpdateInfo> {
        let content = std::fs::read_to_string(&self.update_file)
            .map_err(|e| miette::miette!("Failed to read update file: {}", e))?;
        
        let update_info: UpdateInfo = serde_json::from_str(&content)
            .map_err(|e| miette::miette!("Failed to parse update file: {}", e))?;
        
        Ok(update_info)
    }

    fn should_check_for_updates(&self, update_info: Option<&UpdateInfo>) -> miette::Result<bool> {
        let last_check = match update_info {
            Some(update_info) => update_info.last_check,
            None => return Ok(true),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        Ok(now - last_check > CHECK_INTERVAL)
    }

    fn load_tx3_tools(&self) -> miette::Result<Tx3Tools> {
        let content = std::fs::read_to_string(&self.tx3_file)
            .map_err(|e| miette::miette!("Failed to read tx3 file: {}", e))?;
        
        let tx3_tools: Tx3Tools = serde_json::from_str(&content)
            .map_err(|e| miette::miette!("Failed to parse tx3 file: {}", e))?;
        
        Ok(tx3_tools)
    }

    fn needs_to_update(&self, update_info: &UpdateInfo, tx3_tools: &Tx3Tools) -> bool {
        for tool in &update_info.tools {
            if let Some(tx3_tool) = tx3_tools.tools.iter().find(|t| t.repo_name == tool.repo_name && t.repo_owner == tool.repo_owner) {
                if version_compare(&tool.min_version, &tx3_tool.version) {
                    return true;
                }
            }
        }
        false
    }

    fn show_update_notification(&self, update_info: &UpdateInfo, tx3_tools: &Tx3Tools) {
        println!();
        for tool in &update_info.tools {
            if let Some(tx3_tool) = tx3_tools.tools.iter().find(|t| t.repo_name == tool.repo_name && t.repo_owner == tool.repo_owner) {
                if version_compare(&tool.min_version, &tx3_tool.version) {
                    println!("  A new version of {} is available! 🎉", tx3_tool.repo_name);
                    println!("    Current version: {}", tx3_tool.version);
                    println!("    Latest version:  {}", tool.min_version);
                    println!();
                }
            }
        }
        println!("  Run 'tx3up' to update");
        println!();
    }

    async fn fetch_manifest(&self) -> miette::Result<Manifest> {
        let octocrab = Octocrab::builder().build()
            .map_err(|e| miette::miette!("Failed to create Octocrab client: {}", e))?;
        
        let repo = octocrab.repos("tx3-lang", "toolchain");
        
        let release = repo.releases().get_latest().await
            .map_err(|e| miette::miette!("Failed to fetch latest release: {}", e))?;

        let manifest_asset = release.assets.iter()
            .find(|asset| asset.name == "manifest.json")
            .ok_or_else(|| miette::miette!("No manifest asset found in latest release"))?;

        let manifest_content = self.fetch_manifest_content(manifest_asset.browser_download_url.as_ref()).await
            .map_err(|e| miette::miette!("Failed to fetch manifest: {}", e))?;

        let manifest: Manifest = serde_json::from_str(&manifest_content)
            .map_err(|e| miette::miette!("Failed to parse manifest file: {}", e))?;

        Ok(manifest)
    }

    async fn fetch_manifest_content(&self, url: &str) -> miette::Result<String> {
        let client = Client::new();
        let response = client.get(url).send().await
            .map_err(|e| miette::miette!("Failed to fetch manifest: {}", e))?;
        let data = response.text().await
            .map_err(|e| miette::miette!("Failed to read manifest response: {}", e))?;
        Ok(data)
    }

    fn save_update_info(&self, manifest: Manifest) -> miette::Result<UpdateInfo> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        let update_info = UpdateInfo {
            last_check: now,
            tools: manifest.tools.into_iter().map(|tool| ManifestTool {
                repo_name: tool.repo_name,
                repo_owner: tool.repo_owner,
                min_version: tool.min_version,
                max_version: tool.max_version,
            }).collect(),
        };

        let content = serde_json::to_string_pretty(&update_info)
            .map_err(|e| miette::miette!("Failed to serialize update info: {}", e))?;

        std::fs::write(&self.update_file, content)
            .map_err(|e| miette::miette!("Failed to write update file: {}", e))?;

        Ok(update_info)
    }

    fn update_last_check_time(&self) -> miette::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        let update_info = self.load_update_info().unwrap_or(UpdateInfo {
            last_check: now,
            tools: vec![],
        });

        let updated_info = UpdateInfo {
            last_check: now,
            tools: update_info.tools,
        };

        let content = serde_json::to_string_pretty(&updated_info)
            .map_err(|e| miette::miette!("Failed to serialize update info: {}", e))?;

        std::fs::write(&self.update_file, content)
            .map_err(|e| miette::miette!("Failed to write update file: {}", e))?;

        Ok(())
    }
}

impl Clone for UpdateChecker {
    fn clone(&self) -> Self {
        Self {
            update_file: self.update_file.clone(),
            tx3_file: self.tx3_file.clone(),
        }
    }
}

fn version_compare(version_a: &str, version_b: &str) -> bool {
    let parts_a: Vec<u32> = version_a
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    
    let parts_b: Vec<u32> = version_b
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect();
    
    let max_len = parts_a.len().max(parts_b.len());
    
    for i in 0..max_len {
        let a = parts_a.get(i).copied().unwrap_or(0);
        let b = parts_b.get(i).copied().unwrap_or(0);
        
        if a > b {
            return true;
        } else if a < b {
            return false;
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert!(version_compare("1.2.3", "1.2.2"));
        assert!(version_compare("1.3.0", "1.2.9"));
        assert!(version_compare("2.0.0", "1.9.9"));
        assert!(!version_compare("1.2.2", "1.2.3"));
        assert!(!version_compare("1.2.2", "1.2.2"));
    }
}
