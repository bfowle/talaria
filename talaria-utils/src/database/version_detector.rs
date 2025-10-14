/// Version detection for bioinformatics databases
///
/// Detects and extracts upstream version information from database files
/// to provide meaningful version names instead of timestamps
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a database version with multiple naming schemes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseVersion {
    /// Primary timestamp-based identifier (e.g., "20250915_053033")
    /// This is the actual directory name and prevents collisions
    pub timestamp: String,

    /// Upstream version if detected (e.g., "2024_04" for UniProt)
    pub upstream_version: Option<String>,

    /// Source database type
    pub source: String,

    /// Dataset name
    pub dataset: String,

    /// When this version was downloaded/created
    pub created_at: DateTime<Utc>,

    /// Different types of aliases
    pub aliases: VersionAliases,

    /// Available reduction profiles for this version
    pub profiles: Vec<String>,

    /// Additional metadata (sequences count, size, etc.)
    pub metadata: HashMap<String, String>,
}

/// Categorized aliases for a version
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionAliases {
    /// System aliases (current, latest, stable)
    pub system: Vec<String>,

    /// Upstream aliases (e.g., "2024_04" for UniProt)
    pub upstream: Vec<String>,

    /// User-defined custom aliases
    pub custom: Vec<String>,
}

impl DatabaseVersion {
    /// Create a new database version with timestamp as primary key
    pub fn new(source: &str, dataset: &str) -> Self {
        let now = Utc::now();
        // Use timestamp as primary identifier - no UUID needed for uniqueness
        let timestamp = format!("{}", now.format("%Y%m%d_%H%M%S"));

        Self {
            timestamp: timestamp.clone(),
            upstream_version: None,
            source: source.to_string(),
            dataset: dataset.to_string(),
            created_at: now,
            aliases: VersionAliases {
                system: vec!["latest".to_string()],
                upstream: Vec::new(),
                custom: Vec::new(),
            },
            profiles: vec!["auto-detect".to_string()],
            metadata: HashMap::new(),
        }
    }

    /// Get the display name for this version (prefer upstream over timestamp)
    pub fn display_name(&self) -> &str {
        self.upstream_version.as_ref().unwrap_or(&self.timestamp)
    }

    /// Get all aliases for this version
    pub fn all_aliases(&self) -> Vec<String> {
        let mut all = Vec::new();
        all.extend(self.aliases.system.clone());
        all.extend(self.aliases.upstream.clone());
        all.extend(self.aliases.custom.clone());
        all
    }

    /// Check if this version matches a given reference
    pub fn matches(&self, reference: &str) -> bool {
        // Check timestamp (primary key)
        if self.timestamp == reference {
            return true;
        }

        // Check upstream version
        if let Some(ref upstream) = self.upstream_version {
            if upstream == reference {
                return true;
            }
        }

        // Check all alias categories
        if self.aliases.system.iter().any(|a| a == reference)
            || self.aliases.upstream.iter().any(|a| a == reference)
            || self.aliases.custom.iter().any(|a| a == reference)
        {
            return true;
        }

        false
    }

    /// Add a custom alias to this version
    pub fn add_custom_alias(&mut self, alias: String) {
        if !self.aliases.custom.contains(&alias) {
            self.aliases.custom.push(alias);
        }
    }

    /// Add a system alias to this version
    pub fn add_system_alias(&mut self, alias: String) {
        if !self.aliases.system.contains(&alias) {
            self.aliases.system.push(alias);
        }
    }

    /// Remove a custom alias (system/upstream aliases are protected)
    pub fn remove_custom_alias(&mut self, alias: &str) -> bool {
        if let Some(pos) = self.aliases.custom.iter().position(|a| a == alias) {
            self.aliases.custom.remove(pos);
            true
        } else {
            false
        }
    }
}

/// Version detector for different database sources
pub struct VersionDetector {
    detectors: HashMap<String, Box<dyn VersionExtractor>>,
}

