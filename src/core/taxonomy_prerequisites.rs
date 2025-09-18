/// Taxonomy prerequisites management
///
/// This module ensures that required taxonomy databases are available
/// for proper sequence annotation and accession-to-taxid mapping.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use crate::core::paths;
use crate::cli::output::*;

/// Taxonomy database files and their descriptions
#[derive(Debug, Clone)]
pub struct TaxonomyFile {
    pub source: &'static str,
    pub name: &'static str,
    pub path: PathBuf,
    pub description: &'static str,
    pub required: bool,
    pub approximate_size: &'static str,
    pub download_url: Option<&'static str>,
}

/// Check and ensure taxonomy prerequisites are available
pub struct TaxonomyPrerequisites {
    taxonomy_dir: PathBuf,
    files: Vec<TaxonomyFile>,
}

impl TaxonomyPrerequisites {
    pub fn new() -> Self {
        // Use unified taxonomy directory
        let taxonomy_dir = paths::talaria_taxonomy_current_dir();
        let tree_dir = taxonomy_dir.join("tree");
        let mappings_dir = taxonomy_dir.join("mappings");

        let files = vec![
            TaxonomyFile {
                source: "ncbi",
                name: "prot.accession2taxid.gz",
                path: mappings_dir.join("prot.accession2taxid.gz"),
                description: "NCBI protein accession to taxonomy ID mapping",
                required: false,  // Recommended but not required
                approximate_size: "~15 GB compressed",
                download_url: Some("https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/prot.accession2taxid.gz"),
            },
            TaxonomyFile {
                source: "ncbi",
                name: "nucl.accession2taxid.gz",
                path: mappings_dir.join("nucl.accession2taxid.gz"),
                description: "NCBI nucleotide accession to taxonomy ID mapping",
                required: false,
                approximate_size: "~8 GB compressed",
                download_url: Some("https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/nucl.accession2taxid.gz"),
            },
            TaxonomyFile {
                source: "ncbi",
                name: "taxdump",
                path: tree_dir,
                description: "NCBI taxonomy database dump",
                required: false,
                approximate_size: "~50 MB compressed",
                download_url: Some("https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz"),
            },
            TaxonomyFile {
                source: "uniprot",
                name: "uniprot_idmapping.dat.gz",
                path: mappings_dir.join("uniprot_idmapping.dat.gz"),
                description: "UniProt accession to taxonomy and other ID mappings",
                required: false,
                approximate_size: "~15 GB compressed",
                download_url: Some("https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/idmapping/idmapping.dat.gz"),
            },
        ];

        TaxonomyPrerequisites {
            taxonomy_dir,
            files,
        }
    }

    /// Check which taxonomy files are present
    pub fn check_status(&self) -> TaxonomyStatus {
        let mut status = TaxonomyStatus {
            has_any: false,
            has_ncbi_protein: false,
            has_ncbi_nucleotide: false,
            has_ncbi_taxdump: false,
            has_uniprot: false,
            missing_files: Vec::new(),
            present_files: Vec::new(),
        };

        for file in &self.files {
            if file.path.exists() {
                status.has_any = true;
                status.present_files.push(file.clone());

                match (file.source, file.name) {
                    ("ncbi", "prot.accession2taxid.gz") => status.has_ncbi_protein = true,
                    ("ncbi", "nucl.accession2taxid.gz") => status.has_ncbi_nucleotide = true,
                    ("ncbi", "taxdump") => status.has_ncbi_taxdump = true,
                    ("uniprot", "uniprot_idmapping.dat.gz") => status.has_uniprot = true,
                    _ => {}
                }
            } else {
                status.missing_files.push(file.clone());
            }
        }

        status
    }

    /// Display status of taxonomy files
    pub fn display_status(&self) {
        let status = self.check_status();

        if status.has_any {
            success("Taxonomy databases found:");
            for file in &status.present_files {
                tree_item(
                    false,
                    &format!("{}/{}", file.source, file.name),
                    Some(file.description),
                );
            }
        }

        if !status.missing_files.is_empty() {
            warning("Missing taxonomy databases (recommended for better annotation):");
            for file in &status.missing_files {
                tree_item(
                    false,
                    &format!("{}/{}", file.source, file.name),
                    Some(&format!("{} ({})", file.description, file.approximate_size)),
                );
            }

            info("To download missing taxonomy databases, run:");
            tree_item(false, "talaria database download ncbi/taxonomy", None);
        }
    }

    /// Ensure required taxonomy files exist (with optional auto-download)
    pub fn ensure_prerequisites(&self, auto_download: bool) -> Result<()> {
        let status = self.check_status();

        // If we have at least one mapping file, we're good
        if status.has_ncbi_protein || status.has_uniprot {
            return Ok(());
        }

        // No mapping files found
        if !auto_download {
            warning("No taxonomy mapping files found. Sequences will use TaxID from headers only.");
            info("For better taxonomy resolution, consider downloading:");
            tree_item(false, "talaria database download ncbi/taxonomy", None);
            return Ok(());
        }

        // Auto-download the smallest useful file (taxdump)
        info("Auto-downloading minimal taxonomy database...");
        self.download_taxdump()?;

        Ok(())
    }

