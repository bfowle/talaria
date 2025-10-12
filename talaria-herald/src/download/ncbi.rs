use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use super::progress::DownloadProgress;

pub struct NCBIDownloader {
    client: Client,
    base_url: String,
}

impl Default for NCBIDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl NCBIDownloader {
    pub fn new() -> Self {
        NCBIDownloader {
            client: Client::builder()
                .user_agent("Talaria/0.1.0")
                // Increased timeout for very large files (2 hours)
                // Some files like idmapping.dat.gz are over 20GB
                .timeout(std::time::Duration::from_secs(7200))
                // Connection timeout - time to establish connection
                .connect_timeout(std::time::Duration::from_secs(60))
                // Enable TCP keepalive to prevent connection drops
                .tcp_keepalive(std::time::Duration::from_secs(60))
                // Connection pooling for better stability
                .pool_idle_timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap(),
            base_url: "https://ftp.ncbi.nlm.nih.gov".to_string(),
        }
    }

    pub async fn download_nr(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!("{}/blast/db/FASTA/nr.gz", self.base_url);
        self.download_and_extract(&url, output_path, progress).await
    }

    pub async fn download_nt(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!("{}/blast/db/FASTA/nt.gz", self.base_url);
        self.download_and_extract(&url, output_path, progress).await
    }

