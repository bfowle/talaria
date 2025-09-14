use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;

use super::progress::DownloadProgress;

pub struct UniProtDownloader {
    client: Client,
    base_url: String,
}

impl Default for UniProtDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl UniProtDownloader {
    pub fn new() -> Self {
        UniProtDownloader {
            client: Client::builder()
                .user_agent("Talaria/0.1.0")
                // Increased timeout for large files like idmapping (30 minutes)
                .timeout(std::time::Duration::from_secs(1800))
                // Add connection timeout separately
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            // Using EBI mirror as it's more reliable for HTTPS access
            base_url: "https://ftp.ebi.ac.uk/pub/databases/uniprot".to_string(),
        }
    }
    
    pub async fn download_swissprot(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        self.download_swissprot_with_options(output_path, progress, false).await
    }
    
    pub async fn download_swissprot_with_options(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
        skip_verify: bool,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz",
            self.base_url
        );
        
        self.download_and_extract_with_verification(&url, output_path, progress, skip_verify).await
    }
    
    pub async fn download_trembl(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz",
            self.base_url
        );
        
        self.download_and_extract(&url, output_path, progress).await
    }
    
    pub async fn download_uniref50(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/uniref/uniref50/uniref50.fasta.gz",
            self.base_url
        );
        
        self.download_and_extract(&url, output_path, progress).await
    }
    
    pub async fn download_uniref90(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/uniref/uniref90/uniref90.fasta.gz",
            self.base_url
        );
        
        self.download_and_extract(&url, output_path, progress).await
    }
    
    pub async fn download_uniref100(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/uniref/uniref100/uniref100.fasta.gz",
            self.base_url
        );

        self.download_and_extract(&url, output_path, progress).await
    }

    pub async fn download_idmapping(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
    ) -> Result<()> {
        self.download_idmapping_with_resume(output_path, progress, true).await
    }

    pub async fn download_idmapping_with_resume(
        &self,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        let url = format!(
            "{}/current_release/knowledgebase/idmapping/idmapping.dat.gz",
            self.base_url
        );

        // Use the compressed download method that supports resume
        self.download_compressed_with_resume(&url, output_path, progress, resume).await
    }

    /// Download a compressed file without extracting it, with resume support
    pub async fn download_compressed_with_resume(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        resume: bool,
    ) -> Result<()> {
        progress.set_message(&format!("Downloading from {}", url));

        // Use .tmp extension for temporary file
        let temp_path = output_path.with_extension("tmp");

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

        let response = request
            .send()
            .await
            .context("Failed to start download")?;

        // Check if server supports resume
        let supports_resume = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if resume_from > 0 && !supports_resume {
            progress.set_message("Server doesn't support resume, starting from beginning");
            resume_from = 0;
            std::fs::remove_file(&temp_path).ok();
        }

        let total_size = response
            .content_length()
            .unwrap_or(0) + resume_from;

        progress.set_total(total_size as usize);
        progress.set_current(resume_from as usize);

        let mut file = if resume_from > 0 && supports_resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .context("Failed to open temporary file for resume")?
        } else {
            File::create(&temp_path)
                .context("Failed to create temporary file")?
        };

        // Initialize downloaded to resume_from to track total bytes correctly
        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read chunk")?;
            file.write_all(&chunk)
                .context("Failed to write chunk")?;

            downloaded += chunk.len() as u64;
            progress.set_current(downloaded as usize);
        }

        // Move to final location
        std::fs::rename(&temp_path, output_path)
            .context("Failed to move file to final location")?;

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
        self.download_and_extract_with_verification(url, output_path, progress, true).await
    }
    
    async fn download_and_extract_with_verification(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        skip_verify: bool,
    ) -> Result<()> {
        self.download_and_extract_with_options(url, output_path, progress, skip_verify, false).await
    }
    
    pub async fn download_and_extract_with_options(
        &self,
        url: &str,
        output_path: &Path,
        progress: &mut DownloadProgress,
        skip_verify: bool,
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
        
        let response = request
            .send()
            .await
            .context("Failed to start download")?;
        
        // Check if server supports resume
        let supports_resume = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        if resume_from > 0 && !supports_resume {
            progress.set_message("Server doesn't support resume, starting from beginning");
            resume_from = 0;
            std::fs::remove_file(&temp_path).ok();
        }
        
        let total_size = response
            .content_length()
            .unwrap_or(0) + resume_from;
        
        progress.set_total(total_size as usize);
        progress.set_current(resume_from as usize);
        
        let mut file = if resume_from > 0 && supports_resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&temp_path)
                .context("Failed to open temporary file for resume")?
        } else {
            File::create(&temp_path)
                .context("Failed to create temporary file")?
        };

        // Initialize downloaded to resume_from to track total bytes correctly
        let mut downloaded = resume_from;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read chunk")?;
            file.write_all(&chunk)
                .context("Failed to write chunk")?;

            downloaded += chunk.len() as u64;
            progress.set_current(downloaded as usize);
        }
        
        progress.set_message("Decompressing file...");
        
        // Decompress the file
        let gz_file = File::open(&temp_path)
            .context("Failed to open compressed file")?;
        let mut decoder = GzDecoder::new(BufReader::new(gz_file));
        let mut output_file = File::create(output_path)
            .context("Failed to create output file")?;
        
        io::copy(&mut decoder, &mut output_file)
            .context("Failed to decompress file")?;
        
        // Clean up temporary file
        std::fs::remove_file(&temp_path)
            .context("Failed to remove temporary file")?;
        
        // Verify checksum if not skipped
        if !skip_verify {
            progress.set_message("Verifying checksum...");
            
            // Try to download checksum file
            let checksum_url = format!("{}.md5", url);
            if let Ok(checksum_response) = self.client.get(&checksum_url).send().await {
                if checksum_response.status().is_success() {
                    let checksum_text = checksum_response.text().await?;
                    // Parse checksum (usually in format: "checksum  filename")
                    if let Some(expected_checksum) = checksum_text.split_whitespace().next() {
                        if !self.verify_checksum(output_path, expected_checksum).await? {
                            // Delete the file if checksum doesn't match
                            std::fs::remove_file(output_path).ok();
                            return Err(anyhow::anyhow!("Checksum verification failed"));
                        }
                        progress.set_message("Checksum verified!");
                    }
                }
            }
        }
        
        progress.set_message("Download complete!");
        progress.finish();
        
        Ok(())
    }
    
    pub async fn verify_checksum(
        &self,
        file_path: &Path,
        expected_checksum: &str,
    ) -> Result<bool> {
        let mut file = File::open(file_path)
            .context("Failed to open file for checksum")?;
        
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];
        
        loop {
            let bytes_read = file.read(&mut buffer)
                .context("Failed to read file for checksum")?;
            
            if bytes_read == 0 {
                break;
            }
            
            hasher.update(&buffer[..bytes_read]);
        }
        
        let result = hasher.finalize();
        let calculated = format!("{:x}", result);
        
        Ok(calculated == expected_checksum)
    }
    
    pub async fn get_latest_release_info(&self) -> Result<String> {
        let url = format!("{}/current_release/relnotes.txt", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch release notes")?;
        
        let text = response.text()
            .await
            .context("Failed to read release notes")?;
        
        // Extract first few lines with release info
        let lines: Vec<&str> = text.lines().take(10).collect();
        Ok(lines.join("\n"))
    }
}