    /// Download NCBI taxdump (small but useful)
    fn download_taxdump(&self) -> Result<()> {
        // For now, just provide instructions
        // TODO: Implement actual download when download infrastructure is stable

        let taxdump = self.files.iter()
            .find(|f| f.name == "taxdump.tar.gz")
            .ok_or_else(|| anyhow::anyhow!("Taxdump configuration not found"))?;

        warning("Auto-download not yet implemented. Please download manually:");
        info(&format!("  wget {}", taxdump.download_url.unwrap_or("URL not available")));
        info(&format!("  mv taxdump.tar.gz {}", taxdump.path.display()));

        anyhow::bail!("Please download taxonomy files manually using the commands above")
    }

    /// Get path to taxonomy directory
    pub fn taxonomy_dir(&self) -> &Path {
        &self.taxonomy_dir
    }

    /// Check if any accession mapping is available
    pub fn has_accession_mapping(&self) -> bool {
        let status = self.check_status();
        status.has_ncbi_protein || status.has_ncbi_nucleotide || status.has_uniprot
    }
}

/// Status of taxonomy prerequisites
#[derive(Debug)]
pub struct TaxonomyStatus {
    pub has_any: bool,
    pub has_ncbi_protein: bool,
    pub has_ncbi_nucleotide: bool,
    pub has_ncbi_taxdump: bool,
    pub has_uniprot: bool,
    pub missing_files: Vec<TaxonomyFile>,
    pub present_files: Vec<TaxonomyFile>,
}

/// Extract NCBI taxdump archive
#[allow(dead_code)]
fn extract_taxdump(archive_path: &Path) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;
    use std::fs::File;

    let file = File::open(archive_path)
        .context("Failed to open taxdump archive")?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let extract_dir = archive_path.parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid archive path"))?;

    archive.unpack(extract_dir)
        .context("Failed to extract taxdump")?;

    Ok(())
}

/// Check if we should show taxonomy warnings
pub fn should_warn_about_taxonomy() -> bool {
    // Check if user has explicitly disabled warnings
    if std::env::var("TALARIA_SKIP_TAXONOMY_WARNING").is_ok() {
        return false;
    }

    // Check if this is a test environment
    if std::env::var("TALARIA_TEST_MODE").is_ok() {
        return false;
    }

    true
}

/// Download specific taxonomy database
pub async fn download_taxonomy_database(database: &str) -> Result<()> {
    use crate::cli::commands::database::download::DownloadArgs;
    use crate::cli::commands::database::download;

    let args = DownloadArgs {
        database: Some(format!("ncbi/{}", database)),
        output: paths::talaria_databases_dir(),
        taxonomy: false,
        resume: true,
        interactive: false,
        skip_verify: false,
        list_datasets: false,
        json: false,
        manifest_server: None,
        talaria_home: None,
        preserve_lambda_on_failure: false,
        dry_run: false,
        force: false,
        taxids: None,
        taxid_list: None,
        reference_proteomes: false,
        max_sequences: None,
        description: None,
    };

    download::run(args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_taxonomy_status_check() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATABASES_DIR", temp_dir.path());

        let prereqs = TaxonomyPrerequisites::new();
        let status = prereqs.check_status();

        // Should have no files initially
        assert!(!status.has_any);
        assert!(!status.has_ncbi_protein);
        assert!(!status.has_uniprot);
        assert_eq!(status.present_files.len(), 0);
        assert_eq!(status.missing_files.len(), 4); // All files missing

        std::env::remove_var("TALARIA_DATABASES_DIR");
    }

    #[test]
    fn test_has_accession_mapping() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("TALARIA_DATABASES_DIR", temp_dir.path());

        // Create unified taxonomy directory structure
        let tax_version_dir = temp_dir.path().join("taxonomy").join("20240101_000000");
        let mappings_dir = tax_version_dir.join("mappings");
        std::fs::create_dir_all(&mappings_dir).unwrap();
        std::fs::write(mappings_dir.join("prot.accession2taxid.gz"), b"dummy").unwrap();

        // Create current symlink
        let current_link = temp_dir.path().join("taxonomy").join("current");
        #[cfg(unix)]
        std::os::unix::fs::symlink("20240101_000000", &current_link).unwrap();
        #[cfg(windows)]
        std::fs::write(&current_link, b"20240101_000000").unwrap();

        let prereqs = TaxonomyPrerequisites::new();
        assert!(prereqs.has_accession_mapping());

        std::env::remove_var("TALARIA_DATABASES_DIR");
    }
}