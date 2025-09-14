pub mod ncbi;
pub mod progress;
pub mod uniprot;

use anyhow::Result;
use sha2::{Sha256, Digest};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub use ncbi::{NCBIDatabase, NCBIDownloader};
pub use progress::DownloadProgress;
pub use uniprot::{UniProtDatabase, UniProtDownloader};

/// Verify file checksum
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
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub name: String,
    pub base_url: String,
    pub datasets: Vec<DatasetInfo>,
}

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
pub fn get_database_configs() -> Vec<DatabaseConfig> {
    vec![
        DatabaseConfig {
            name: "UniProt".to_string(),
            base_url: "https://ftp.ebi.ac.uk/pub/databases/uniprot/".to_string(),
            datasets: vec![
                DatasetInfo {
                    name: "SwissProt".to_string(),
                    filename: "uniprot_sprot.fasta.gz".to_string(),
                    url: "current_release/knowledgebase/complete/uniprot_sprot.fasta.gz".to_string(),
                    size_mb: Some(85),
                    checksum: None,
                    description: "Manually reviewed protein sequences".to_string(),
                },
                DatasetInfo {
                    name: "TrEMBL".to_string(),
                    filename: "uniprot_trembl.fasta.gz".to_string(),
                    url: "current_release/knowledgebase/complete/uniprot_trembl.fasta.gz".to_string(),
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

#[derive(Clone)]
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
}

impl std::fmt::Display for DatabaseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseSource::UniProt(db) => write!(f, "UniProt: {}", db),
            DatabaseSource::NCBI(db) => write!(f, "NCBI: {}", db),
            DatabaseSource::Custom(path) => write!(f, "Custom: {}", path),
        }
    }
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
                    downloader.download_trembl(output_path, progress).await
                }
                UniProtDatabase::UniRef50 => {
                    downloader.download_uniref50(output_path, progress).await
                }
                UniProtDatabase::UniRef90 => {
                    downloader.download_uniref90(output_path, progress).await
                }
                UniProtDatabase::UniRef100 => {
                    downloader.download_uniref100(output_path, progress).await
                }
                UniProtDatabase::IdMapping => {
                    if resume {
                        downloader.download_idmapping_with_resume(output_path, progress, resume).await
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
                        let url = format!("https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nr.gz");
                        downloader.download_and_extract_with_resume(&url, output_path, progress, resume).await
                    } else {
                        downloader.download_nr(output_path, progress).await
                    }
                }
                NCBIDatabase::NT => {
                    if resume {
                        let url = format!("https://ftp.ncbi.nlm.nih.gov/blast/db/FASTA/nt.gz");
                        downloader.download_and_extract_with_resume(&url, output_path, progress, resume).await
                    } else {
                        downloader.download_nt(output_path, progress).await
                    }
                }
                NCBIDatabase::RefSeqProtein => {
                    if resume {
                        let url = format!("https://ftp.ncbi.nlm.nih.gov/refseq/release/complete/complete.protein.faa.gz");
                        downloader.download_and_extract_with_resume(&url, output_path, progress, resume).await
                    } else {
                        downloader.download_refseq_protein(output_path, progress).await
                    }
                }
                NCBIDatabase::RefSeqGenomic => {
                    if resume {
                        let url = format!("https://ftp.ncbi.nlm.nih.gov/refseq/release/complete/complete.genomic.fna.gz");
                        downloader.download_and_extract_with_resume(&url, output_path, progress, resume).await
                    } else {
                        downloader.download_refseq_genomic(output_path, progress).await
                    }
                }
                NCBIDatabase::Taxonomy => {
                    let output_dir = output_path.parent().unwrap_or(Path::new("."));
                    downloader.download_taxonomy(output_dir, progress).await
                }
                NCBIDatabase::ProtAccession2TaxId => {
                    if resume {
                        downloader.download_compressed_with_resume(
                            &format!("https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/prot.accession2taxid.gz"),
                            output_path, progress, resume
                        ).await
                    } else {
                        downloader.download_prot_accession2taxid(output_path, progress).await
                    }
                }
                NCBIDatabase::NuclAccession2TaxId => {
                    if resume {
                        downloader.download_compressed_with_resume(
                            &format!("https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/accession2taxid/nucl_gb.accession2taxid.gz"),
                            output_path, progress, resume
                        ).await
                    } else {
                        downloader.download_nucl_accession2taxid(output_path, progress).await
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