#[derive(Debug, Clone)]
pub enum UniProtDatabase {
    SwissProt,
    TrEMBL,
    UniRef50,
    UniRef90,
    UniRef100,
    IdMapping,
}

impl std::fmt::Display for UniProtDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UniProtDatabase::SwissProt => write!(f, "SwissProt"),
            UniProtDatabase::TrEMBL => write!(f, "TrEMBL"),
            UniProtDatabase::UniRef50 => write!(f, "UniRef50"),
            UniProtDatabase::UniRef90 => write!(f, "UniRef90"),
            UniProtDatabase::UniRef100 => write!(f, "UniRef100"),
            UniProtDatabase::IdMapping => write!(f, "IdMapping"),
        }
    }
}

impl UniProtDatabase {
    pub fn description(&self) -> &str {
        match self {
            UniProtDatabase::SwissProt => "Manually annotated and reviewed protein sequences",
            UniProtDatabase::TrEMBL => "Automatically annotated protein sequences",
            UniProtDatabase::UniRef50 => "Clustered sequences at 50% identity",
            UniProtDatabase::UniRef90 => "Clustered sequences at 90% identity",
            UniProtDatabase::UniRef100 => "Clustered sequences at 100% identity",
            UniProtDatabase::IdMapping => "UniProt accession to taxonomy mapping",
        }
    }
    
    pub fn typical_size(&self) -> &str {
        match self {
            UniProtDatabase::SwissProt => "~100 MB compressed",
            UniProtDatabase::TrEMBL => "~50 GB compressed",
            UniProtDatabase::UniRef50 => "~10 GB compressed",
            UniProtDatabase::UniRef90 => "~20 GB compressed",
            UniProtDatabase::UniRef100 => "~60 GB compressed",
            UniProtDatabase::IdMapping => "~15 GB compressed",
        }
    }
}