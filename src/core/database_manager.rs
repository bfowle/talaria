/// Database manager using content-addressed storage
///
/// Instead of downloading entire databases and creating dated directories,
/// this uses content-addressed storage with manifests for efficient updates.

use crate::casg::{CASGRepository, TaxonomicChunker, ChunkingStrategy, SHA256Hash};
use crate::bio::sequence::Sequence;
use crate::core::paths;
use crate::download::{DatabaseSource, NCBIDatabase, UniProtDatabase};
use crate::utils::progress::create_progress_bar;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct DatabaseManager {
    repository: CASGRepository,
    base_path: PathBuf,
}

impl DatabaseManager {
    /// Create a new CASG database manager
    pub fn get_storage(&self) -> &crate::casg::storage::CASGStorage {
        &self.repository.storage
    }

    pub fn new(base_dir: Option<String>) -> Result<Self> {
        let base_path = if let Some(dir) = base_dir {
            PathBuf::from(dir)
        } else {
            // Use centralized path configuration
            paths::talaria_databases_dir()
        };

        // Ensure directory exists
        std::fs::create_dir_all(&base_path)?;

        // Initialize or open CASG repository
        // Always use open if chunks directory exists (indicating existing data)
        let repository = if base_path.join("chunks").exists() {
            CASGRepository::open(&base_path)?
        } else {
            CASGRepository::init(&base_path)?
        };

        Ok(Self {
            repository,
            base_path,
        })
    }

    /// Download a database using CASG
    pub async fn download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Check if we have a cached manifest
        let manifest_path = self.get_manifest_path(source);
        let mut has_existing = manifest_path.exists();

        // Check for old manifest location and migrate if needed
        if !has_existing {
            let old_manifest_path = self.base_path.join("manifest.json");
            if old_manifest_path.exists() {
                progress_callback("Migrating manifest from old location...");

                // Ensure manifests directory exists
                if let Some(parent) = manifest_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Copy the old manifest to the new location
                std::fs::copy(&old_manifest_path, &manifest_path)?;

                // Also copy .etag file if it exists
                let old_etag = old_manifest_path.with_extension("etag");
                if old_etag.exists() {
                    let new_etag = manifest_path.with_extension("etag");
                    std::fs::copy(old_etag, new_etag).ok();
                }

                has_existing = true;
                progress_callback("Manifest migration complete");
            }
        }

        // If we have an existing manifest, check for updates
        if has_existing {
            // Try to get manifest URL (may not exist in dev/local mode)
            if let Ok(manifest_url) = self.get_manifest_url(source) {
                progress_callback("Checking for updates...");

                // Set remote URL in repository
                self.repository.manifest.set_remote_url(manifest_url.clone());

                // Try to check for updates, but don't fail if manifest server is unavailable
                match self.repository.check_updates().await {
                    Ok(false) => {
                        progress_callback("Database is up to date");
                        return Ok(DownloadResult::UpToDate);
                    }
                    Ok(true) => {
                        progress_callback("Updates available, downloading manifest...");
                        // Try to fetch remote manifest
                        match self.repository.manifest.fetch_remote().await {
                            Ok(new_manifest) => {
                                // Successfully got remote manifest, proceed with incremental update
                                return self.handle_incremental_update(new_manifest, progress_callback).await;
                            }
                            Err(_) => {
                                progress_callback("[!] Manifest server unavailable, keeping current version");
                                return Ok(DownloadResult::UpToDate);
                            }
                        }
                    }
                    Err(_) => {
                        // Manifest server unavailable, but we have local data
                        progress_callback("[!] Cannot check for updates (manifest server unavailable)");
                        return Ok(DownloadResult::UpToDate);
                    }
                }
            } else {
                // No manifest URL available (dev mode), just use local
                progress_callback("Using local CASG database (no remote manifest configured)");
                return Ok(DownloadResult::UpToDate);
            }
        }

        // No existing manifest - need to do initial download
        progress_callback("[NEW] Initial download required - no local CASG data found");
        progress_callback("This will download the full database and convert it to CASG format");
        progress_callback("Future updates will be incremental and much faster!");

