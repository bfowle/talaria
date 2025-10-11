//! Download management system with resume capability and workspace isolation
//!
//! This module provides a robust download system for biological databases with:
//! - **Resume capability**: Downloads can be interrupted and resumed from any stage
//! - **Workspace isolation**: Each download gets a unique workspace preventing collisions
//! - **State persistence**: Download progress is saved and can be recovered after failures
//! - **Concurrent download prevention**: File-based locking prevents duplicate downloads
//! - **Distributed systems patterns**: Session IDs, deterministic naming, process locks
//!
//! # Architecture
//!
//! The download system uses a state machine pattern with the following stages:
//! - `Pending`: Initial state, ready to start download
//! - `Downloading`: Actively downloading from remote source
//! - `Verifying`: Checking checksums/integrity
//! - `Decompressing`: Extracting compressed files
//! - `Processing`: Converting to internal format (e.g., chunking)
//! - `Completed`: Successfully finished
//! - `Failed`: Error occurred (may be recoverable)
//!
//! # Workspace Structure
//!
//! Each download gets an isolated workspace at:
//! ```text
//! ${TALARIA_DATA_DIR}/downloads/{database}_{version}_{session_id}/
//! ├── state.json          # Persistent state for resume
//! ├── .lock              # Process lock file
//! ├── checkpoints/       # Recovery checkpoints
//! └── files/            # Downloaded/processed files
//! ```
//!
//! # Environment Variables
//!
//! - `TALARIA_PRESERVE_ON_FAILURE`: Keep workspace on errors for debugging
//! - `TALARIA_PRESERVE_ALWAYS`: Never clean up workspaces
//! - `TALARIA_DATA_DIR`: Base directory for all downloads
//!
//! # Example Usage
//!
//! ```rust,no_run
//! use talaria_sequoia::download::{DownloadManager, DownloadOptions, DownloadProgress};
//! use talaria_core::DatabaseSource;
//!
//! async fn download_database() -> anyhow::Result<()> {
//!     let mut manager = DownloadManager::new()?;
//!     let mut progress = DownloadProgress::new();
//!
//!     let options = DownloadOptions {
//!         resume: true,  // Resume if interrupted
//!         skip_verify: false,
//!         preserve_on_failure: true,
//!         preserve_always: false,
//!         force: false,
//!     };
//!
//!     let path = manager.download_with_state(
//!         DatabaseSource::Custom("my_test_db".to_string()),
//!         options,
//!         &mut progress,
//!     ).await?;
//!
//!     tracing::info!("Downloaded to: {}", path.display());
//!     Ok(())
//! }
//! ```

pub mod manager;
pub mod ncbi;
pub mod progress;
pub mod resumable_downloader;
pub mod resume;
pub mod unified_progress;
pub mod uniprot;
pub mod workspace;

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub use manager::{DownloadManager, DownloadOptions};
pub use ncbi::NCBIDownloader;
pub use progress::DownloadProgress;
pub use uniprot::UniProtDownloader;
pub use workspace::{
    find_existing_workspace_for_source, find_resumable_downloads, get_download_workspace,
    DatabaseSourceExt, DownloadState, Stage,
};

/// Verify file checksum
#[allow(dead_code)]
pub fn verify_checksum(file_path: &Path, expected_checksum: &str) -> io::Result<bool> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    let calculated = format!("{:x}", result);

    Ok(calculated == expected_checksum)
}

/// Database configuration
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub name: String,
    pub base_url: String,
    pub datasets: Vec<DatasetInfo>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DatasetInfo {
    pub name: String,
    pub filename: String,
    pub url: String,
    pub size_mb: Option<usize>,
    pub checksum: Option<String>,
    pub description: String,
}

