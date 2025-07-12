use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

const CHECK_INTERVAL: u64 = 24 * 60 * 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateInfo {
    last_check: u64,
    latest_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

pub struct UpdateChecker {
    cache_file: PathBuf,
    current_version: String,
}

impl UpdateChecker {
    pub fn new() -> miette::Result<Self> {
        let cache_dir = dirs::cache_dir()
            .ok_or_else(|| miette::miette!("Failed to get cache directory"))?
            .join("trix");
        
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| miette::miette!("Failed to create cache directory: {}", e))?;
        
        let cache_file = cache_dir.join("update_info.json");
        let current_version = env!("CARGO_PKG_VERSION").to_string();

        Ok(Self {
            cache_file,
            current_version,
        })
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
        let should_check = self.should_check_for_updates()?;
        
        if !should_check {
            // Check if we have a cached newer version to show
            if let Ok(update_info) = self.load_update_info() {
                if let Some(latest_version) = &update_info.latest_version {
                    if self.is_newer_version(latest_version) {
                        self.show_update_notification(latest_version);
                    }
                }
            }
            return Ok(());
        }

        // Perform the actual version check with timeout
        match timeout(REQUEST_TIMEOUT, self.fetch_latest_version()).await {
            Ok(Ok(latest_version)) => {
                self.save_update_info(&latest_version)?;
                
                if self.is_newer_version(&latest_version) {
                    self.show_update_notification(&latest_version);
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

    fn should_check_for_updates(&self) -> miette::Result<bool> {
        let update_info = match self.load_update_info() {
            Ok(info) => info,
            Err(_) => return Ok(true),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        Ok(now - update_info.last_check > CHECK_INTERVAL)
    }

    fn load_update_info(&self) -> miette::Result<UpdateInfo> {
        let content = std::fs::read_to_string(&self.cache_file)
            .map_err(|e| miette::miette!("Failed to read cache file: {}", e))?;
        
        let update_info: UpdateInfo = serde_json::from_str(&content)
            .map_err(|e| miette::miette!("Failed to parse cache file: {}", e))?;
        
        Ok(update_info)
    }

    fn is_newer_version(&self, latest_version: &str) -> bool {
        version_compare(latest_version, &self.current_version)
    }

    fn show_update_notification(&self, latest_version: &str) {
        println!();
        println!("  A new version of trix is available! 🎉");
        println!("    Current version: {}", self.current_version);
        println!("    Latest version:  {}", latest_version);
        println!("  Run 'tx3up' to update");
        println!();
    }

    async fn fetch_latest_version(&self) -> miette::Result<String> {
        let client = reqwest::Client::new();
        let url = "https://api.github.com/repos/tx3-lang/trix/releases/latest";
        
        let response = client
            .get(url)
            .header("User-Agent", format!("trix/{}", self.current_version))
            .send()
            .await
            .map_err(|e| miette::miette!("Failed to fetch release info: {}", e))?;

        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|e| miette::miette!("Failed to parse release info: {}", e))?;

        let version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
        Ok(version.to_string())
    }

    fn save_update_info(&self, latest_version: &str) -> miette::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        let update_info = UpdateInfo {
            last_check: now,
            latest_version: Some(latest_version.to_string()),
        };

        let content = serde_json::to_string_pretty(&update_info)
            .map_err(|e| miette::miette!("Failed to serialize update info: {}", e))?;

        std::fs::write(&self.cache_file, content)
            .map_err(|e| miette::miette!("Failed to write cache file: {}", e))?;

        Ok(())
    }

    fn update_last_check_time(&self) -> miette::Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| miette::miette!("Failed to get current time: {}", e))?
            .as_secs();

        let update_info = self.load_update_info().unwrap_or(UpdateInfo {
            last_check: now,
            latest_version: None,
        });

        let updated_info = UpdateInfo {
            last_check: now,
            latest_version: update_info.latest_version,
        };

        let content = serde_json::to_string_pretty(&updated_info)
            .map_err(|e| miette::miette!("Failed to serialize update info: {}", e))?;

        std::fs::write(&self.cache_file, content)
            .map_err(|e| miette::miette!("Failed to write cache file: {}", e))?;

        Ok(())
    }
}

impl Clone for UpdateChecker {
    fn clone(&self) -> Self {
        Self {
            cache_file: self.cache_file.clone(),
            current_version: self.current_version.clone(),
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