        self.handle_initial_download(source, progress_callback).await
    }

    /// Handle incremental update when manifest is available
    async fn handle_incremental_update(
        &mut self,
        new_manifest: crate::casg::Manifest,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        use crate::casg::{OperationType, SourceInfo};

        // Get manifest data for version info
        let manifest_data = new_manifest.get_data()
            .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
        let manifest_hash = SHA256Hash::compute(&serde_json::to_vec(&manifest_data)?);
        let manifest_version = manifest_data.version.clone();

        // Compute diff to see what chunks we need
        let diff = self.repository.manifest.diff(&new_manifest)?;

        let chunks_to_download = diff.new_chunks.len();
        let chunks_to_remove = diff.removed_chunks.len();

        // Check for resumable state
        let source_info = SourceInfo {
            database: manifest_data.source_database.clone().unwrap_or_else(|| "unknown".to_string()),
            source_url: new_manifest.get_remote_url().map(|s| s.to_string()),
            etag: new_manifest.get_etag().map(|s| s.to_string()),
            total_size_bytes: None,
        };

        let resumable_state = self.repository.storage.check_resumable(
            &source_info.database,
            &OperationType::IncrementalUpdate,
            &manifest_hash,
            &manifest_version,
        )?;

        if let Some(state) = resumable_state {
            progress_callback(&format!(
                "Found resumable update: {} ({:.1}% complete)",
                state.summary(),
                state.completion_percentage()
            ));
            progress_callback(&format!(
                "Resuming with {} chunks remaining",
                state.remaining_chunks()
            ));
        } else if chunks_to_download > 0 {
            // Start new processing operation
            self.repository.storage.start_processing(
                OperationType::IncrementalUpdate,
                manifest_hash,
                manifest_version.clone(),
                chunks_to_download,
                source_info,
            )?;
        }

        progress_callback(&format!(
            "Need to download {} new chunks, remove {} old chunks",
            chunks_to_download, chunks_to_remove
        ));

        // Download only new chunks (with resume support)
        if !diff.new_chunks.is_empty() {
            progress_callback("Downloading new chunks...");
            let downloaded = self.repository.storage.fetch_chunks_with_resume(
                &diff.new_chunks,
                true  // Enable resume checking
            ).await?;

            progress_callback(&format!(
                "Downloaded {} chunks, {:.2} MB",
                downloaded.len(),
                downloaded.iter().map(|c| c.size).sum::<usize>() as f64 / 1_048_576.0
            ));
        }

        // Remove old chunks (garbage collection)
        if !diff.removed_chunks.is_empty() {
            progress_callback("Removing obsolete chunks...");

            // Get all currently referenced chunks from the new manifest
            let manifest_data = new_manifest.get_data()
                .ok_or_else(|| anyhow::anyhow!("No manifest data"))?;
            let referenced_chunks: Vec<SHA256Hash> = manifest_data.chunk_index
                .iter()
                .map(|c| c.hash.clone())
                .collect();

            // Run garbage collection
            let gc_result = self.repository.storage.gc(&referenced_chunks)?;

            if gc_result.removed_count > 0 {
                progress_callback(&format!(
                    "Removed {} obsolete chunks, freed {:.2} MB",
                    gc_result.removed_count,
                    gc_result.freed_space as f64 / 1_048_576.0
                ));
            }
        }

        // Mark operation as complete
        self.repository.storage.complete_processing()?;

        // Track version in temporal index before updating manifest
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Add sequence version tracking
        if let Some(manifest_data) = new_manifest.get_data() {
            temporal_index.add_sequence_version(
                manifest_data.version.clone(),
                manifest_data.sequence_root.clone(),
                manifest_data.chunk_index.len(),
                manifest_data.chunk_index.iter()
                    .map(|c| c.sequence_count)
                    .sum(),
            )?;

            // Save the temporal index
            temporal_index.save()?;
        }

        // Update manifest
        self.repository.manifest = new_manifest;
        self.repository.manifest.save()?;

        Ok(DownloadResult::Updated {
            chunks_added: chunks_to_download,
            chunks_removed: chunks_to_remove,
        })
    }

    /// Handle initial download when no local manifest exists
    /// Check if the database being downloaded is taxonomy data itself
    fn is_taxonomy_database(source: &DatabaseSource) -> bool {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        match source {
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => true,
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => true,
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => true,
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => true,
            _ => false,
        }
    }

    /// Create a simple manifest for taxonomy files to track their download
    fn create_taxonomy_manifest(&self, source: &DatabaseSource, file_path: &Path) -> Result<()> {
        use chrono::Utc;
        use serde_json::json;

        let manifest_path = self.get_manifest_path(source);

        // Ensure manifests directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Get file metadata
        let metadata = std::fs::metadata(file_path)?;
        let file_size = metadata.len();

        // Create a simple manifest
        let manifest = json!({
            "type": "taxonomy",
            "source": source.to_string(),
            "file_path": file_path.to_string_lossy(),
            "file_size": file_size,
            "downloaded_at": Utc::now().to_rfc3339(),
            "version": Utc::now().format("%Y-%m-%d").to_string(),
        });

        // Write manifest
        let manifest_content = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(&manifest_path, manifest_content)?;

        println!("  Manifest created: {}", manifest_path.display());
        Ok(())
    }

    /// Store taxonomy mapping files directly without FASTA processing
    fn store_taxonomy_mapping_file(&mut self, file_path: &Path, source: &DatabaseSource) -> Result<()> {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        println!("Storing taxonomy mapping file...");

        // Determine the appropriate storage location based on the source
        let dest_dir = match source {
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                self.base_path.join("taxonomy").join("uniprot")
            }
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => {
                self.base_path.join("taxonomy").join("taxdump")
            }
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                self.base_path.join("taxonomy").join("accession2taxid")
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                self.base_path.join("taxonomy").join("accession2taxid")
            }
            _ => {
                return Err(anyhow::anyhow!("Not a taxonomy mapping file: {}", source));
            }
        };

        // Create destination directory
        std::fs::create_dir_all(&dest_dir)?;

        // Determine the destination filename
        let dest_file = match source {
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => {
                dest_dir.join("idmapping.dat.gz")
            }
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => {
                // Taxonomy is a tar.gz that needs extraction
                println!("Extracting taxonomy dump...");
                // Extract directly to the taxdump directory
                let tar_gz = std::fs::File::open(file_path)?;
                let tar = flate2::read::GzDecoder::new(tar_gz);
                let mut archive = tar::Archive::new(tar);
                archive.unpack(&dest_dir)?;
                println!("Taxonomy dump extracted successfully");
                return Ok(());
            }
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                dest_dir.join("prot.accession2taxid.gz")
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                dest_dir.join("nucl.accession2taxid.gz")
            }
            _ => unreachable!(),
        };

        // Copy or move the file to its destination
        println!("Moving taxonomy file to: {}", dest_file.display());
        std::fs::rename(file_path, &dest_file)
            .or_else(|_| -> Result<()> {
                // If rename fails (e.g., across filesystems), copy and delete
                std::fs::copy(file_path, &dest_file)?;
                std::fs::remove_file(file_path)?;
                Ok(())
            })?;

        println!("✓ Taxonomy mapping file stored successfully");
        println!("  Location: {}", dest_file.display());

        // Create a simple manifest for this taxonomy file
        self.create_taxonomy_manifest(source, &dest_file)?;

        Ok(())
    }

    async fn handle_initial_download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Skip taxonomy check if we're downloading taxonomy data itself
        if !Self::is_taxonomy_database(source) {
            // Check if taxonomy is needed and download if missing
            if !self.repository.taxonomy.has_taxonomy() {
                progress_callback("Checking for taxonomy data...");
                if let Err(e) = self.ensure_taxonomy_loaded(&progress_callback).await {
                    progress_callback(&format!("[!] Warning: Could not load taxonomy: {}", e));
                    progress_callback("Continuing without taxonomy data (will use placeholders)");
                    // Ensure at least a minimal taxonomy structure
                    self.repository.taxonomy.ensure_taxonomy()?;
                }
            }
        }

        // For initial download, fall back to traditional download
        // then chunk it into CASG format
        let temp_file = self.base_path.join("temp_download.fasta.gz");

        progress_callback("Downloading full database (this may take a while)...");

        // Download full file
        self.download_full_database(source, &temp_file, &progress_callback).await?;

        // Chunk the database
        progress_callback("Processing database into CASG chunks...");
        progress_callback("This one-time conversion enables future incremental updates");
        self.chunk_database(&temp_file, source)?;

        // Clean up temp file
        if temp_file.exists() {
            std::fs::remove_file(&temp_file).ok();
        }

        progress_callback("✓ Initial CASG setup complete!");
        progress_callback("Future updates will only download changed chunks");

        Ok(DownloadResult::InitialDownload)
    }

    /// Chunk a downloaded database into CASG format
    fn chunk_database(&mut self, file_path: &Path, source: &DatabaseSource) -> Result<()> {
        // Check if this is a taxonomy mapping file (not a FASTA file)
        if Self::is_taxonomy_database(source) {
            return self.store_taxonomy_mapping_file(file_path, source);
        }

        // Load taxonomy mapping if available
        let taxonomy_map = self.load_taxonomy_mapping(source)?;

        // Create chunker with strategy
        let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
        chunker.load_taxonomy_mapping(taxonomy_map);

        // Read sequences from FASTA file
        println!("Reading sequences from FASTA file...");
        let sequences = self.read_fasta_sequences(file_path)?;

        // Analyze and chunk
        println!("Analyzing database structure...");
        let analysis = chunker.analyze_for_strategy(&sequences);
        println!("Database analysis:");
        println!("  Total sequences: {}", analysis.total_sequences);
        println!("  Unique taxa: {}", analysis.unique_taxa);
        println!("  High-volume taxa: {}", analysis.high_volume_taxa.len());

        println!("Creating taxonomy-aware chunks...");
        let chunks = chunker.chunk_sequences(sequences)?;

        println!("Created {} chunks", chunks.len());

        // Store chunks in CASG with parallel processing
        let total_chunks = chunks.len();
        let pb = ProgressBar::new(total_chunks as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message("Storing chunks in CASG repository");

        // Create thread-safe wrappers
        let pb = Arc::new(Mutex::new(pb));
        let storage = &self.repository.storage;
        let taxonomy = Arc::new(Mutex::new(&mut self.repository.taxonomy));

        // Process chunks in parallel using rayon
        let results: Vec<_> = chunks
            .par_iter()
            .map(|chunk| {
                // Store chunk in storage (already thread-safe)
                let store_result = storage.store_taxonomy_chunk(chunk);

                // Update taxonomy mapping (requires mutex for write access)
                if store_result.is_ok() {
                    let mut tax = taxonomy.lock().unwrap();
                    tax.update_chunk_mapping(chunk);
                }

                // Update progress bar
                pb.lock().unwrap().inc(1);

                store_result
            })
            .collect();

        pb.lock().unwrap().finish_with_message("All chunks stored");

        // Check for any errors
        for result in results {
            result?;
        }

        // Create and save manifest
        println!("Creating and saving manifest...");
        let mut manifest_data = self.repository.manifest.create_from_chunks(
            chunks,
            self.repository.taxonomy.get_taxonomy_root()?,
            self.repository.storage.get_sequence_root()?,
        )?;

        // Set the source database (use slash format for consistency)
        manifest_data.source_database = Some(match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => "uniprot/swissprot".to_string(),
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => "uniprot/trembl".to_string(),
            DatabaseSource::NCBI(NCBIDatabase::NR) => "ncbi/nr".to_string(),
            DatabaseSource::NCBI(NCBIDatabase::NT) => "ncbi/nt".to_string(),
            _ => "custom".to_string(),
        });

        // Save manifest to database-specific location
        let manifest_path = self.get_manifest_path(source);

        // Ensure manifests directory exists
        if let Some(parent) = manifest_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write manifest to the database-specific path
        let manifest_content = serde_json::to_string_pretty(&manifest_data)?;
        std::fs::write(&manifest_path, manifest_content)?;

        // Track version in temporal index
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Add sequence version tracking
        temporal_index.add_sequence_version(
            manifest_data.version.clone(),
            manifest_data.sequence_root.clone(),
            manifest_data.chunk_index.len(),
            manifest_data.chunk_index.iter()
                .map(|c| c.sequence_count)
                .sum(),
        )?;

        // Save the temporal index
        temporal_index.save()?;
        println!("Version history updated");

        // Also update the repository's manifest for immediate use
        self.repository.manifest.set_data(manifest_data);

        println!("Manifest saved successfully to {}", manifest_path.display());

        Ok(())
    }

    /// Get manifest URL for a database source
    fn get_manifest_url(&self, source: &DatabaseSource) -> Result<String> {
        // Check environment variable for manifest server
        if let Ok(manifest_server) = std::env::var("TALARIA_MANIFEST_SERVER") {
            return Ok(match source {
                DatabaseSource::UniProt(UniProtDatabase::SwissProt) =>
                    format!("{}/uniprot-swissprot.json", manifest_server),
                DatabaseSource::UniProt(UniProtDatabase::TrEMBL) =>
                    format!("{}/uniprot-trembl.json", manifest_server),
                DatabaseSource::NCBI(NCBIDatabase::NR) =>
                    format!("{}/ncbi-nr.json", manifest_server),
                DatabaseSource::NCBI(NCBIDatabase::NT) =>
                    format!("{}/ncbi-nt.json", manifest_server),
                _ => anyhow::bail!("No manifest URL for this database source"),
            });
        }

        // No manifest server configured - this is fine for local/dev use
        anyhow::bail!("No manifest server configured (set TALARIA_MANIFEST_SERVER for remote updates)")
    }

    /// Get local manifest path for a database
    fn get_manifest_path(&self, source: &DatabaseSource) -> PathBuf {
        use crate::download::{NCBIDatabase, UniProtDatabase};

        let filename = match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => "uniprot-swissprot.json",
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => "uniprot-trembl.json",
            DatabaseSource::UniProt(UniProtDatabase::UniRef50) => "uniprot-uniref50.json",
            DatabaseSource::UniProt(UniProtDatabase::UniRef90) => "uniprot-uniref90.json",
            DatabaseSource::UniProt(UniProtDatabase::UniRef100) => "uniprot-uniref100.json",
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => "uniprot-idmapping.json",
            DatabaseSource::NCBI(NCBIDatabase::NR) => "ncbi-nr.json",
            DatabaseSource::NCBI(NCBIDatabase::NT) => "ncbi-nt.json",
            DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein) => "ncbi-refseq-protein.json",
            DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic) => "ncbi-refseq-genomic.json",
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => "ncbi-taxonomy.json",
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => "ncbi-prot-accession2taxid.json",
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => "ncbi-nucl-accession2taxid.json",
            DatabaseSource::Custom(_) => "custom.json",
        };

        self.base_path.join("manifests").join(filename)
    }

    /// Download full database (for initial setup)
    async fn download_full_database(
        &self,
        source: &DatabaseSource,
        output_path: &Path,
        progress_callback: &impl Fn(&str),
    ) -> Result<()> {
        use crate::download::DownloadProgress;

        progress_callback("Downloading full database...");

        let mut progress = DownloadProgress::new();
        crate::download::download_database(
            source.clone(),
            output_path,
            &mut progress,
        ).await?;

        Ok(())
    }

    /// Get taxonomy mapping from CASG manifest
    /// This extracts accession-to-taxid mappings directly from the manifest's chunk metadata
    pub fn get_taxonomy_mapping_from_manifest(&self, source: &DatabaseSource) -> Result<std::collections::HashMap<String, crate::casg::TaxonId>> {
        use std::collections::HashMap;

        // Load manifest for this database
        let manifest_path = self.get_manifest_path(source);
        if !manifest_path.exists() {
            anyhow::bail!("Database manifest not found. Run download first.");
        }

        let manifest_content = std::fs::read_to_string(manifest_path)?;
        let manifest: crate::casg::TemporalManifest = serde_json::from_str(&manifest_content)?;

        let mut mapping = HashMap::new();

        println!("Processing {} chunks from manifest", manifest.chunk_index.len());

        // For each chunk, we need to load its sequences to get the accessions
        // and map them to the chunk's TaxIDs
        for (idx, chunk_meta) in manifest.chunk_index.iter().enumerate() {
            if chunk_meta.taxon_ids.is_empty() {
                println!("  Chunk {}: No taxon IDs, skipping", idx);
                continue; // Skip chunks without taxonomy
            }

            println!("  Chunk {}: {} taxon IDs", idx, chunk_meta.taxon_ids.len());

            // Load the chunk to get sequence headers
            let chunk_data = self.repository.storage.get_chunk(&chunk_meta.hash)?;

            // Parse sequences from chunk
            let sequences = crate::bio::fasta::parse_fasta_from_bytes(&chunk_data)?;

            // Map each sequence to the chunk's primary TaxID
            // Note: chunks are organized by taxonomy, so all sequences in a chunk
            // should have the same TaxID
            let primary_taxid = chunk_meta.taxon_ids[0];

            for seq in sequences {
                // Extract accession from sequence ID/header
                if let Some(accession) = Self::extract_accession_from_header(&seq.id) {
                    mapping.insert(accession.clone(), primary_taxid);

                    // Also store without version suffix if present
                    if let Some(dot_pos) = accession.rfind('.') {
                        mapping.insert(accession[..dot_pos].to_string(), primary_taxid);
                    }
                }
            }
        }

        println!("Extracted {} accession-to-taxid mappings from manifest", mapping.len());
        Ok(mapping)
    }

    /// Extract accession from FASTA header
    fn extract_accession_from_header(header: &str) -> Option<String> {
        // UniProt format: sp|P12345|PROT1_HUMAN or tr|Q12345|...
        if header.starts_with("sp|") || header.starts_with("tr|") {
            let parts: Vec<&str> = header.split('|').collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }

        // NCBI format: might be just the accession or gi|12345|ref|NP_123456.1|
        if header.contains('|') {
            let parts: Vec<&str> = header.split('|').collect();
            // Look for ref| or gb| or similar
            for (i, part) in parts.iter().enumerate() {
                if (*part == "ref" || *part == "gb" || *part == "emb" || *part == "dbj")
                    && i + 1 < parts.len() {
                    return Some(parts[i + 1].to_string());
                }
            }
        }

        // Simple format: just accession (possibly with version)
        let first_part = header.split_whitespace().next()?;
        Some(first_part.to_string())
    }

    /// Create a temporary accession2taxid file from manifest mapping
    pub fn create_accession2taxid_from_manifest(&self, source: &DatabaseSource) -> Result<PathBuf> {
        let mapping = self.get_taxonomy_mapping_from_manifest(source)?;

        // Create temporary file with .accession2taxid extension (required by LAMBDA)
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("talaria_manifest_{}.accession2taxid",
                                              std::process::id()));

        use std::io::Write;
        let mut file = std::fs::File::create(&temp_file)?;

        // Write header (NCBI format)
        writeln!(file, "accession\taccession.version\ttaxid\tgi")?;

        // Write mappings
        for (accession, taxid) in mapping {
            // Write in NCBI prot.accession2taxid format
            // accession, accession.version, taxid, gi (we use 0 for gi)
            writeln!(file, "{}\t{}\t{}\t0", accession, accession, taxid.0)?;
        }

        println!("Created temporary accession2taxid file with manifest data: {:?}", temp_file);
        Ok(temp_file)
    }

    /// Load taxonomy mapping for a database
    fn load_taxonomy_mapping(&self, source: &DatabaseSource) -> Result<std::collections::HashMap<String, crate::casg::TaxonId>> {
        use std::collections::HashMap;
        use flate2::read::GzDecoder;
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        // Try to load existing taxonomy mappings
        let mapping_file = match source {
            DatabaseSource::UniProt(_) => self.base_path.join("taxonomy").join("uniprot_idmapping.dat.gz"),
            DatabaseSource::NCBI(_) => self.base_path.join("taxonomy").join("prot.accession2taxid.gz"),
            _ => return Ok(HashMap::new()),
        };

        if !mapping_file.exists() {
            return Ok(HashMap::new());
        }

        eprintln!("● Loading taxonomy mapping from {}", mapping_file.display());
        let mut mappings = HashMap::new();

        let file = File::open(&mapping_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        let pb = crate::utils::progress::create_spinner("Parsing taxonomy mappings");
        let mut line_count = 0;

        match source {
            DatabaseSource::UniProt(_) => {
                // UniProt idmapping format: accession<tab>type<tab>value
                // We're looking for: P12345<tab>NCBI_TaxID<tab>9606
                for line_result in reader.lines() {
                    let line = line_result?;
                    line_count += 1;

                    if line_count % 100000 == 0 {
                        pb.set_message(format!("Processed {} mappings", line_count));
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 && parts[1] == "NCBI_TaxID" {
                        if let Ok(taxid) = parts[2].parse::<u32>() {
                            mappings.insert(parts[0].to_string(), crate::casg::TaxonId(taxid));
                        }
                    }
                }
            }
            DatabaseSource::NCBI(_) => {
                // NCBI prot.accession2taxid format:
                // accession.version<tab>taxid<tab>gi
                // Skip header line
                let mut lines = reader.lines();
                lines.next(); // Skip header

                for line_result in lines {
                    let line = line_result?;
                    line_count += 1;

                    if line_count % 100000 == 0 {
                        pb.set_message(format!("Processed {} mappings", line_count));
                    }

                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        if let Ok(taxid) = parts[1].parse::<u32>() {
                            // Store both with and without version
                            let accession = parts[0].to_string();
                            mappings.insert(accession.clone(), crate::casg::TaxonId(taxid));

                            // Also store without version suffix
                            if let Some(dot_pos) = accession.rfind('.') {
                                mappings.insert(accession[..dot_pos].to_string(), crate::casg::TaxonId(taxid));
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        pb.finish_with_message(format!("Loaded {} taxonomy mappings", mappings.len()));
        Ok(mappings)
    }

    /// Ensure taxonomy is loaded, downloading if necessary
    async fn ensure_taxonomy_loaded(
        &mut self,
        progress_callback: &impl Fn(&str),
    ) -> Result<()> {
        let taxonomy_dir = self.base_path.join("taxonomy");
        let taxdump_dir = taxonomy_dir.join("taxdump");

        // Check if taxonomy dump files exist
        let nodes_file = taxdump_dir.join("nodes.dmp");
        let names_file = taxdump_dir.join("names.dmp");

        if !nodes_file.exists() || !names_file.exists() {
            progress_callback("Taxonomy data not found, downloading NCBI taxonomy...");

            // Create taxonomy directory
            std::fs::create_dir_all(&taxdump_dir)?;

            // Download NCBI taxonomy
            let taxdump_url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";
            let taxdump_file = taxdump_dir.join("taxdump.tar.gz");

            progress_callback("Downloading NCBI taxonomy dump...");

            // Use reqwest to download
            let response = reqwest::get(taxdump_url).await?;
            let bytes = response.bytes().await?;
            std::fs::write(&taxdump_file, bytes)?;

            progress_callback("Extracting taxonomy files...");

            // Extract the tar.gz file
            use flate2::read::GzDecoder;
            use tar::Archive;

            let tar_gz = std::fs::File::open(&taxdump_file)?;
            let tar = GzDecoder::new(tar_gz);
            let mut archive = Archive::new(tar);
            archive.unpack(&taxdump_dir)?;

            // Clean up tar file
            std::fs::remove_file(taxdump_file).ok();

            progress_callback("Taxonomy files downloaded and extracted");
        }

        // Load the taxonomy
        progress_callback("Loading taxonomy data...");
        self.repository.taxonomy.load_ncbi_taxonomy(&taxdump_dir)?;
        progress_callback("Taxonomy loaded successfully");

        Ok(())
    }

    /// List all available databases in CASG
    pub fn list_databases(&self) -> Result<Vec<DatabaseInfo>> {
        let mut databases = Vec::new();

        // Look for databases in the new manifests/ directory structure
        // First, check if base_path exists
        if !self.base_path.exists() {
            return Ok(databases);
        }

        // Check for manifests in the manifests/ directory (new structure)
        let manifests_dir = self.base_path.join("manifests");
        if manifests_dir.exists() {
            for entry in std::fs::read_dir(&manifests_dir)? {
                let entry = entry?;
                let path = entry.path();

                // Only process .json files
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }

                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(manifest) = serde_json::from_str::<crate::casg::TemporalManifest>(&content) {
                        // Use source_database field if available, otherwise parse from filename
                        let name = if let Some(ref source_db) = manifest.source_database {
                            // Ensure slash format even for old manifests that might have hyphens
                            if source_db.contains('-') && !source_db.contains('/') {
                                source_db.replace('-', "/")
                            } else {
                                source_db.clone()
                            }
                        } else {
                            // Parse from filename like "uniprot-swissprot.json"
                            path.file_stem()
                                .and_then(|s| s.to_str())
                                .map(|s| s.replace('-', "/"))
                                .unwrap_or_else(|| "unknown/database".to_string())
                        };

                        databases.push(DatabaseInfo {
                            name,
                            version: manifest.version,
                            created_at: manifest.created_at,
                            chunk_count: manifest.chunk_index.len(),
                            total_size: manifest.chunk_index.iter().map(|c| c.size).sum(),
                        });
                    }
                }
            }
        }

        // Iterate through source directories (e.g., uniprot, ncbi, custom)
        for source_entry in std::fs::read_dir(&self.base_path)? {
            let source_entry = source_entry?;
            let source_path = source_entry.path();

            // Skip non-directories and special directories
            if !source_path.is_dir() {
                continue;
            }

            let source_name = source_path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();

            // Skip system directories
            if source_name.starts_with('.') ||
               source_name == "chunks" ||
               source_name == "temporal" ||
               source_name == "taxonomy" ||
               source_name == "storage" {
                continue;
            }

            // Look for database directories within each source
            for db_entry in std::fs::read_dir(&source_path)? {
                let db_entry = db_entry?;
                let db_path = db_entry.path();

                if !db_path.is_dir() {
                    continue;
                }

                // Check for manifest.json in this database directory
                let manifest_path = db_path.join("manifest.json");
                if manifest_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<crate::casg::TemporalManifest>(&content) {
                            let db_name = db_path.file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown");

                            let full_name = format!("{}/{}", source_name, db_name);

                            databases.push(DatabaseInfo {
                                name: full_name,
                                version: manifest.version,
                                created_at: manifest.created_at,
                                chunk_count: manifest.chunk_index.len(),
                                total_size: manifest.chunk_index.iter().map(|c| c.size).sum(),
                            });
                        }
                    }
                }
            }
        }

        Ok(databases)
    }

    /// Initialize temporal tracking for existing data
    pub fn init_temporal_for_existing(&mut self) -> Result<()> {
        let temporal_path = self.base_path.clone();
        let mut temporal_index = crate::casg::temporal::TemporalIndex::load(&temporal_path)?;

        // Check if temporal index is empty
        let history = temporal_index.get_version_history(1)?;
        if !history.is_empty() {
            // Already has history
            return Ok(());
        }

        // Check for existing manifest
        let root_manifest = self.base_path.join("manifest.json");
        if root_manifest.exists() {
            if let Ok(content) = std::fs::read_to_string(&root_manifest) {
                if let Ok(manifest) = serde_json::from_str::<crate::casg::TemporalManifest>(&content) {
                    // Add initial version to temporal index
                    temporal_index.add_sequence_version(
                        manifest.version.clone(),
                        manifest.sequence_root.clone(),
                        manifest.chunk_index.len(),
                        manifest.chunk_index.iter()
                            .map(|c| c.sequence_count)
                            .sum(),
                    )?;

                    // Save the temporal index
                    temporal_index.save()?;
                    println!("Initialized temporal tracking for existing database");
                }
            }
        }

        Ok(())
    }

    /// Get statistics for the CASG repository
    pub fn get_stats(&self) -> Result<CASGStats> {
        let storage_stats = self.repository.storage.get_stats();
        let databases = self.list_databases()?;

        Ok(CASGStats {
            total_chunks: storage_stats.total_chunks,
            total_size: storage_stats.total_size,
            compressed_chunks: storage_stats.compressed_chunks,
            deduplication_ratio: storage_stats.deduplication_ratio,
            database_count: databases.len(),
            databases,
        })
    }

    /// List all resumable operations
    pub fn list_resumable_operations(&self) -> Result<Vec<(String, crate::casg::ProcessingState)>> {
        self.repository.storage.list_resumable_operations()
    }

    /// Clean up expired processing states
    pub fn cleanup_expired_states(&self) -> Result<usize> {
        self.repository.storage.cleanup_expired_states()
    }

    /// Check for taxonomy updates and download if available
    pub async fn update_taxonomy(&mut self) -> Result<TaxonomyUpdateResult> {
        let taxonomy_dir = self.base_path.join("taxonomy");
        let taxdump_dir = taxonomy_dir.join("taxdump");
        let version_file = taxonomy_dir.join("version.json");

        // Read current version if it exists
        let current_version = if version_file.exists() {
            let content = std::fs::read_to_string(&version_file)?;
            let version_data: serde_json::Value = serde_json::from_str(&content)?;
            version_data["date"].as_str().map(|s| s.to_string())
        } else {
            None
        };

        // Check NCBI for latest taxonomy version
        // NCBI updates taxonomy weekly, we can check the timestamp
        let taxdump_url = "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz";

        // Do a HEAD request to check if there's an update
        let client = reqwest::Client::new();
        let response = client.head(taxdump_url).send().await?;

        // Get last modified date from headers
        let last_modified = response.headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Check if we need to update
        let needs_update = match (&current_version, &last_modified) {
            (Some(current), Some(latest)) => current != latest,
            (None, Some(_)) => true, // No current version, need to download
            _ => false, // Can't determine, assume no update needed
        };

        if !needs_update {
            return Ok(TaxonomyUpdateResult::UpToDate);
        }

        // Download new taxonomy
        println!("Downloading updated NCBI taxonomy...");
        let response = client.get(taxdump_url).send().await?;
        let bytes = response.bytes().await?;

        // Create backup of old taxonomy if it exists
        if taxdump_dir.exists() {
            let backup_dir = taxonomy_dir.join(format!("backup_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")));
            std::fs::rename(&taxdump_dir, backup_dir)?;
        }

        // Create new taxonomy directory
        std::fs::create_dir_all(&taxdump_dir)?;

        // Extract the tar.gz file
        let taxdump_file = taxdump_dir.join("taxdump.tar.gz");
        std::fs::write(&taxdump_file, bytes)?;

        use flate2::read::GzDecoder;
        use tar::Archive;
        let tar_gz = std::fs::File::open(&taxdump_file)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(&taxdump_dir)?;

        // Clean up tar file
        std::fs::remove_file(taxdump_file).ok();

        // Save version information
        let version_date = last_modified.clone().unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let version_data = serde_json::json!({
            "date": &version_date,
            "source": "NCBI",
            "updated_at": chrono::Utc::now().to_rfc3339()
        });
        std::fs::write(&version_file, serde_json::to_string_pretty(&version_data)?)?;

        // Reload taxonomy in repository
        self.repository.taxonomy.load_ncbi_taxonomy(&taxdump_dir)?;

        Ok(TaxonomyUpdateResult::Updated {
            old_version: current_version,
            new_version: last_modified,
        })
    }

    /// Get current taxonomy version
    pub fn get_taxonomy_version(&self) -> Result<Option<String>> {
        let version_file = self.base_path.join("taxonomy/version.json");
        if !version_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&version_file)?;
        let version_data: serde_json::Value = serde_json::from_str(&content)?;
        Ok(version_data["date"].as_str().map(|s| s.to_string()))
    }

    /// Assemble a FASTA file from CASG for a specific database
    pub fn assemble_database(&self, source: &DatabaseSource, output_path: &Path) -> Result<()> {
        // Load manifest for this database
        let manifest_path = self.get_manifest_path(source);
        if !manifest_path.exists() {
            anyhow::bail!("Database not found in CASG. Run download first.");
        }

        let manifest_content = std::fs::read_to_string(manifest_path)?;
        let manifest: crate::casg::TemporalManifest = serde_json::from_str(&manifest_content)?;

        // Get all chunk hashes
        let chunk_hashes: Vec<_> = manifest.chunk_index
            .iter()
            .map(|c| c.hash.clone())
            .collect();

        // Assemble to output file
        let assembler = crate::casg::FastaAssembler::new(&self.repository.storage);
        let mut output_file = std::fs::File::create(output_path)?;

        let sequence_count = assembler.stream_assembly(&chunk_hashes, &mut output_file)?;

        println!("Assembled {} sequences to {}", sequence_count, output_path.display());

        Ok(())
    }

    /// Assemble a taxonomic subset
    pub fn assemble_taxon(&self, taxon: &str, output_path: &Path) -> Result<()> {
        let sequences = self.repository.extract_taxon(taxon)?;

        // Write to FASTA
        use std::io::Write;
        let mut output = std::fs::File::create(output_path)?;

        for seq in sequences {
            writeln!(output, ">{}", seq.id)?;
            if let Some(desc) = seq.description {
                writeln!(output, " {}", desc)?;
            }
            writeln!(output, "{}", String::from_utf8_lossy(&seq.sequence))?;
        }

        Ok(())
    }

    /// Read sequences from a FASTA file
    fn read_fasta_sequences(&self, path: &Path) -> Result<Vec<Sequence>> {
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let reader = BufReader::new(file);

        // Create progress bar based on file size
        let progress = create_progress_bar(file_size, "Reading FASTA file");
        let mut bytes_read = 0u64;

        let mut sequences = Vec::new();
        let mut current_id = String::new();
        let mut current_desc = None;
        let mut current_seq = Vec::new();

        for line in reader.lines() {
            let line = line?;
            bytes_read += line.len() as u64 + 1; // +1 for newline
            progress.set_position(bytes_read);

            if line.starts_with('>') {
                // Save previous sequence if any
                if !current_id.is_empty() {
                    sequences.push(Sequence {
                        id: current_id.clone(),
                        description: current_desc.clone(),
                        sequence: current_seq.clone(),
                        taxon_id: None,
                    });
                }

                // Parse new header
                let header = &line[1..];
                let parts: Vec<&str> = header.splitn(2, ' ').collect();
                current_id = parts[0].to_string();
                current_desc = parts.get(1).map(|s| s.to_string());
                current_seq.clear();
            } else {
                // Append to sequence
                current_seq.extend(line.bytes());
            }
        }

        // Save last sequence
        if !current_id.is_empty() {
            sequences.push(Sequence {
                id: current_id,
                description: current_desc,
                sequence: current_seq,
                taxon_id: None,
            });
        }

        progress.finish_with_message(format!("Read {} sequences", sequences.len()));
        Ok(sequences)
    }
}

#[derive(Debug)]
pub enum DownloadResult {
    UpToDate,
    Updated {
        chunks_added: usize,
        chunks_removed: usize,
    },
    InitialDownload,
}

#[derive(Debug)]
pub struct DatabaseInfo {
    pub name: String,
    pub version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub chunk_count: usize,
    pub total_size: usize,
}

#[derive(Debug)]
pub struct CASGStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
    pub database_count: usize,
    pub databases: Vec<DatabaseInfo>,
}

#[derive(Debug)]
pub enum TaxonomyUpdateResult {
    UpToDate,
    Updated {
        old_version: Option<String>,
        new_version: Option<String>,
    },
}

#[cfg(test)]
#[path = "casg_database_manager_tests.rs"]
mod tests;