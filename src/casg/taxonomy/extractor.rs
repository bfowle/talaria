/// Taxonomy version extraction implementations
///
/// Provides version extractors for NCBI and UniProt taxonomy databases

use anyhow::{Context, Result};
use chrono::{DateTime, Utc, NaiveDate};
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::fs::File;
use std::path::Path;

/// Trait for taxonomy-specific version extraction
pub trait TaxonomyVersionExtractor: Send + Sync {
    /// Extract version information from taxonomy files
    fn extract_version(&self, taxonomy_path: &Path) -> Result<TaxonomyVersionInfo>;

    /// Parse ETags or checksums for change detection
    fn extract_etag(&self, headers: &str) -> Option<String>;

    /// Detect if files have changed since a given version
    fn has_changed_since(&self, current_path: &Path, previous_version: &TaxonomyVersionInfo) -> Result<bool>;
}

/// Taxonomy version information
#[derive(Debug, Clone)]
pub struct TaxonomyVersionInfo {
    pub version: String,
    pub date: DateTime<Utc>,
    pub etag: Option<String>,
    pub checksum: Option<String>,
    pub source_type: String,
}

/// NCBI taxonomy version extractor
pub struct NCBITaxonomyExtractor;

impl TaxonomyVersionExtractor for NCBITaxonomyExtractor {
    fn extract_version(&self, taxonomy_path: &Path) -> Result<TaxonomyVersionInfo> {
        // Check for taxdump.tar.gz or extracted files
        let readme_path = taxonomy_path.join("readme.txt");
        let taxdump_path = taxonomy_path.join("taxdump.tar.gz");

        // Try to extract from readme.txt first
        if readme_path.exists() {
            let file = File::open(&readme_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                // Look for date patterns in readme
                if line.contains("Release") || line.contains("Date") {
                    if let Some(date) = extract_date_from_string(&line) {
                        return Ok(TaxonomyVersionInfo {
                            version: date.format("%Y-%m-%d").to_string(),
                            date: DateTime::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), Utc),
                            etag: None,
                            checksum: compute_file_checksum(&readme_path).ok(),
                            source_type: "ncbi".to_string(),
                        });
                    }
                }
            }
        }

        // Try to get modification time from taxdump file
        if taxdump_path.exists() {
            let metadata = std::fs::metadata(&taxdump_path)?;
            if let Ok(modified) = metadata.modified() {
                let datetime: DateTime<Utc> = modified.into();
                return Ok(TaxonomyVersionInfo {
                    version: datetime.format("%Y-%m-%d").to_string(),
                    date: datetime,
                    etag: None,
                    checksum: compute_file_checksum(&taxdump_path).ok(),
                    source_type: "ncbi".to_string(),
                });
            }
        }

        // Check for nodes.dmp modification time as fallback
        let nodes_path = taxonomy_path.join("nodes.dmp");
        if nodes_path.exists() {
            let metadata = std::fs::metadata(&nodes_path)?;
            if let Ok(modified) = metadata.modified() {
                let datetime: DateTime<Utc> = modified.into();
                return Ok(TaxonomyVersionInfo {
                    version: datetime.format("%Y-%m-%d").to_string(),
                    date: datetime,
                    etag: None,
                    checksum: compute_file_checksum(&nodes_path).ok(),
                    source_type: "ncbi".to_string(),
                });
            }
        }

        anyhow::bail!("Could not extract version from NCBI taxonomy files")
    }

    fn extract_etag(&self, headers: &str) -> Option<String> {
        // Extract ETag from HTTP headers
        for line in headers.lines() {
            if line.starts_with("ETag:") || line.starts_with("etag:") {
                let etag = line.split(':').nth(1)?.trim();
                return Some(etag.trim_matches('"').to_string());
            }
        }
        None
    }

    fn has_changed_since(&self, current_path: &Path, previous_version: &TaxonomyVersionInfo) -> Result<bool> {
        // Compare checksums if available
        if let Some(prev_checksum) = &previous_version.checksum {
            let nodes_path = current_path.join("nodes.dmp");
            if nodes_path.exists() {
                let current_checksum = compute_file_checksum(&nodes_path)?;
                return Ok(&current_checksum != prev_checksum);
            }
        }

        // Compare modification dates
        let current_version = self.extract_version(current_path)?;
        Ok(current_version.date > previous_version.date)
    }
}

/// UniProt taxonomy version extractor
pub struct UniProtTaxonomyExtractor;