impl Default for VersionDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionDetector {
    /// Create a new version detector with built-in extractors
    pub fn new() -> Self {
        let mut detectors: HashMap<String, Box<dyn VersionExtractor>> = HashMap::new();

        // Register built-in detectors
        detectors.insert("uniprot".to_string(), Box::new(UniProtVersionExtractor));
        detectors.insert("ncbi".to_string(), Box::new(NCBIVersionExtractor));
        detectors.insert("pdb".to_string(), Box::new(PDBVersionExtractor));

        Self { detectors }
    }

    /// Detect version from file content or headers
    pub fn detect_version(
        &self,
        source: &str,
        dataset: &str,
        content: &[u8],
    ) -> Result<DatabaseVersion> {
        let mut version = DatabaseVersion::new(source, dataset);

        // Try to use source-specific detector
        if let Some(detector) = self.detectors.get(source) {
            if let Ok(upstream_version) = detector.extract_version(dataset, content) {
                version.upstream_version = Some(upstream_version.clone());

                // Add upstream version as an alias
                version.aliases.upstream.push(upstream_version.clone());

                // Add metadata based on source
                if source == "uniprot" {
                    version
                        .metadata
                        .insert("release_type".to_string(), "official".to_string());
                } else if source == "ncbi" {
                    version
                        .metadata
                        .insert("release_type".to_string(), "snapshot".to_string());
                }
            }
        }

        Ok(version)
    }

    /// Detect version from a manifest file
    pub fn detect_from_manifest(&self, manifest_path: &str) -> Result<DatabaseVersion> {
        let content = std::fs::read(manifest_path).context("Failed to read manifest file")?;

        let manifest: serde_json::Value =
            serde_json::from_slice(&content).context("Failed to parse manifest JSON")?;

        // Extract source and dataset from manifest or path
        let source = manifest["source"].as_str().unwrap_or("unknown");
        let dataset = manifest["dataset"].as_str().unwrap_or("unknown");

        let mut version = DatabaseVersion::new(source, dataset);

        // Try to extract version information from manifest
        if let Some(v) = manifest["upstream_version"].as_str() {
            version.upstream_version = Some(v.to_string());
            version.aliases.upstream.push(v.to_string());
        } else if let Some(v) = manifest["version"].as_str() {
            // Try to parse as date and convert to upstream format
            if let Some(detector) = self.detectors.get(source) {
                if let Ok(upstream) = detector.parse_version_string(v) {
                    version.upstream_version = Some(upstream.clone());
                    version.aliases.upstream.push(upstream);
                }
            }
        }

        Ok(version)
    }
}

/// Trait for source-specific version extraction
trait VersionExtractor: Send + Sync {
    /// Extract version from file content
    fn extract_version(&self, dataset: &str, content: &[u8]) -> Result<String>;

    /// Parse a version string into upstream format
    fn parse_version_string(&self, version: &str) -> Result<String> {
        Ok(version.to_string())
    }
}

/// UniProt version extractor
struct UniProtVersionExtractor;

impl VersionExtractor for UniProtVersionExtractor {
    fn extract_version(&self, _dataset: &str, content: &[u8]) -> Result<String> {
        // Look for UniProt release pattern in first few KB
        let sample = std::str::from_utf8(&content[..content.len().min(4096)]).unwrap_or("");

        // UniProt includes release info in headers like "Release: 2024_04"
        let re = Regex::new(r"Release:\s*(\d{4}_\d{2})")?;
        if let Some(caps) = re.captures(sample) {
            return Ok(caps[1].to_string());
        }

        // Try to find in description lines
        let re = Regex::new(r"UniProt Release (\d{4}_\d{2})")?;
        if let Some(caps) = re.captures(sample) {
            return Ok(caps[1].to_string());
        }

        anyhow::bail!("Could not detect UniProt version")
    }

    fn parse_version_string(&self, version: &str) -> Result<String> {
        // Convert timestamp to UniProt monthly format
        // e.g., "20250915_053033" -> "2025_09" (monthly releases)
        if version.len() >= 8 {
            let year = &version[0..4];
            let month = &version[4..6];

            // UniProt uses monthly releases with format YYYY_MM
            return Ok(format!("{}_{}", year, month));
        }

        Ok(version.to_string())
    }
}

