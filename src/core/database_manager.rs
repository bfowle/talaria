use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Read};
use sha2::{Sha256, Digest};

/// Manages the centralized database directory structure
pub struct DatabaseManager {
    base_dir: PathBuf,
    retention_count: usize,
}

impl DatabaseManager {
    /// Create a new DatabaseManager with the specified base directory
    pub fn new(base_dir: Option<String>) -> Result<Self> {
        let base = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Default to ~/.talaria/databases/data/
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
            home.join(".talaria").join("databases").join("data")
        };
        
        // Ensure the directory exists
        fs::create_dir_all(&base)
            .context("Failed to create database directory")?;
        
        Ok(Self {
            base_dir: base,
            retention_count: 3,
        })
    }
    
    pub fn with_retention(mut self, count: usize) -> Self {
        self.retention_count = count;
        self
    }
    
    /// Get the base database directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
    
    /// Get the metadata directory
    pub fn metadata_dir(&self) -> PathBuf {
        self.base_dir.parent()
            .map(|p| p.join("metadata"))
            .unwrap_or_else(|| self.base_dir.join("metadata"))
    }
    
    /// Get the directory for a specific database and dataset
    pub fn get_database_dir(&self, source: &str, dataset: &str) -> PathBuf {
        self.base_dir.join(source).join(dataset)
    }

    /// Check if a database is a custom (user-added) database
    pub fn is_custom_database(&self, source: &str) -> bool {
        source == "custom" || !matches!(source, "uniprot" | "ncbi" | "pdb" | "refseq")
    }
    
    /// Get the directory for a specific version
    pub fn get_version_dir(&self, source: &str, dataset: &str, date: &str) -> PathBuf {
        self.get_database_dir(source, dataset).join(date)
    }
    
    /// Get the current version symlink path
    pub fn get_current_link(&self, source: &str, dataset: &str) -> PathBuf {
        self.get_database_dir(source, dataset).join("current")
    }
    
    /// Download a database to the versioned directory structure with idempotency
    pub fn prepare_download(
        &self,
        source: &str,
        dataset: &str,
    ) -> Result<(PathBuf, String)> {
        let date = Local::now().format("%Y-%m-%d").to_string();
        let version_dir = self.get_version_dir(source, dataset, &date);
        let temp_dir = self.get_database_dir(source, dataset).join(format!(".tmp_{}", date));

        // Clean up any incomplete downloads first
        self.cleanup_temp_downloads(source, dataset)?;

        // Check if this version already exists and is complete
        if version_dir.exists() {
            if self.verify_download_complete(&version_dir)? {
                println!("✅ Database version {} already downloaded and verified", date);
                return Ok((version_dir, date));
            } else {
                println!("⚠ Incomplete download detected for {}, removing...", date);
                fs::remove_dir_all(&version_dir)?;
            }
        }

        // Create temporary directory for download
        fs::create_dir_all(&temp_dir)
            .context("Failed to create temporary download directory")?;

        Ok((temp_dir, date))
    }

    /// Finalize a download by moving from temp to final location
    pub fn finalize_download(
        &self,
        source: &str,
        dataset: &str,
        version: &str,
    ) -> Result<()> {
        let temp_dir = self.get_database_dir(source, dataset).join(format!(".tmp_{}", version));
        let final_dir = self.get_version_dir(source, dataset, version);

        if !temp_dir.exists() {
            anyhow::bail!("Temporary download directory not found");
        }

        // Verify the download is complete before moving
        if !self.verify_download_complete(&temp_dir)? {
            anyhow::bail!("Download verification failed");
        }

        // Atomic move from temp to final
        fs::rename(&temp_dir, &final_dir)
            .context("Failed to move download to final location")?;

        Ok(())
    }

    /// Clean up temporary download directories
    fn cleanup_temp_downloads(&self, source: &str, dataset: &str) -> Result<()> {
        let db_dir = self.get_database_dir(source, dataset);
        if !db_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&db_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(".tmp_") {
                    println!("Cleaning up incomplete download: {}", name);
                    fs::remove_dir_all(&path).ok();
                }
            }
        }
        Ok(())
    }

    /// Verify that a download is complete
    fn verify_download_complete(&self, dir: &Path) -> Result<bool> {
        // Check if metadata.json exists
        let metadata_path = dir.join("metadata.json");
        if !metadata_path.exists() {
            return Ok(false);
        }

        // Check if at least one data file exists
        let has_data_files = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .any(|entry| {
                let path = entry.path();
                if path.is_file() {
                    // Check for various database file types by extension
                    let ext = path.extension().and_then(|s| s.to_str());
                    // Include .gz files and taxonomy dumps
                    matches!(ext, Some("fasta") | Some("gz") | Some("tar") | Some("dmp") | Some("prt") | Some("dat"))
                } else {
                    false
                }
            });

        Ok(has_data_files)
    }

    /// Calculate checksum of a file
    pub fn calculate_checksum(file_path: &Path) -> io::Result<String> {
        let mut file = fs::File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
    
    /// Update the "current" symlink to point to the latest version
    pub fn update_current_link(&self, source: &str, dataset: &str, version: &str) -> Result<()> {
        let current_link = self.get_current_link(source, dataset);
        let _target_dir = self.get_version_dir(source, dataset, version);
        
        // Remove existing symlink if it exists
        if current_link.exists() || current_link.is_symlink() {
            fs::remove_file(&current_link).ok();
        }
        
        // Create new symlink (use relative path for portability)
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(version, &current_link)
                .context("Failed to create current symlink")?;
        }
        
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_dir;
            symlink_dir(&target_dir, &current_link)
                .context("Failed to create current symlink")?;
        }
        
        Ok(())
    }
    
    /// List all versions of a database
    pub fn list_versions(&self, source: &str, dataset: &str) -> Result<Vec<DatabaseVersion>> {
        let db_dir = self.get_database_dir(source, dataset);
        
        if !db_dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut versions = Vec::new();
        
        for entry in fs::read_dir(&db_dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Skip the "current" symlink
            if path.file_name() == Some(std::ffi::OsStr::new("current")) {
                continue;
            }
            
            if path.is_dir() {
                if let Some(version_name) = path.file_name().and_then(|s| s.to_str()) {
                    // Try to parse as date
                    let date = NaiveDate::parse_from_str(version_name, "%Y-%m-%d").ok();
                    
                    let metadata = fs::metadata(&path)?;
                    let modified: DateTime<Local> = metadata.modified()?.into();
                    
                    // Check if this is the current version
                    let current_link = self.get_current_link(source, dataset);
                    let is_current = if current_link.exists() {
                        fs::read_link(&current_link)
                            .ok()
                            .and_then(|p| p.file_name().map(|n| n == path.file_name().unwrap()))
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    
                    // Calculate directory size
                    let size = calculate_dir_size(&path)?;
                    
                    versions.push(DatabaseVersion {
                        version: version_name.to_string(),
                        path: path.clone(),
                        date,
                        modified,
                        size,
                        is_current,
                    });
                }
            }
        }
        
        // Sort by version (date) descending
        versions.sort_by(|a, b| b.version.cmp(&a.version));
        
        Ok(versions)
    }
    
    /// Clean old versions based on retention policy
    pub fn clean_old_versions(&self, source: &str, dataset: &str) -> Result<Vec<String>> {
        if self.retention_count == 0 {
            // Keep all versions
            return Ok(Vec::new());
        }
        
        let versions = self.list_versions(source, dataset)?;
        let mut removed = Vec::new();
        
        // Keep the current version and the N most recent versions
        let mut keep_count = 0;
        for version in &versions {
            if version.is_current {
                continue; // Always keep current
            }
            
            keep_count += 1;
            if keep_count > self.retention_count {
                // Remove this version
                fs::remove_dir_all(&version.path)
                    .context(format!("Failed to remove old version: {}", version.version))?;
                removed.push(version.version.clone());
            }
        }
        
        Ok(removed)
    }
    
    /// List all available databases
    pub fn list_all_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let mut databases = Vec::new();
        
        if !self.base_dir.exists() {
            return Ok(databases);
        }
        
        // Iterate through source directories (uniprot, ncbi, etc.)
        for source_entry in fs::read_dir(&self.base_dir)? {
            let source_entry = source_entry?;
            let source_path = source_entry.path();
            
            if !source_path.is_dir() {
                continue;
            }
            
            let source_name = source_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            
            // Iterate through dataset directories
            for dataset_entry in fs::read_dir(&source_path)? {
                let dataset_entry = dataset_entry?;
                let dataset_path = dataset_entry.path();
                
                if !dataset_path.is_dir() {
                    continue;
                }
                
                let dataset_name = dataset_path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                
                // Get versions for this database
                let versions = self.list_versions(&source_name, &dataset_name)?;
                
                if !versions.is_empty() {
                    let current_version = versions.iter()
                        .find(|v| v.is_current)
                        .or_else(|| versions.first());
                    
                    if let Some(current) = current_version {
                        // Check for reductions in the current version
                        let mut reductions = Vec::new();
                        let reduced_dir = current.path.join("reduced");
                        if reduced_dir.exists() {
                            for entry in fs::read_dir(&reduced_dir)? {
                                let entry = entry?;
                                let path = entry.path();
                                
                                if path.is_dir() {
                                    let profile = path.file_name()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    
                                    // Read metadata if exists
                                    let metadata_path = path.join("metadata.json");
                                    if metadata_path.exists() {
                                        if let Ok(content) = fs::read_to_string(&metadata_path) {
                                            if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&content) {
                                                let size = metadata["output_size"].as_u64().unwrap_or(0);
                                                let ratio = metadata["reduction_ratio"].as_f64().unwrap_or(0.0);
                                                let sequences = metadata["reference_sequences"].as_u64().unwrap_or(0) as usize;
                                                
                                                reductions.push(ReductionInfo {
                                                    profile,
                                                    path: path.clone(),
                                                    size,
                                                    reduction_ratio: ratio,
                                                    sequences,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        databases.push(DatabaseInfo {
                            source: source_name.clone(),
                            dataset: dataset_name.clone(),
                            current_version: current.version.clone(),
                            current_path: current.path.clone(),
                            size: current.size,
                            modified: current.modified,
                            version_count: versions.len(),
                            reductions,
                        });
                    }
                }
            }
        }
        
        Ok(databases)
    }
    
    /// Parse a database reference (e.g., "uniprot/swissprot", "uniprot/swissprot@2024-01-01")
    pub fn parse_reference(&self, reference: &str) -> Result<DatabaseReference> {
        let parts: Vec<&str> = reference.split('@').collect();
        
        let path_parts: Vec<&str> = parts[0].split('/').collect();
        if path_parts.len() != 2 {
            anyhow::bail!("Invalid database reference format. Expected: source/dataset[@version]");
        }
        
        let source = path_parts[0].to_string();
        let dataset = path_parts[1].to_string();
        
        let version = if parts.len() > 1 {
            Some(parts[1].to_string())
        } else {
            None
        };
        
        Ok(DatabaseReference {
            source,
            dataset,
            version,
        })
    }
    
    /// Find a FASTA file in a directory
    pub fn find_fasta_in_dir(&self, dir: &Path) -> Result<PathBuf> {
        use std::fs;
        
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if matches!(ext, "fasta" | "fa" | "fna" | "faa" | "ffn" | "frn") {
                        return Ok(path);
                    }
                }
            }
        }
        
        anyhow::bail!("No FASTA file found in directory: {}", dir.display())
    }
    
    /// Resolve a database reference to an actual path
    pub fn resolve_reference(&self, reference: &DatabaseReference) -> Result<PathBuf> {
        if let Some(version) = &reference.version {
            if version == "current" {
                // Resolve the current symlink
                let current_link = self.get_current_link(&reference.source, &reference.dataset);
                if current_link.exists() {
                    Ok(current_link)
                } else {
                    anyhow::bail!("No current version for {}/{}", reference.source, reference.dataset);
                }
            } else {
                // Specific version
                let version_dir = self.get_version_dir(&reference.source, &reference.dataset, version);
                if version_dir.exists() {
                    Ok(version_dir)
                } else {
                    anyhow::bail!("Version {} not found for {}/{}", 
                                  version, reference.source, reference.dataset);
                }
            }
        } else {
            // Default to current
            let current_link = self.get_current_link(&reference.source, &reference.dataset);
            if current_link.exists() {
                Ok(current_link)
            } else {
                // If no current link, use the latest version
                let versions = self.list_versions(&reference.source, &reference.dataset)?;
                if let Some(latest) = versions.first() {
                    Ok(latest.path.clone())
                } else {
                    anyhow::bail!("No versions found for {}/{}", 
                                  reference.source, reference.dataset);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseVersion {
    pub version: String,
    pub path: PathBuf,
    pub date: Option<NaiveDate>,
    pub modified: DateTime<Local>,
    pub size: u64,
    pub is_current: bool,
}

#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub source: String,
    pub dataset: String,
    pub current_version: String,
    pub current_path: PathBuf,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub version_count: usize,
    pub reductions: Vec<ReductionInfo>,
}

#[derive(Debug, Clone)]
pub struct ReductionInfo {
    pub profile: String,
    pub path: PathBuf,
    pub size: u64,
    pub reduction_ratio: f64,
    pub sequences: usize,
}

#[derive(Debug, Clone)]
pub struct DatabaseReference {
    pub source: String,
    pub dataset: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseMetadata {
    pub source: String,
    pub dataset: String,
    pub version: String,
    pub download_date: DateTime<Utc>,
    pub file_size: u64,
    pub checksum: Option<String>,
    pub url: Option<String>,
}

impl DatabaseMetadata {
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
    
    pub fn load(path: &Path) -> Result<Self> {
        let json = fs::read_to_string(path)?;
        let metadata = serde_json::from_str(&json)?;
        Ok(metadata)
    }
}

/// Calculate the total size of a directory
fn calculate_dir_size(path: &Path) -> Result<u64> {
    let mut total_size = 0;
    
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        
        if metadata.is_file() {
            total_size += metadata.len();
        } else if metadata.is_dir() {
            total_size += calculate_dir_size(&entry.path())?;
        }
    }
    
    Ok(total_size)
}