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
        use flate2::read::GzDecoder;
        use std::fs;
        use tar::Archive;

        println!("Downloading NCBI taxonomy database...");

        // Create directory if needed
        fs::create_dir_all(&self.taxonomy_dir)?;

        // Download from NCBI FTP
        let taxdump_url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";
        let taxdump_file = self.taxonomy_dir.join("taxdump.tar.gz");

        // Download the file synchronously using blocking client
        let runtime = tokio::runtime::Runtime::new()?;
        let bytes = runtime.block_on(async {
            let response = reqwest::get(taxdump_url).await?;
            response.bytes().await
        })?;

        println!(
            "  Writing taxonomy archive ({:.2} MB)...",
            bytes.len() as f64 / 1_048_576.0
        );
        fs::write(&taxdump_file, bytes)?;

        println!("  Extracting taxonomy files...");

        // Extract the tar.gz file
        let tar_gz = fs::File::open(&taxdump_file)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(&self.taxonomy_dir)?;

        // Clean up tar file
        fs::remove_file(taxdump_file).ok();

        println!("  ✓ Taxonomy database downloaded and extracted");

        Ok(())
    }
}