impl TaxonomyVersionExtractor for UniProtTaxonomyExtractor {
    fn extract_version(&self, taxonomy_path: &Path) -> Result<TaxonomyVersionInfo> {
        // Look for release notes or version info
        let release_path = taxonomy_path.join("releasenotes.txt");
        let _readme_path = taxonomy_path.join("README");

        // Try release notes first
        if release_path.exists() {
            let file = File::open(&release_path)?;
            let reader = BufReader::new(file);

            for line in reader.lines() {
                let line = line?;
                // Look for UniProt release pattern
                if let Some(version) = extract_uniprot_release(&line) {
                    let date = parse_uniprot_release_date(&version)?;
                    return Ok(TaxonomyVersionInfo {
                        version: version.clone(),
                        date,
                        etag: None,
                        checksum: compute_file_checksum(&release_path).ok(),
                        source_type: "uniprot".to_string(),
                    });
                }
            }
        }

        // Check idmapping.dat.gz modification time
        let idmapping_path = taxonomy_path.join("idmapping.dat.gz");
        if idmapping_path.exists() {
            let metadata = std::fs::metadata(&idmapping_path)?;
            if let Ok(modified) = metadata.modified() {
                let datetime: DateTime<Utc> = modified.into();
                // UniProt releases are monthly, format as YYYY_MM
                let version = datetime.format("%Y_%m").to_string();
                return Ok(TaxonomyVersionInfo {
                    version,
                    date: datetime,
                    etag: None,
                    checksum: None, // Don't compute checksum for large file
                    source_type: "uniprot".to_string(),
                });
            }
        }

        anyhow::bail!("Could not extract version from UniProt taxonomy files")
    }

    fn extract_etag(&self, headers: &str) -> Option<String> {
        // UniProt may use different header names
        for line in headers.lines() {
            if line.starts_with("ETag:") || line.starts_with("Last-Modified:") {
                let value = line.split(':').nth(1)?.trim();
                return Some(value.to_string());
            }
        }
        None
    }

    fn has_changed_since(&self, current_path: &Path, previous_version: &TaxonomyVersionInfo) -> Result<bool> {
        let current_version = self.extract_version(current_path)?;

        // Compare versions directly for UniProt (YYYY_MM format)
        if current_version.version != previous_version.version {
            return Ok(true);
        }

        // Fall back to date comparison
        Ok(current_version.date > previous_version.date)
    }
}

/// Custom taxonomy version extractor (for user-provided taxonomies)
pub struct CustomTaxonomyExtractor {
    pub name: String,
}

impl TaxonomyVersionExtractor for CustomTaxonomyExtractor {
    fn extract_version(&self, taxonomy_path: &Path) -> Result<TaxonomyVersionInfo> {
        // Look for VERSION file or use modification time
        let version_file = taxonomy_path.join("VERSION");

        if version_file.exists() {
            let version = std::fs::read_to_string(&version_file)?
                .trim()
                .to_string();

            // Try to parse date from version
            let date = if let Some(parsed_date) = extract_date_from_string(&version) {
                DateTime::from_naive_utc_and_offset(parsed_date.and_hms_opt(0, 0, 0).unwrap(), Utc)
            } else {
                // Use file modification time
                let metadata = std::fs::metadata(&version_file)?;
                metadata.modified()?.into()
            };

            return Ok(TaxonomyVersionInfo {
                version,
                date,
                etag: None,
                checksum: None,
                source_type: format!("custom/{}", self.name),
            });
        }

        // Fall back to using directory modification time
        let metadata = std::fs::metadata(taxonomy_path)?;
        let datetime: DateTime<Utc> = metadata.modified()?.into();

        Ok(TaxonomyVersionInfo {
            version: datetime.format("%Y%m%d_%H%M%S").to_string(),
            date: datetime,
            etag: None,
            checksum: None,
            source_type: format!("custom/{}", self.name),
        })
    }

    fn extract_etag(&self, headers: &str) -> Option<String> {
        // Custom sources may not have ETags
        for line in headers.lines() {
            if line.starts_with("ETag:") {
                let etag = line.split(':').nth(1)?.trim();
                return Some(etag.trim_matches('"').to_string());
            }
        }
        None
    }

    fn has_changed_since(&self, current_path: &Path, previous_version: &TaxonomyVersionInfo) -> Result<bool> {
        let current_version = self.extract_version(current_path)?;
        Ok(current_version.date > previous_version.date)
    }
}

