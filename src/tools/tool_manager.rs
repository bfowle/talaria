use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::Tool;

/// Information about an installed tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub tool: String,
    pub version: String,
    pub installed_date: DateTime<Utc>,
    pub binary_path: PathBuf,
    pub is_current: bool,
}

/// Manager for external bioinformatics tools
pub struct ToolManager {
    tools_dir: PathBuf,
    client: reqwest::Client,
}

impl ToolManager {
    /// Create a new tool manager with the default tools directory
    pub fn new() -> Result<Self> {
        use crate::core::paths;
        let tools_dir = paths::talaria_tools_dir();

        Ok(Self {
            tools_dir,
            client: reqwest::Client::new(),
        })
    }

    /// Create a tool manager with a custom directory
    pub fn with_directory<P: AsRef<Path>>(dir: P) -> Self {
        Self {
            tools_dir: dir.as_ref().to_path_buf(),
            client: reqwest::Client::new(),
        }
    }

    /// Get the path to a tool's directory
    pub fn tool_dir(&self, tool: Tool) -> PathBuf {
        self.tools_dir.join(tool.name())
    }

    /// Get the current version directory for a tool
    pub fn current_dir(&self, tool: Tool) -> Option<PathBuf> {
        let current_link = self.tool_dir(tool).join("current");
        if current_link.exists() {
            fs::read_link(&current_link).ok().and_then(|p| {
                if p.is_absolute() {
                    Some(p)
                } else {
                    Some(self.tool_dir(tool).join(p))
                }
            })
        } else {
            None
        }
    }

    /// Get the path to a tool's binary if installed
    pub fn get_tool_path(&self, tool: Tool) -> Option<PathBuf> {
        self.current_dir(tool)
            .map(|dir| dir.join(tool.binary_name()))
            .filter(|p| p.exists())
    }

    /// Check if a tool is installed
    pub fn is_installed(&self, tool: Tool) -> bool {
        self.get_tool_path(tool).is_some()
    }

    /// Get the path to the current version of a tool
    pub fn get_current_tool_path(&self, tool: Tool) -> Result<PathBuf> {
        self.get_tool_path(tool).ok_or_else(|| {
            anyhow::anyhow!(
                "{} is not installed. Run: talaria tools install {}",
                tool.display_name(),
                tool.name()
            )
        })
    }

    /// List all installed versions of a tool
    pub fn list_versions(&self, tool: Tool) -> Result<Vec<ToolInfo>> {
        let tool_dir = self.tool_dir(tool);
        if !tool_dir.exists() {
            return Ok(Vec::new());
        }

        let current_version = self.get_current_version(tool)?;
        let mut versions = Vec::new();

        for entry in fs::read_dir(&tool_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() && path.file_name().unwrap() != "current" {
                if let Some(version) = path.file_name().and_then(|s| s.to_str()) {
                    let info_path = path.join("info.json");
                    if info_path.exists() {
                        let info_str = fs::read_to_string(&info_path)?;
                        let mut info: ToolInfo = serde_json::from_str(&info_str)?;
                        info.is_current = Some(version) == current_version.as_deref();
                        versions.push(info);
                    }
                }
            }
        }

        versions.sort_by(|a, b| b.installed_date.cmp(&a.installed_date));
        Ok(versions)
    }