/// Get default database configurations
#[allow(dead_code)]
pub fn get_database_configs() -> Vec<DatabaseConfig> {
    vec![
        DatabaseConfig {
            name: "UniProt".to_string(),
            base_url: "https://ftp.ebi.ac.uk/pub/databases/uniprot/".to_string(),
            datasets: vec![
                DatasetInfo {
                    name: "SwissProt".to_string(),
                    filename: "uniprot_sprot.fasta.gz".to_string(),
                    url: "current_release/knowledgebase/complete/uniprot_sprot.fasta.gz"
                        .to_string(),
                    size_mb: Some(85),
                    checksum: None,
                    description: "Manually reviewed protein sequences".to_string(),
                },
                DatasetInfo {
                    name: "TrEMBL".to_string(),
                    filename: "uniprot_trembl.fasta.gz".to_string(),
                    url: "current_release/knowledgebase/complete/uniprot_trembl.fasta.gz"
                        .to_string(),
                    size_mb: Some(52000),
                    checksum: None,
                    description: "Unreviewed protein sequences".to_string(),
                },
            ],
        },
        DatabaseConfig {
            name: "NCBI".to_string(),
            base_url: "https://ftp.ncbi.nlm.nih.gov/".to_string(),
            datasets: vec![
                DatasetInfo {
                    name: "nr".to_string(),
                    filename: "nr.gz".to_string(),
                    url: "blast/db/FASTA/nr.gz".to_string(),
                    size_mb: Some(90000),
                    checksum: None,
                    description: "Non-redundant protein sequences".to_string(),
                },
                DatasetInfo {
                    name: "Taxonomy".to_string(),
                    filename: "taxdump.tar.gz".to_string(),
                    url: "pub/taxonomy/taxdump.tar.gz".to_string(),
                    size_mb: Some(50),
                    checksum: None,
                    description: "NCBI taxonomy database".to_string(),
                },
            ],
        },
    ]
}

// Import DatabaseSource from talaria-core
pub use talaria_core::{DatabaseSource, NCBIDatabase, UniProtDatabase};

// Additional helper functions for DatabaseSource
/// Parse a database name string into a DatabaseSource
pub fn parse_database_source(name: &str) -> anyhow::Result<DatabaseSource> {
    // Handle source/dataset format (e.g., "ncbi/taxonomy", "uniprot/swissprot")
    if name.contains('/') {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() == 2 {
            let source = parts[0];
            let dataset = parts[1];

            // Handle UniProt databases
            if source.eq_ignore_ascii_case("uniprot") {
                return match dataset.to_lowercase().as_str() {
                    "swissprot" | "sprot" => {
                        Ok(DatabaseSource::UniProt(UniProtDatabase::SwissProt))
                    }
                    "trembl" => Ok(DatabaseSource::UniProt(UniProtDatabase::TrEMBL)),
                    "uniref50" => Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef50)),
                    "uniref90" => Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef90)),
                    "uniref100" => Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef100)),
                    "idmapping" => Ok(DatabaseSource::UniProt(UniProtDatabase::IdMapping)),
                    _ => Ok(DatabaseSource::Custom(name.to_string())),
                };
            }

            // Handle NCBI databases
            if source.eq_ignore_ascii_case("ncbi") {
                return match dataset.to_lowercase().as_str() {
                    "nr" => Ok(DatabaseSource::NCBI(NCBIDatabase::NR)),
                    "nt" => Ok(DatabaseSource::NCBI(NCBIDatabase::NT)),
                    "refseq-protein" | "refseq_protein" | "refseq" => {
                        Ok(DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein))
                    }
                    "refseq-genomic" | "refseq_genomic" => {
                        Ok(DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic))
                    }
                    "taxonomy" => Ok(DatabaseSource::NCBI(NCBIDatabase::Taxonomy)),
                    "prot-accession2taxid" | "prot_accession2taxid" => {
                        Ok(DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId))
                    }
                    "nucl-accession2taxid" | "nucl_accession2taxid" => {
                        Ok(DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId))
                    }
                    _ => Ok(DatabaseSource::Custom(name.to_string())),
                };
            }
        }
    }

    // Try legacy format (just the dataset name without source prefix)
    if name.eq_ignore_ascii_case("swissprot") || name.eq_ignore_ascii_case("sprot") {
        return Ok(DatabaseSource::UniProt(UniProtDatabase::SwissProt));
    }
    if name.eq_ignore_ascii_case("trembl") {
        return Ok(DatabaseSource::UniProt(UniProtDatabase::TrEMBL));
    }
    if name.eq_ignore_ascii_case("uniref50") {
        return Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef50));
    }
    if name.eq_ignore_ascii_case("uniref90") {
        return Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef90));
    }
    if name.eq_ignore_ascii_case("uniref100") {
        return Ok(DatabaseSource::UniProt(UniProtDatabase::UniRef100));
    }

    // Try common NCBI databases
    if name.eq_ignore_ascii_case("refseq_protein") || name.eq_ignore_ascii_case("refseq") {
        return Ok(DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein));
    }
    if name.eq_ignore_ascii_case("refseq_genomic") {
        return Ok(DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic));
    }
    if name.eq_ignore_ascii_case("nr") || name.eq_ignore_ascii_case("ncbi_nr") {
        return Ok(DatabaseSource::NCBI(NCBIDatabase::NR));
    }
    if name.eq_ignore_ascii_case("nt") {
        return Ok(DatabaseSource::NCBI(NCBIDatabase::NT));
    }
    if name.eq_ignore_ascii_case("taxonomy") {
        return Ok(DatabaseSource::NCBI(NCBIDatabase::Taxonomy));
    }

    // Default to Custom
    Ok(DatabaseSource::Custom(name.to_string()))
}