/// NCBI version extractor
struct NCBIVersionExtractor;

impl VersionExtractor for NCBIVersionExtractor {
    fn extract_version(&self, dataset: &str, content: &[u8]) -> Result<String> {
        // NCBI uses date-based versions
        let sample = std::str::from_utf8(&content[..content.len().min(4096)]).unwrap_or("");

        // Look for date patterns in headers
        let re = Regex::new(r"(\d{4}-\d{2}-\d{2})")?;
        if let Some(caps) = re.captures(sample) {
            return Ok(caps[1].to_string());
        }

        // For taxonomy, look for specific version info
        if dataset == "taxonomy" {
            let re = Regex::new(r"taxdump_(\d{4}-\d{2}-\d{2})")?;
            if let Some(caps) = re.captures(sample) {
                return Ok(caps[1].to_string());
            }
        }

        anyhow::bail!("Could not detect NCBI version")
    }
}

/// PDB version extractor
struct PDBVersionExtractor;

impl VersionExtractor for PDBVersionExtractor {
    fn extract_version(&self, _dataset: &str, content: &[u8]) -> Result<String> {
        // PDB uses weekly snapshots with week numbers
        let sample = std::str::from_utf8(&content[..content.len().min(4096)]).unwrap_or("");

        // Look for week-based version
        let re = Regex::new(r"(\d{4}-W\d{2})")?;
        if let Some(caps) = re.captures(sample) {
            return Ok(caps[1].to_string());
        }

        anyhow::bail!("Could not detect PDB version")
    }
}

/// Version manager for handling version symlinks and aliases
pub struct VersionManager {
    base_path: std::path::PathBuf,
}