    /// Get the current version of a tool
    pub fn get_current_version(&self, tool: Tool) -> Result<Option<String>> {
        let current_link = self.tool_dir(tool).join("current");
        if !current_link.exists() {
            return Ok(None);
        }

        let target = fs::read_link(&current_link)?;
        Ok(target
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()))
    }

    /// Set the current version of a tool
    pub fn set_current_version(&self, tool: Tool, version: &str) -> Result<()> {
        let tool_dir = self.tool_dir(tool);
        let version_dir = tool_dir.join(version);

        if !version_dir.exists() {
            anyhow::bail!("{} version {} is not installed", tool, version);
        }

        let current_link = tool_dir.join("current");

        // Remove old symlink if it exists
        if current_link.exists() {
            fs::remove_file(&current_link)?;
        }

        // Create new symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&version_dir, &current_link)?;
        }

        #[cfg(not(unix))]
        {
            anyhow::bail!("Symlinks are not supported on this platform");
        }

        Ok(())
    }

    /// Verify that a tool installation is complete and valid
    fn verify_tool_installation(&self, tool: Tool, version_dir: &Path) -> bool {
        // Check if binary exists
        let binary_path = version_dir.join(tool.binary_name());
        if !binary_path.exists() || !binary_path.is_file() {
            return false;
        }

        // Check if info.json exists
        let info_path = version_dir.join("info.json");
        if !info_path.exists() {
            return false;
        }

        // Check if binary is executable (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&binary_path) {
                let perms = metadata.permissions();
                if perms.mode() & 0o111 == 0 {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Clean up temporary directories from failed installations
    fn cleanup_temp_dirs(&self, tool: Tool) -> Result<()> {
        let tool_dir = self.tool_dir(tool);
        if !tool_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&tool_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(".tmp_") {
                    println!("Cleaning up temporary directory: {}", name);
                    fs::remove_dir_all(&path).ok();
                }
            }
        }
        Ok(())
    }

    /// Download and install LAMBDA
    pub async fn install_lambda(&self, version: Option<&str>) -> Result<()> {
        let version = match version {
            Some(v) => v.to_string(),
            None => self.get_latest_lambda_version().await?,
        };

        let tool_dir = self.tool_dir(Tool::Lambda);
        let version_dir = tool_dir.join(&version);
        let temp_dir = tool_dir.join(format!(".tmp_{}", version));

        // Clean up any old temporary directories
        self.cleanup_temp_dirs(Tool::Lambda)?;

        // Check if already installed and valid
        if version_dir.exists() {
            if self.verify_tool_installation(Tool::Lambda, &version_dir) {
                println!("✓ LAMBDA {} is already installed and verified", version);
                self.set_current_version(Tool::Lambda, &version)?;
                return Ok(());
            } else {
                println!(
                    "⚠ LAMBDA {} directory exists but installation is incomplete/corrupt",
                    version
                );
                println!("  Repairing installation...");
                // Remove the broken installation
                fs::remove_dir_all(&version_dir)?;
            }
        }

        println!("📦 Installing LAMBDA version {}...", version);

        // Create temporary directory for download
        fs::create_dir_all(&temp_dir)?;

        // Determine platform
        let (os, arch) = self.detect_platform()?;

        // Download URL for LAMBDA
        // Extract version number from tag (e.g., "lambda-v3.1.0" -> "3.1.0")
        let version_num = version.trim_start_matches("lambda-v");

        let (platform_str, extension) = match (os.as_str(), arch.as_str()) {
            ("linux", "x86_64") => ("Linux-x86_64", "tar.xz"),
            ("macos", "x86_64") => ("Darwin-x86_64", "zip"),
            _ => anyhow::bail!("Unsupported platform: {}-{}", os, arch),
        };

        let download_url = format!(
            "https://github.com/seqan/lambda/releases/download/{}/lambda3-{}-{}.{}",
            version, version_num, platform_str, extension
        );

        println!("⬇ Downloading from {}...", download_url);

        // Download the archive
        let response = self
            .client
            .get(&download_url)
            .send()
            .await
            .context("Failed to download LAMBDA")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download LAMBDA: HTTP {}", response.status());
        }

        let bytes = response.bytes().await?;

        // Determine archive type and extract accordingly
        let (archive_path, archive_type) = if download_url.ends_with(".tar.xz") {
            (temp_dir.join("lambda.tar.xz"), "tar.xz")
        } else if download_url.ends_with(".zip") {
            (temp_dir.join("lambda.zip"), "zip")
        } else {
            (temp_dir.join("lambda.tar.gz"), "tar.gz")
        };

        fs::write(&archive_path, &bytes)?;

        // Extract the archive
        println!("📂 Extracting LAMBDA...");
        match archive_type {
            "tar.xz" => self.extract_tar_xz(&archive_path, &temp_dir)?,
            "zip" => self.extract_zip(&archive_path, &temp_dir)?,
            _ => self.extract_tar_gz(&archive_path, &temp_dir)?,
        }

        // Remove archive
        fs::remove_file(&archive_path)?;

        // Find and move the lambda3 binary
        // The archive extracts to a subdirectory like lambda3-3.1.0-Linux-x86_64/bin/lambda3
        let extracted_dir = temp_dir
            .read_dir()?
            .filter_map(|entry| entry.ok())
            .find(|entry| {
                entry.path().is_dir() && entry.file_name().to_string_lossy().starts_with("lambda3-")
            })
            .map(|entry| entry.path())
            .context("Could not find extracted lambda directory")?;

        let extracted_binary = extracted_dir.join("bin").join("lambda3");
        let final_binary_path = temp_dir.join("lambda3");

        // Move the binary to the version directory
        if extracted_binary.exists() {
            fs::rename(&extracted_binary, &final_binary_path)?;

            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&final_binary_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&final_binary_path, perms)?;
            }

            // Clean up the extracted directory
            fs::remove_dir_all(&extracted_dir)?;
        } else {
            anyhow::bail!(
                "Binary not found after extraction at {:?}",
                extracted_binary
            );
        }

        // Save tool info
        let info = ToolInfo {
            tool: Tool::Lambda.name().to_string(),
            version: version.clone(),
            installed_date: Utc::now(),
            binary_path: final_binary_path, // Use the actual moved binary path
            is_current: true,
        };

        let info_json = serde_json::to_string_pretty(&info)?;
        fs::write(temp_dir.join("info.json"), info_json)?;

        // Verify the installation in temp directory
        if !self.verify_tool_installation(Tool::Lambda, &temp_dir) {
            fs::remove_dir_all(&temp_dir)?;
            anyhow::bail!("Installation verification failed");
        }

        // Move from temp to final directory (atomic operation)
        fs::rename(&temp_dir, &version_dir)
            .context("Failed to move installation to final directory")?;

        // Set as current version
        self.set_current_version(Tool::Lambda, &version)?;

        println!("✓ Successfully installed LAMBDA {}", version);
        Ok(())
    }

    /// Get the latest version of LAMBDA from GitHub
    async fn get_latest_lambda_version(&self) -> Result<String> {
        let api_url = "https://api.github.com/repos/seqan/lambda/releases/latest";

        let response = self
            .client
            .get(api_url)
            .header("User-Agent", "talaria")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch latest version: HTTP {}", response.status());
        }

        let release: serde_json::Value = response.json().await?;
        let tag = release["tag_name"]
            .as_str()
            .context("Could not parse release tag")?;

        // Keep the full tag for consistency
        Ok(tag.to_string())
    }

    /// Compare two version strings (supports semantic versioning)
    pub fn compare_versions(&self, v1: &str, v2: &str) -> Ordering {
        // Strip common prefixes
        let v1_clean = v1.trim_start_matches("lambda-v").trim_start_matches('v');
        let v2_clean = v2.trim_start_matches("lambda-v").trim_start_matches('v');

        // Parse semantic version parts
        let v1_parts: Vec<u32> = v1_clean.split('.').filter_map(|s| s.parse().ok()).collect();
        let v2_parts: Vec<u32> = v2_clean.split('.').filter_map(|s| s.parse().ok()).collect();

        // Compare each part
        for i in 0..std::cmp::max(v1_parts.len(), v2_parts.len()) {
            let p1 = v1_parts.get(i).unwrap_or(&0);
            let p2 = v2_parts.get(i).unwrap_or(&0);
            match p1.cmp(p2) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }

    /// Check if an upgrade is available for a tool
    pub async fn check_for_upgrade(&self, tool: Tool) -> Result<Option<String>> {
        let current_version = match self.get_current_version(tool)? {
            Some(v) => v,
            None => return Ok(None), // Tool not installed
        };

        let latest_version = match tool {
            Tool::Lambda => self.get_latest_lambda_version().await?,
            _ => return Ok(None), // Other tools not implemented yet
        };

        if self.compare_versions(&latest_version, &current_version) == Ordering::Greater {
            Ok(Some(latest_version))
        } else {
            Ok(None)
        }
    }

    /// Detect the current platform
    fn detect_platform(&self) -> Result<(String, String)> {
        let os = if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            anyhow::bail!("Unsupported operating system");
        };

        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            anyhow::bail!("Unsupported architecture");
        };

        Ok((os.to_string(), arch.to_string()))
    }

    /// Extract a tar.gz archive
    fn extract_tar_gz(&self, archive_path: &Path, dest_dir: &Path) -> Result<()> {
        use flate2::read::GzDecoder;
        use tar::Archive;

        let file = fs::File::open(archive_path)?;
        let gz = GzDecoder::new(file);
        let mut archive = Archive::new(gz);

        archive.unpack(dest_dir)?;
        Ok(())
    }

    /// Extract a tar.xz archive
    fn extract_tar_xz(&self, archive_path: &Path, dest_dir: &Path) -> Result<()> {
        use std::process::Command;

        // Use system tar command for xz archives
        let output = Command::new("tar")
            .args([
                "-xf",
                archive_path.to_str().unwrap(),
                "-C",
                dest_dir.to_str().unwrap(),
            ])
            .output()
            .context("Failed to extract tar.xz archive")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to extract archive: {}", stderr);
        }

        Ok(())
    }

    /// Extract a zip archive
    fn extract_zip(&self, archive_path: &Path, dest_dir: &Path) -> Result<()> {
        use std::process::Command;

        // Use system unzip command
        let output = Command::new("unzip")
            .args([
                "-q",
                archive_path.to_str().unwrap(),
                "-d",
                dest_dir.to_str().unwrap(),
            ])
            .output()
            .context("Failed to extract zip archive")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to extract archive: {}", stderr);
        }

        Ok(())
    }

    /// List all installed tools
    pub fn list_all_tools(&self) -> Result<Vec<(Tool, Vec<ToolInfo>)>> {
        let mut results = Vec::new();

        for tool in &[Tool::Lambda, Tool::Blast, Tool::Diamond, Tool::Mmseqs2] {
            let versions = self.list_versions(*tool)?;
            if !versions.is_empty() {
                results.push((*tool, versions));
            }
        }

        Ok(results)
    }
}