    pub async fn download_refseq_protein(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/refseq/release/complete/complete.protein.faa.gz",
            self.base_url
        );
        self.download_and_extract(&url, output_path, progress).await
    }

    pub async fn download_refseq_genomic(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/refseq/release/complete/complete.genomic.fna.gz",
            self.base_url
        );
        self.download_and_extract(&url, output_path, progress).await
    }

    pub async fn download_taxonomy(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!("{}/pub/taxonomy/taxdump.tar.gz", self.base_url);

        progress.set_message("Downloading NCBI taxonomy database...");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to start taxonomy download")?;

        let total_size = response.content_length().unwrap_or(0);
        progress.set_total(total_size as usize);

        // Download to the specified output path (not extracting yet)
        let mut file = File::create(output_path).context("Failed to create output file")?;

        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read chunk")?;
            file.write_all(&chunk).context("Failed to write chunk")?;

            downloaded += chunk.len() as u64;
            progress.set_current(downloaded as usize);
        }

        progress.set_message("Taxonomy download complete!");
        progress.finish();

        // Don't extract here - let store_taxonomy_mapping_file handle extraction
        // to the proper versioned directory
        Ok(())
    }

    pub async fn download_prot_accession2taxid(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/pub/taxonomy/accession2taxid/prot.accession2taxid.gz",
            self.base_url
        );
        // Keep compressed - these files are huge
        self.download_compressed(&url, output_path, progress).await
    }

    pub async fn download_nucl_accession2taxid(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/pub/taxonomy/accession2taxid/nucl_gb.accession2taxid.gz",
            self.base_url
        );
        // Keep compressed - these files are huge
        self.download_compressed(&url, output_path, progress).await
    }

    /// Download a compressed file without extracting it, with resume support
    pub async fn download_compressed(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        self.download_compressed_with_resume(url, output_path, progress, true)
            .await
    }

    pub async fn download_compressed_with_resume(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        progress.set_message(&format!("Downloading from {}", url));

        // Append .tmp extension for temporary file (don't replace existing extension)
        let temp_path = PathBuf::from(format!("{}.tmp", output_path.display()));

        // Check if we can resume
        let mut resume_from = 0u64;
        if resume && temp_path.exists() {
            resume_from = std::fs::metadata(&temp_path)?.len();
            progress.set_message(&format!("Resuming download from {} bytes", resume_from));
        }

        // Build request with range header for resume
        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let response = request.send().await.context("Failed to start download")?;

        // Check if server supports resume
        let supports_resume = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if resume_from > 0 && !supports_resume {
            progress.set_message("Server doesn't support resume, starting from beginning");
            resume_from = 0;
            std::fs::remove_file(&temp_path).ok();
        }

        let total_size = response.content_length().unwrap_or(0) + resume_from;

        progress.set_total(total_size as usize);
        progress.set_current(resume_from as usize);

        let mut file = if resume_from > 0 && supports_resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .context("Failed to open temporary file for resume")?
        } else {
            File::create(&temp_path).context("Failed to create temporary file")?
        };

        // Initialize downloaded to resume_from to track total bytes correctly
        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        let mut consecutive_errors = 0;
        const MAX_ERRORS: u32 = 3;

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Reset error counter on successful chunk
                    consecutive_errors = 0;

                    file.write_all(&chunk).context("Failed to write chunk")?;
                    downloaded += chunk.len() as u64;
                    progress.set_current(downloaded as usize);
                }
                Err(e) => {
                    consecutive_errors += 1;
                    let error_msg = format!(
                        "Failed to read chunk at {}% ({} MB downloaded): {}",
                        (downloaded * 100) / total_size.max(1),
                        downloaded / (1024 * 1024),
                        e
                    );

                    if consecutive_errors >= MAX_ERRORS {
                        // Save progress by keeping the temp file
                        file.sync_all().ok();
                        return Err(anyhow::anyhow!(
                            "Download interrupted after {} consecutive errors at {} bytes.\nThe download can be resumed by running the command again.",
                            MAX_ERRORS,
                            downloaded
                        ));
                    }

                    progress.set_message(&format!("Warning: {}", error_msg));
                    progress.set_message(&format!(
                        "Attempting to continue download (error {}/{})",
                        consecutive_errors, MAX_ERRORS
                    ));

                    // Brief pause before continuing
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        // Final flush before validation
        file.flush()?;
        drop(file); // Close the file handle before moving

        // Validate that we downloaded the complete file
        if total_size > 0 && downloaded < total_size {
            let missing_bytes = total_size - downloaded;
            tracing::error!(
                "Incomplete download: got {} bytes, expected {} bytes ({} bytes missing)",
                downloaded,
                total_size,
                missing_bytes
            );
            return Err(anyhow::anyhow!(
                "Incomplete download: got {} bytes, expected {} bytes. Missing {} bytes. \n\
                 The download can be resumed by running the command again with --resume.",
                downloaded,
                total_size,
                missing_bytes
            ));
        }

        tracing::info!("Download complete ({} bytes), moving to final location", downloaded);

        // Move to final location
        std::fs::rename(&temp_path, output_path)
            .context("Failed to move file to final location")?;

        // Validate gzip file integrity
        if output_path.extension().and_then(|s| s.to_str()) == Some("gz") {
            progress.set_message("Validating gzip file integrity...");
            if let Err(e) = Self::validate_gzip_file(output_path) {
                tracing::error!("Gzip validation failed: {}", e);
                return Err(anyhow::anyhow!(
                    "Downloaded file appears to be corrupted: {}. \n\
                     Please delete {} and try again.",
                    e,
                    output_path.display()
                ));
            }
            tracing::info!("Gzip file validation passed");
        }

        progress.set_message("Download complete!");
        progress.finish();

        Ok(())
    }

    async fn download_and_extract(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        self.download_and_extract_with_resume(url, output_path, progress, false)
            .await
    }

    pub async fn download_and_extract_with_resume(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        progress.set_message(&format!("Downloading from {}", url));

        let temp_path = output_path.with_extension("gz.tmp");

        // Check if we can resume
        let mut resume_from = 0u64;
        if resume && temp_path.exists() {
            resume_from = std::fs::metadata(&temp_path)?.len();
            progress.set_message(&format!("Resuming download from {} bytes", resume_from));
        }

        // Build request with range header for resume
        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let response = request.send().await.context("Failed to start download")?;

        // Check if server supports resume
        let supports_resume = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if resume_from > 0 && !supports_resume {
            progress.set_message("Server doesn't support resume, starting from beginning");
            resume_from = 0;
            std::fs::remove_file(&temp_path).ok();
        }

        let total_size = response.content_length().unwrap_or(0) + resume_from;

        progress.set_total(total_size as usize);
        progress.set_current(resume_from as usize);

        let mut file = if resume_from > 0 && supports_resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .context("Failed to open temporary file for resume")?
        } else {
            File::create(&temp_path).context("Failed to create temporary file")?
        };

        // Initialize downloaded to resume_from to track total bytes correctly
        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read chunk")?;
            file.write_all(&chunk).context("Failed to write chunk")?;

            downloaded += chunk.len() as u64;
            progress.set_current(downloaded as usize);
        }

        // Final flush before decompression
        file.flush()?;
        drop(file); // Close the file handle before decompression

        // Validate that we downloaded the complete file
        if total_size > 0 && downloaded < total_size {
            let missing_bytes = total_size - downloaded;
            tracing::error!(
                "Incomplete download: got {} bytes, expected {} bytes ({} bytes missing)",
                downloaded,
                total_size,
                missing_bytes
            );
            return Err(anyhow::anyhow!(
                "Incomplete download: got {} bytes, expected {} bytes. Missing {} bytes. \n\
                 The download can be resumed by running the command again with --resume.",
                downloaded,
                total_size,
                missing_bytes
            ));
        }

        tracing::info!("Download complete ({} bytes), starting decompression", downloaded);

        progress.set_message("Decompressing file...");

        // Decompress
        let gz_file = File::open(&temp_path).context("Failed to open compressed file")?;
        let mut decoder = GzDecoder::new(BufReader::new(gz_file));
        let mut output_file = File::create(output_path).context("Failed to create output file")?;

        io::copy(&mut decoder, &mut output_file).context("Failed to decompress file")?;

        // IMPORTANT: Do NOT delete temp file here - cleanup happens after processing succeeds
        // This allows retry without re-downloading if chunking/processing fails
        // The download manager will handle cleanup after all operations complete
        // std::fs::remove_file(&temp_path).context("Failed to remove temporary file")?;

        progress.set_message("Download complete!");
        progress.finish();

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn get_database_info(&self, db: &NCBIDatabase) -> Result<String> {
        let info = match db {
            NCBIDatabase::NR => {
                "NCBI NR (Non-Redundant) Protein Database\n\
                 Contains non-redundant sequences from GenBank translations, \
                 PDB, SwissProt, PIR, and PRF"
            }
            NCBIDatabase::NT => {
                "NCBI NT Nucleotide Database\n\
                 Contains nucleotide sequences from GenBank, EMBL, and DDBJ"
            }
            NCBIDatabase::RefSeqProtein => {
                "NCBI RefSeq Protein Database\n\
                 Curated non-redundant protein sequences"
            }
            NCBIDatabase::RefSeqGenomic => {
                "NCBI RefSeq Genomic Database\n\
                 Complete genomic sequences"
            }
            NCBIDatabase::Taxonomy => {
                "NCBI Taxonomy Database\n\
                 Taxonomic classification and nomenclature for all organisms"
            }
            NCBIDatabase::ProtAccession2TaxId => {
                "NCBI Protein Accession to TaxID Mapping\n\
                 Maps protein accessions to their taxonomic identifiers"
            }
            NCBIDatabase::NuclAccession2TaxId => {
                "NCBI Nucleotide Accession to TaxID Mapping\n\
                 Maps nucleotide accessions to their taxonomic identifiers"
            }
            NCBIDatabase::RefSeq => {
                "NCBI RefSeq Database\n\
                 Curated non-redundant sequence database of genomic, transcript, and protein sequences"
            }
            NCBIDatabase::GenBank => {
                "NCBI GenBank Database\n\
                 Comprehensive database of publicly available nucleotide sequences"
            }
        };

        Ok(info.to_string())
    }

    /// Quick validation that a .gz file has valid gzip headers and structure
    /// This doesn't decompress the entire file, just validates the headers
    pub fn validate_gzip_file(file_path: &Path) -> Result<()> {
        let file = File::open(file_path)
            .context("Failed to open file for gzip validation")?;

        // Try to read just the gzip header
        let mut decoder = GzDecoder::new(BufReader::new(file));
        let mut buffer = [0u8; 1024];

        // Try to read first chunk - this will validate the header
        match decoder.read(&mut buffer) {
            Ok(0) => {
                // Empty file
                Err(anyhow::anyhow!("Gzip file is empty"))
            }
            Ok(_) => {
                // Successfully read header and some data
                Ok(())
            }
            Err(e) => {
                // Failed to read - likely corrupted header
                Err(anyhow::anyhow!("Invalid gzip file: {}", e))
            }
        }
    }
}