impl VersionManager {
    pub fn new(base_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Get the versions directory for a database
    pub fn get_versions_dir(&self, source: &str, dataset: &str) -> std::path::PathBuf {
        self.base_path.join("versions").join(source).join(dataset)
    }

    /// Resolve a version reference to a timestamp
    pub fn resolve_version(&self, source: &str, dataset: &str, reference: &str) -> Result<String> {
        let versions_dir = self.get_versions_dir(source, dataset);

        // If it's a symlink, resolve it
        let reference_path = versions_dir.join(reference);
        if reference_path.is_symlink() {
            let target = std::fs::read_link(&reference_path)?;
            if let Some(name) = target.file_name() {
                return Ok(name.to_string_lossy().to_string());
            }
        }

        // If it's a directory (timestamp), return it directly
        if reference_path.is_dir() {
            return Ok(reference.to_string());
        }

        // Try to find a version that matches this reference
        let versions = self.list_versions(source, dataset)?;
        for version in versions {
            if version.matches(reference) {
                return Ok(version.timestamp);
            }
        }

        anyhow::bail!(
            "Version '{}' not found for {}/{}",
            reference,
            source,
            dataset
        )
    }

    /// Set the current version symlink (expects timestamp)
    #[cfg(unix)]
    pub fn set_current(&self, source: &str, dataset: &str, timestamp: &str) -> Result<()> {
        use std::os::unix::fs;

        let versions_dir = self.get_versions_dir(source, dataset);
        let version_dir = versions_dir.join(timestamp);

        // Verify the version exists
        if !version_dir.exists() {
            anyhow::bail!("Version {} does not exist", timestamp);
        }

        let current_link = versions_dir.join("current");

        // Remove old symlink if it exists
        if current_link.exists() {
            std::fs::remove_file(&current_link)?;
        }

        // Create new symlink to the timestamp directory
        fs::symlink(timestamp, &current_link)
            .context("Failed to create current version symlink")?;

        // Update version metadata to add 'current' to system aliases
        self.update_system_alias(source, dataset, timestamp, "current", true)?;

        Ok(())
    }

    #[cfg(not(unix))]
    pub fn set_current(&self, _source: &str, _dataset: &str, _version: &str) -> Result<()> {
        // On non-Unix systems, store in a file instead
        anyhow::bail!("Symlinks not supported on this platform")
    }

    /// Create an alias for a version (expects timestamp)
    #[cfg(unix)]
    pub fn create_alias(
        &self,
        source: &str,
        dataset: &str,
        timestamp: &str,
        alias: &str,
    ) -> Result<()> {
        use std::os::unix::fs;

        // Check if alias is protected
        if is_protected_alias(alias) {
            anyhow::bail!(
                "Cannot manually create protected alias '{}'. Use appropriate commands.",
                alias
            );
        }

        let versions_dir = self.get_versions_dir(source, dataset);
        let version_dir = versions_dir.join(timestamp);

        // Verify the version exists
        if !version_dir.exists() {
            anyhow::bail!("Version {} does not exist", timestamp);
        }

        let alias_link = versions_dir.join(alias);

        // Remove old symlink if it exists
        if alias_link.exists() {
            std::fs::remove_file(&alias_link)?;
        }

        // Create new symlink to the timestamp directory
        fs::symlink(timestamp, &alias_link).context("Failed to create version alias symlink")?;

        // Update version metadata to add this custom alias
        self.update_custom_alias(source, dataset, timestamp, alias, true)?;

        Ok(())
    }

    /// Remove a custom alias
    #[allow(dead_code)]
    #[cfg(unix)]
    pub fn remove_alias(&self, source: &str, dataset: &str, alias: &str) -> Result<()> {
        // Check if alias is protected
        if is_protected_alias(alias) {
            anyhow::bail!("Cannot remove protected alias '{}'", alias);
        }

        let versions_dir = self.get_versions_dir(source, dataset);
        let alias_link = versions_dir.join(alias);

        if !alias_link.exists() {
            anyhow::bail!("Alias '{}' does not exist", alias);
        }

        // Get the target timestamp before removing
        let target = std::fs::read_link(&alias_link)?;
        let timestamp = target
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid symlink target"))?
            .to_string_lossy();

        // Remove the symlink
        std::fs::remove_file(&alias_link)?;

        // Update version metadata to remove this custom alias
        self.update_custom_alias(source, dataset, &timestamp, alias, false)?;

        Ok(())
    }

    #[cfg(not(unix))]
    pub fn create_alias(
        &self,
        _source: &str,
        _dataset: &str,
        _version: &str,
        _alias: &str,
    ) -> Result<()> {
        anyhow::bail!("Symlinks not supported on this platform")
    }

    /// List all versions for a database with current alias information
    pub fn list_versions(&self, source: &str, dataset: &str) -> Result<Vec<DatabaseVersion>> {
        let versions_dir = self.get_versions_dir(source, dataset);

        if !versions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        let mut symlinks: HashMap<String, String> = HashMap::new();

        // First pass: collect all symlinks
        for entry in std::fs::read_dir(&versions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_symlink() {
                if let Ok(target) = std::fs::read_link(&path) {
                    if let (Some(link_name), Some(target_name)) =
                        (path.file_name(), target.file_name())
                    {
                        let link = link_name.to_string_lossy().to_string();
                        let target = target_name.to_string_lossy().to_string();
                        symlinks.insert(link, target);
                    }
                }
            }
        }

        // Second pass: load versions from timestamp directories
        for entry in std::fs::read_dir(versions_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Skip symlinks, only process directories
            if path.is_symlink() || !path.is_dir() {
                continue;
            }

            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid directory name"))?;

            // Only process timestamp directories (format: YYYYMMDD_HHMMSS)
            if !is_timestamp_format(dir_name) {
                continue;
            }

            // Try to load version info
            let version_file_path = path.join("version.json");
            if let Ok(version_file) = std::fs::read(&version_file_path) {
                if let Ok(mut version) = serde_json::from_slice::<DatabaseVersion>(&version_file) {
                    // Update system aliases based on current symlinks
                    version.aliases.system.clear();
                    for (alias, target) in &symlinks {
                        if target == &version.timestamp && is_system_alias(alias) {
                            version.aliases.system.push(alias.clone());
                        }
                    }
                    versions.push(version);
                }
            }
        }

        // Sort by creation date (newest first)
        versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(versions)
    }

    /// Update system alias in version metadata
    fn update_system_alias(
        &self,
        source: &str,
        dataset: &str,
        timestamp: &str,
        alias: &str,
        add: bool,
    ) -> Result<()> {
        let version_path = self
            .get_versions_dir(source, dataset)
            .join(timestamp)
            .join("version.json");

        if let Ok(content) = std::fs::read(&version_path) {
            if let Ok(mut version) = serde_json::from_slice::<DatabaseVersion>(&content) {
                if add {
                    version.add_system_alias(alias.to_string());
                } else {
                    version.aliases.system.retain(|a| a != alias);
                }

                let json = serde_json::to_string_pretty(&version)?;
                std::fs::write(&version_path, json)?;
            }
        }

        Ok(())
    }

    /// Update custom alias in version metadata
    fn update_custom_alias(
        &self,
        source: &str,
        dataset: &str,
        timestamp: &str,
        alias: &str,
        add: bool,
    ) -> Result<()> {
        let version_path = self
            .get_versions_dir(source, dataset)
            .join(timestamp)
            .join("version.json");

        if let Ok(content) = std::fs::read(&version_path) {
            if let Ok(mut version) = serde_json::from_slice::<DatabaseVersion>(&content) {
                if add {
                    version.add_custom_alias(alias.to_string());
                } else {
                    version.remove_custom_alias(alias);
                }

                let json = serde_json::to_string_pretty(&version)?;
                std::fs::write(&version_path, json)?;
            }
        }

        Ok(())
    }
}

/// Check if a string is in timestamp format (YYYYMMDD_HHMMSS)
pub fn is_timestamp_format(s: &str) -> bool {
    if s.len() != 15 {
        return false;
    }

    let parts: Vec<&str> = s.split('_').collect();
    if parts.len() != 2 {
        return false;
    }

    // Check date part (YYYYMMDD)
    if parts[0].len() != 8 || !parts[0].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    // Check time part (HHMMSS)
    if parts[1].len() != 6 || !parts[1].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    true
}

/// Check if an alias is a protected system alias
fn is_protected_alias(alias: &str) -> bool {
    matches!(alias, "current" | "latest" | "stable")
}

/// Check if an alias is a system alias
fn is_system_alias(alias: &str) -> bool {
    is_protected_alias(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_format() {
        assert!(is_timestamp_format("20250915_053033"));
        assert!(is_timestamp_format("20240101_000000"));
        assert!(!is_timestamp_format("2024_04")); // upstream version
        assert!(!is_timestamp_format("latest"));
        assert!(!is_timestamp_format("20250915053033")); // no underscore
        assert!(!is_timestamp_format("2025-09-15_053033")); // wrong format
    }

    #[test]
    fn test_version_matching() {
        let mut version = DatabaseVersion::new("uniprot", "swissprot");
        version.upstream_version = Some("2024_04".to_string());
        version.aliases.upstream.push("2024_04".to_string());
        version.add_custom_alias("paper-2024".to_string());

        // Should match timestamp
        assert!(version.matches(&version.timestamp));
        // Should match upstream version
        assert!(version.matches("2024_04"));
        // Should match custom alias
        assert!(version.matches("paper-2024"));
        // Should match system alias (latest is added by default)
        assert!(version.matches("latest"));
        // Should not match other versions
        assert!(!version.matches("2024_05"));
        assert!(!version.matches("random"));
    }

    #[test]
    fn test_uniprot_version_extraction() {
        let extractor = UniProtVersionExtractor;
        let content = b"# Release: 2024_04\nSome protein data...";

        let version = extractor.extract_version("swissprot", content).unwrap();
        assert_eq!(version, "2024_04");
    }
}