pub async fn download_database(
    source: DatabaseSource,
    output_path: &Path,
    progress: &mut DownloadProgress,
) -> Result<()> {
    download_database_with_options(source, output_path, progress, false).await
}

pub async fn download_database_with_options(
    source: DatabaseSource,
    output_path: &Path,
    progress: &mut DownloadProgress,
    skip_verify: bool,
) -> Result<()> {
    download_database_with_full_options(source, output_path, progress, skip_verify, false).await
}

pub async fn download_database_with_full_options(
    source: DatabaseSource,
    output_path: &Path,
    progress: &mut DownloadProgress,
    skip_verify: bool,
    resume: bool,
) -> Result<()> {
    match source {
        DatabaseSource::UniProt(db) => {
            let downloader = UniProtDownloader::new();
            match db {
                UniProtDatabase::SwissProt => {
                    if skip_verify || resume {
                        downloader.download_and_extract_with_options(
                            &format!("{}/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz", 
                                    "https://ftp.ebi.ac.uk/pub/databases/uniprot"),
                            output_path,
                            progress,
                            skip_verify,
                            resume
                        ).await
                    } else {
                        downloader.download_swissprot(output_path, progress).await
                    }
                }
                UniProtDatabase::TrEMBL => {
                    if resume {
                        downloader.download_and_extract_with_options(
                            &format!("{}/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz",
                                    "https://ftp.ebi.ac.uk/pub/databases/uniprot"),
                            output_path,
                            progress,
                            skip_verify,
                            resume
                        ).await
                    } else {
                        downloader.download_trembl(output_path, progress).await
                    }
                }
                UniProtDatabase::UniRef50 => {
                    if resume {
                        downloader
                            .download_and_extract_with_options(
                                &format!(
                                    "{}/current_release/uniref/uniref50/uniref50.fasta.gz",
                                    "https://ftp.ebi.ac.uk/pub/databases/uniprot"
                                ),
                                output_path,
                                progress,
                                skip_verify,
                                resume,
                            )
                            .await
                    } else {
                        downloader.download_uniref50(output_path, progress).await
                    }
                }
                UniProtDatabase::UniRef90 => {
                    if resume {
                        downloader
                            .download_and_extract_with_options(
                                &format!(
                                    "{}/current_release/uniref/uniref90/uniref90.fasta.gz",
                                    "https://ftp.ebi.ac.uk/pub/databases/uniprot"
                                ),
                                output_path,
                                progress,
                                skip_verify,
                                resume,
                            )
                            .await
                    } else {
                        downloader.download_uniref90(output_path, progress).await
                    }
                }
                UniProtDatabase::UniRef100 => {
                    if resume {
                        downloader
                            .download_and_extract_with_options(
                                &format!(
                                    "{}/current_release/uniref/uniref100/uniref100.fasta.gz",
                                    "https://ftp.ebi.ac.uk/pub/databases/uniprot"
                                ),
                                output_path,
                                progress,
                                skip_verify,
                                resume,
                            )
                            .await
                    } else {
                        downloader.download_uniref100(output_path, progress).await
                    }
                }
                UniProtDatabase::IdMapping => {
                    if resume {
                        downloader
                            .download_idmapping_with_resume(output_path, progress, resume)
                            .await
                    } else {
                        downloader.download_idmapping(output_path, progress).await
                    }
                }
            }
        }
        DatabaseSource::NCBI(db) => {
            let downloader = NCBIDownloader::new();
            match db {
                NCBIDatabase::NR => {
                    if resume {
                        let url = "https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nr.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        downloader.download_nr(output_path, progress).await
                    }
                }
                NCBIDatabase::NT => {
                    if resume {
                        let url = "https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nt.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        downloader.download_nt(output_path, progress).await
                    }
                }
                NCBIDatabase::RefSeqProtein => {
                    if resume {
                        let url = "https://ftp.ncbi.nlm.nih.gov/refseq/release/complete/complete.protein.faa.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        downloader
                            .download_refseq_protein(output_path, progress)
                            .await
                    }
                }
                NCBIDatabase::RefSeqGenomic => {
                    if resume {
                        let url = "https://ftp.ncbi.nlm.nih.gov/refseq/release/complete/complete.genomic.fna.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        downloader
                            .download_refseq_genomic(output_path, progress)
                            .await
                    }
                }
                NCBIDatabase::Taxonomy => {
                    // Download the tar.gz file directly to output_path
                    // store_taxonomy_mapping_file will handle extraction
                    downloader.download_taxonomy(output_path, progress).await
                }
                NCBIDatabase::ProtAccession2TaxId => {
                    if resume {
                        downloader.download_compressed_with_resume(
                            "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/prot.accession2taxid.gz",
                            output_path, progress, resume
                        ).await
                    } else {
                        downloader
                            .download_prot_accession2taxid(output_path, progress)
                            .await
                    }
                }
                NCBIDatabase::NuclAccession2TaxId => {
                    if resume {
                        downloader.download_compressed_with_resume(
                            "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/nucl_gb.accession2taxid.gz",
                            output_path, progress, resume
                        ).await
                    } else {
                        downloader
                            .download_nucl_accession2taxid(output_path, progress)
                            .await
                    }
                }
                NCBIDatabase::RefSeq => {
                    // RefSeq is a combination of RefSeqProtein and RefSeqGenomic
                    // For now, download the protein version
                    if resume {
                        let url = "https://ftp.ncbi.nlm.nih.gov/refseq/release/complete/complete.protein.faa.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        downloader
                            .download_refseq_protein(output_path, progress)
                            .await
                    }
                }
                NCBIDatabase::GenBank => {
                    // Download GenBank sequences
                    if resume {
                        let url =
                            "https://ftp.ncbi.nlm.nih.gov/genbank/genbank.fasta.gz".to_string();
                        downloader
                            .download_and_extract_with_resume(&url, output_path, progress, resume)
                            .await
                    } else {
                        // Use a generic download method for GenBank
                        downloader
                            .download_compressed(
                                "https://ftp.ncbi.nlm.nih.gov/genbank/genbank.fasta.gz",
                                output_path,
                                progress,
                            )
                            .await
                    }
                }
            }
        }
        DatabaseSource::Custom(path) => {
            progress.set_message(&format!("Using custom database: {}", path));
            progress.finish();
            Ok(())
        }
    }
}