// Import NCBIDatabase from talaria-core
pub use talaria_core::NCBIDatabase;

// Extension trait for NCBIDatabase with CLI-specific methods
#[allow(dead_code)]
pub trait NCBIDatabaseExt {
    fn description(&self) -> &str;
    fn typical_size(&self) -> &str;
}

impl NCBIDatabaseExt for NCBIDatabase {
    #[allow(dead_code)]
    fn description(&self) -> &str {
        match self {
            NCBIDatabase::NR => "Non-redundant protein sequences",
            NCBIDatabase::NT => "Nucleotide sequences from multiple sources",
            NCBIDatabase::RefSeq => "RefSeq curated sequences",
            NCBIDatabase::RefSeqProtein => "Curated protein sequences",
            NCBIDatabase::RefSeqGenomic => "Complete genomic sequences",
            NCBIDatabase::Taxonomy => "Taxonomic classification database",
            NCBIDatabase::ProtAccession2TaxId => "Protein accession to taxonomy ID mapping",
            NCBIDatabase::NuclAccession2TaxId => "Nucleotide accession to taxonomy ID mapping",
            NCBIDatabase::GenBank => "GenBank sequence database",
        }
    }

    #[allow(dead_code)]
    fn typical_size(&self) -> &str {
        match self {
            NCBIDatabase::NR => "~90 GB compressed",
            NCBIDatabase::NT => "~70 GB compressed",
            NCBIDatabase::RefSeq => "~180 GB compressed",
            NCBIDatabase::RefSeqProtein => "~30 GB compressed",
            NCBIDatabase::RefSeqGenomic => "~150 GB compressed",
            NCBIDatabase::Taxonomy => "~50 MB compressed",
            NCBIDatabase::ProtAccession2TaxId => "~15 GB compressed",
            NCBIDatabase::NuclAccession2TaxId => "~8 GB compressed",
            NCBIDatabase::GenBank => "~250 GB compressed",
        }
    }
}
