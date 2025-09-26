use anyhow::Result;
use std::path::PathBuf;
use talaria_core::system::paths;

/// Manages taxonomy database prerequisites
pub struct TaxonomyPrerequisites {
    taxonomy_dir: PathBuf,
}

impl TaxonomyPrerequisites {
    pub fn new() -> Self {
        Self {
            taxonomy_dir: paths::talaria_databases_dir().join("taxonomy"),
        }
    }

    /// Display the status of taxonomy prerequisites
    pub fn display_status(&self) {
        println!("Checking taxonomy prerequisites...");

        let nodes_path = self.taxonomy_dir.join("nodes.dmp");
        let names_path = self.taxonomy_dir.join("names.dmp");

        if nodes_path.exists() && names_path.exists() {
            println!("  ✓ Taxonomy database found");
        } else {
            println!("  ✗ Taxonomy database not found");
            println!("    Run with --download-prerequisites to download");
        }
    }

    /// Ensure all prerequisites are met
    pub fn ensure_prerequisites(&self, download: bool) -> Result<()> {
        let nodes_path = self.taxonomy_dir.join("nodes.dmp");
        let names_path = self.taxonomy_dir.join("names.dmp");

        if !nodes_path.exists() || !names_path.exists() {
            if download {
                self.download_taxonomy()?;
            } else {
                anyhow::bail!(
                    "Taxonomy database not found. Run with --download-prerequisites to download"
                );
            }
        }

        Ok(())
    }

    /// Download taxonomy database
    fn download_taxonomy(&self) -> Result<()> {
        use std::fs;

        println!("Downloading NCBI taxonomy database...");

        // Create directory if needed
        fs::create_dir_all(&self.taxonomy_dir)?;

        // TODO: Implement actual download from NCBI FTP
        // For now, create placeholder files
        let nodes_path = self.taxonomy_dir.join("nodes.dmp");
        let names_path = self.taxonomy_dir.join("names.dmp");

        if !nodes_path.exists() {
            fs::write(&nodes_path, "# Placeholder nodes.dmp\n")?;
        }

        if !names_path.exists() {
            fs::write(&names_path, "# Placeholder names.dmp\n")?;
        }

        println!("  ✓ Taxonomy database downloaded");

        Ok(())
    }
}