/// Helper function to extract date from a string
fn extract_date_from_string(s: &str) -> Option<NaiveDate> {
    // Try various date formats
    // Try YYYY-MM-DD format
    if let Ok(re) = Regex::new(r"(\d{4})-(\d{2})-(\d{2})") {
        if let Some(caps) = re.captures(s) {
            let year = caps[1].parse().ok()?;
            let month = caps[2].parse().ok()?;
            let day = caps[3].parse().ok()?;
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                return Some(date);
            }
        }
    }

    // Try MM/DD/YYYY format (US date format)
    if let Ok(re) = Regex::new(r"(\d{2})/(\d{2})/(\d{4})") {
        if let Some(caps) = re.captures(s) {
            let month = caps[1].parse().ok()?;
            let day = caps[2].parse().ok()?;
            let year = caps[3].parse().ok()?;
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                return Some(date);
            }
        }
    }

    // Try YYYYMMDD format
    if let Ok(re) = Regex::new(r"(\d{4})(\d{2})(\d{2})") {
        if let Some(caps) = re.captures(s) {
            let year = caps[1].parse().ok()?;
            let month = caps[2].parse().ok()?;
            let day = caps[3].parse().ok()?;
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                return Some(date);
            }
        }
    }

    None
}

/// Extract UniProt release version from string
fn extract_uniprot_release(s: &str) -> Option<String> {
    // UniProt uses format like "Release 2024_04"
    let re = Regex::new(r"Release\s+(\d{4}_\d{2})").ok()?;
    re.captures(s).map(|caps| caps[1].to_string())
}

/// Parse UniProt release date from version string
fn parse_uniprot_release_date(version: &str) -> Result<DateTime<Utc>> {
    // Format: YYYY_MM
    let parts: Vec<&str> = version.split('_').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid UniProt version format: {}", version);
    }

    let year: i32 = parts[0].parse()
        .context("Failed to parse year")?;
    let month: u32 = parts[1].parse()
        .context("Failed to parse month")?;

    // UniProt releases are typically on the first Wednesday of the month
    let date = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;

    Ok(DateTime::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), Utc))
}

/// Compute SHA256 checksum of a file
fn compute_file_checksum(path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Factory for creating taxonomy extractors
pub fn create_taxonomy_extractor(source: &str) -> Box<dyn TaxonomyVersionExtractor> {
    match source.to_lowercase().as_str() {
        "ncbi" => Box::new(NCBITaxonomyExtractor),
        "uniprot" => Box::new(UniProtTaxonomyExtractor),
        _ => Box::new(CustomTaxonomyExtractor {
            name: source.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::io::Write;

    #[test]
    fn test_ncbi_version_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let taxonomy_dir = temp_dir.path();

        // Create a mock readme.txt
        let readme_path = taxonomy_dir.join("readme.txt");
        let mut file = File::create(&readme_path).unwrap();
        writeln!(file, "NCBI Taxonomy Database").unwrap();
        writeln!(file, "Release Date: 2024-03-15").unwrap();

        let extractor = NCBITaxonomyExtractor;
        let version_info = extractor.extract_version(taxonomy_dir).unwrap();

        assert_eq!(version_info.version, "2024-03-15");
        assert_eq!(version_info.source_type, "ncbi");
    }

    #[test]
    fn test_uniprot_version_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let taxonomy_dir = temp_dir.path();

        // Create a mock release notes
        let release_path = taxonomy_dir.join("releasenotes.txt");
        let mut file = File::create(&release_path).unwrap();
        writeln!(file, "UniProt Knowledgebase").unwrap();
        writeln!(file, "Release 2024_04").unwrap();

        let extractor = UniProtTaxonomyExtractor;
        let version_info = extractor.extract_version(taxonomy_dir).unwrap();

        assert_eq!(version_info.version, "2024_04");
        assert_eq!(version_info.source_type, "uniprot");
    }

    #[test]
    fn test_custom_version_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let taxonomy_dir = temp_dir.path();

        // Create a VERSION file
        let version_file = taxonomy_dir.join("VERSION");
        std::fs::write(&version_file, "v1.2.3-custom").unwrap();

        let extractor = CustomTaxonomyExtractor {
            name: "mydb".to_string(),
        };
        let version_info = extractor.extract_version(taxonomy_dir).unwrap();

        assert_eq!(version_info.version, "v1.2.3-custom");
        assert_eq!(version_info.source_type, "custom/mydb");
    }

    #[test]
    fn test_date_extraction() {
        assert!(extract_date_from_string("2024-03-15").is_some());
        assert!(extract_date_from_string("03/15/2024").is_some());
        assert!(extract_date_from_string("20240315").is_some());
        assert!(extract_date_from_string("invalid").is_none());
    }

    #[test]
    fn test_uniprot_release_parsing() {
        assert_eq!(
            extract_uniprot_release("Release 2024_04"),
            Some("2024_04".to_string())
        );
        assert!(extract_uniprot_release("No release here").is_none());
    }
}