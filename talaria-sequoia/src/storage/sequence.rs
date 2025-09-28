/// Canonical sequence storage with cross-database deduplication
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::types::{
    CanonicalSequence, DatabaseSource, Representable, SHA256Hash,
    SequenceRepresentation, SequenceRepresentations, SequenceType,
};
use chrono::Utc;

/// Lightweight sequence information
pub struct SequenceInfo {
    pub id: String,
    pub hash: SHA256Hash,
    pub length: usize,
}

// Import the packed storage backend
use super::packed::PackedSequenceStorage;

/// Trait for sequence storage backends
pub trait SequenceStorageBackend: Send + Sync {
    /// Check if a canonical sequence exists
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool>;

    /// Store a canonical sequence
    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()>;

    /// Load a canonical sequence
    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence>;

    /// Store representations for a sequence
    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()>;

    /// Load representations for a sequence
    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations>;

    /// Get storage statistics
    fn get_stats(&self) -> Result<StorageStats>;

    /// List all sequence hashes in storage
    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>>;

    /// Get the size of a sequence
    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize>;

    /// Remove a sequence from storage
    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()>;

    /// Flush any pending writes to disk
    fn flush(&self) -> Result<()>;

    /// Get a reference to self as Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

// Use StorageStats from talaria-core
use talaria_core::StorageStats;

// FileSystemStorage removed - using PackedSequenceStorage only

// Helper functions
fn detect_sequence_type(sequence: &str) -> SequenceType {
    // Check if it's DNA/RNA (contains ATGCU) or protein
    let upper = sequence.to_uppercase();
    let nucleotide_chars = ['A', 'T', 'G', 'C', 'U', 'N'];

    let total_chars = upper.len();
    let nucleotide_count = upper.chars()
        .filter(|c| nucleotide_chars.contains(c))
        .count();

    // If > 90% are nucleotide characters, it's likely DNA/RNA
    if nucleotide_count as f32 / total_chars as f32 > 0.9 {
        if upper.contains('U') {
            SequenceType::RNA
        } else {
            SequenceType::DNA
        }
    } else {
        SequenceType::Protein
    }
}

fn compute_crc64(data: &[u8]) -> u64 {
    // Simple CRC64 implementation
    let mut crc = 0u64;
    for &byte in data {
        crc = crc.wrapping_add(byte as u64);
        crc = crc.wrapping_mul(0x100000001B3); // CRC64 polynomial
    }
    crc
}

fn extract_accessions_from_header(header: &str) -> Vec<String> {
    // Extract accessions from FASTA header
    let mut accessions = Vec::new();

    // Remove '>' if present
    let header = header.strip_prefix('>').unwrap_or(header);

    // First word is usually the primary accession
    if let Some(first_word) = header.split_whitespace().next() {
        // Handle different formats
        if first_word.contains('|') {
            let parts: Vec<&str> = first_word.split('|').collect();

            // Handle UniProt format: sp|P12345|PROT_HUMAN or tr|Q12345|...
            if parts.len() >= 2 && (parts[0] == "sp" || parts[0] == "tr") {
                accessions.push(parts[1].to_string());
                if parts.len() >= 3 {
                    accessions.push(parts[2].to_string());
                }
            }
            // Handle NCBI format: gi|123456|ref|NP_123456.1|
            else if parts.contains(&"ref") || parts.contains(&"gb") || parts.contains(&"emb") || parts.contains(&"dbj") {
                for (i, &part) in parts.iter().enumerate() {
                    if (part == "ref" || part == "gb" || part == "emb" || part == "dbj") && i + 1 < parts.len() {
                        // Get the accession, removing version if present
                        let acc = parts[i + 1].split('.').next().unwrap_or(parts[i + 1]);
                        accessions.push(acc.to_string());
                    }
                }
                // Also add gi number if present
                if parts.len() >= 2 && parts[0] == "gi" {
                    accessions.push(parts[1].to_string());
                }
            }
            // Generic pipe-separated: add all non-empty parts
            else {
                for part in parts {
                    if !part.is_empty() {
                        accessions.push(part.to_string());
                    }
                }
            }
        } else {
            // Simple accession (possibly with version)
            accessions.push(first_word.to_string());
            // Also add without version
            if let Some(acc_no_ver) = first_word.split('.').next() {
                if acc_no_ver != first_word {
                    accessions.push(acc_no_ver.to_string());
                }
            }
        }
    }

    accessions
}

#[allow(dead_code)]
fn extract_taxon_from_header(header: &str) -> Option<crate::types::TaxonId> {
    // Look for OX=##### pattern in header
    if let Some(ox_pos) = header.find("OX=") {
        let after_ox = &header[ox_pos + 3..];
        if let Some(end_pos) = after_ox.find(|c: char| !c.is_ascii_digit()) {
            if let Ok(taxon_id) = after_ox[..end_pos].parse::<u32>() {
                return Some(crate::types::TaxonId(taxon_id));
            }
        }
    }
    None
}

// Old FileSystemStorage implementation removed
/*
impl FileSystemStorage {
    pub fn new(base_path: &Path) -> Result<Self> {
        let sequences_dir = base_path.to_path_buf();
        let indices_dir = base_path.join("indices");

        fs::create_dir_all(&sequences_dir)?;
        fs::create_dir_all(&indices_dir)?;

        // Initialize or load bloom filter for fast existence checks
        let bloom_path = indices_dir.join("sequence_bloom.tal");
        let bloom_filter = if bloom_path.exists() {
            Self::load_bloom_filter(&bloom_path)?
        } else {
            // Create new bloom filter for ~10M sequences with 0.1% false positive rate
            BloomFilter::with_rate(0.001, 10_000_000)
        };

        Ok(Self {
            sequences_dir,
            indices_dir,
            bloom_filter: std::sync::RwLock::new(bloom_filter),
        })
    }

    /// Get the storage path for a canonical sequence
    fn get_sequence_path(&self, hash: &SHA256Hash) -> PathBuf {
        let hex = hash.to_hex();
        let dir1 = &hex[0..2];
        let dir2 = &hex[2..4];
        self.sequences_dir
            .join(dir1)
            .join(dir2)
            .join(format!("{}.seq", hex))
    }

    /// Get the storage path for sequence representations
    fn get_representations_path(&self, hash: &SHA256Hash) -> PathBuf {
        let hex = hash.to_hex();
        let dir1 = &hex[0..2];
        let dir2 = &hex[2..4];
        self.sequences_dir
            .join(dir1)
            .join(dir2)
            .join(format!("{}.reps", hex))
    }

    /// Load bloom filter from disk
    fn load_bloom_filter(path: &Path) -> Result<BloomFilter> {
        let _data = fs::read(path)?;
        // In a real implementation, we'd deserialize the bloom filter
        // For now, create a new one
        Ok(BloomFilter::with_rate(0.001, 10_000_000))
    }

    /// Save bloom filter to disk
    fn save_bloom_filter(&self) -> Result<()> {
        let bloom_path = self.indices_dir.join("sequence_bloom.tal");
        // In a real implementation, we'd serialize the bloom filter
        // For now, just touch the file
        fs::write(bloom_path, b"bloom")?;
        Ok(())
    }
}

impl SequenceStorageBackend for FileSystemStorage {
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        // Fast check with bloom filter first
        if !self.bloom_filter.read().unwrap().contains(&hash.to_hex()) {
            return Ok(false); // Definitely doesn't exist
        }

        // Bloom filter says maybe - check for real
        Ok(self.get_sequence_path(hash).exists())
    }

    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()> {
        let path = self.get_sequence_path(&sequence.sequence_hash);

        // Check if file already exists with identical content
        if path.exists() {
            // Try to load existing - if it matches, skip write
            if let Ok(existing) = self.load_canonical(&sequence.sequence_hash) {
                if existing.sequence == sequence.sequence
                    && existing.length == sequence.length
                    && existing.sequence_type == sequence.sequence_type {
                    // Content is identical - just ensure bloom filter is updated
                    self.bloom_filter.write().unwrap().insert(&sequence.sequence_hash.to_hex());
                    return Ok(()); // Skip write
                }
            }
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize and store
        let data = sequence.to_bytes()?;
        fs::write(&path, &data)?;

        // Update bloom filter
        self.bloom_filter.write().unwrap().insert(&sequence.sequence_hash.to_hex());
        self.save_bloom_filter()?;

        Ok(())
    }

    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        let path = self.get_sequence_path(hash);
        if !path.exists() {
            return Err(anyhow!("Sequence not found: {}", hash));
        }

        let data = fs::read(&path)?;
        CanonicalSequence::from_bytes(&data)
    }

    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()> {
        let path = self.get_representations_path(&representations.canonical_hash);

        // Check if file exists and load for comparison
        if path.exists() {
            if let Ok(existing) = self.load_representations(&representations.canonical_hash) {
                // Check if representations are identical
                if existing.representations.len() == representations.representations.len() {
                    let mut all_match = true;
                    for repr in &representations.representations {
                        if !existing.representations.iter().any(|e|
                            e.source == repr.source &&
                            e.header == repr.header &&
                            e.accessions == repr.accessions) {
                            all_match = false;
                            break;
                        }
                    }
                    if all_match {
                        return Ok(()); // Skip write - identical representations
                    }
                }
            }
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Serialize with MessagePack and compress with zstd
        let data = rmp_serde::to_vec(representations)?;
        let compressed = zstd::encode_all(&data[..], 3)?;
        fs::write(&path, &compressed)?;

        Ok(())
    }

    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        let path = self.get_representations_path(hash);

        if !path.exists() {
            // No representations yet - return empty
            return Ok(SequenceRepresentations {
                canonical_hash: hash.clone(),
                representations: Vec::new(),
            });
        }

        let compressed = fs::read(&path)?;
        let data = zstd::decode_all(&compressed[..])?;
        Ok(rmp_serde::from_slice(&data)?)
    }

    fn get_stats(&self) -> Result<StorageStats> {
        let mut total_sequences = 0;
        let mut total_representations = 0;
        let mut total_size = 0u64;

        // Walk the sequences directory
        for entry in walkdir::WalkDir::new(&self.sequences_dir)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    match ext.to_str() {
                        Some("seq") => {
                            total_sequences += 1;
                            if let Ok(metadata) = fs::metadata(path) {
                                total_size += metadata.len();
                            }
                        }
                        Some("reps") => {
                            total_representations += 1;
                            if let Ok(metadata) = fs::metadata(path) {
                                total_size += metadata.len();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Calculate deduplication ratio (representations per sequence)
        let deduplication_ratio = if total_sequences > 0 {
            total_representations as f32 / total_sequences as f32
        } else {
            0.0
        };

        Ok(StorageStats {
            total_sequences,
            total_representations,
            total_size,
            deduplication_ratio,
        })
    }
}
*/

/// Main sequence storage interface
pub struct SequenceStorage {
    pub(crate) backend: Box<dyn SequenceStorageBackend>,
    accession_index: RwLock<AccessionIndex>,
    taxonomy_index: RwLock<TaxonomyIndex>,
}

impl SequenceStorage {
    pub fn new(base_path: &Path) -> Result<Self> {
        let backend = Box::new(PackedSequenceStorage::new(base_path)?);
        let indices_dir = base_path.join("indices");

        let accession_index = AccessionIndex::load_or_create(&indices_dir)?;
        let taxonomy_index = TaxonomyIndex::load_or_create(&indices_dir)?;

        Ok(Self {
            backend,
            accession_index: RwLock::new(accession_index),
            taxonomy_index: RwLock::new(taxonomy_index),
        })
    }

    /// Store a sequence with its database-specific representation
    pub fn store_sequence(
        &self,
        sequence: &str,
        header: &str,
        source: DatabaseSource,
    ) -> Result<SHA256Hash> {
        // Step 1: Compute canonical hash (sequence only)
        let canonical_hash = SHA256Hash::compute(sequence.as_bytes());

        // Step 2: Store canonical sequence if new
        let is_new = !self.backend.sequence_exists(&canonical_hash)?;

        if is_new {
            let canonical = CanonicalSequence {
                sequence_hash: canonical_hash.clone(),
                sequence: sequence.as_bytes().to_vec(),
                length: sequence.len(),
                sequence_type: detect_sequence_type(sequence),
                checksum: compute_crc64(sequence.as_bytes()),
                first_seen: Utc::now(),
                last_seen: Utc::now(),
            };
            self.backend.store_canonical(&canonical)?;
        }

        // Step 3: Add database-specific representation
        let representation = SequenceRepresentation {
            source: source.clone(),
            header: header.to_string(),
            accessions: extract_accessions_from_header(header),
            description: extract_description(header),
            taxon_id: extract_taxon_id(header),
            metadata: parse_metadata(header),
            timestamp: Utc::now(),
        };

        // Load existing representations or create new
        let mut representations = self.backend.load_representations(&canonical_hash)?;

        // Update accession index
        for accession in &representation.accessions {
            self.accession_index.write().unwrap().insert(accession.clone(), canonical_hash.clone(), source.clone())?;
        }

        // Update taxonomy index
        if let Some(taxon_id) = representation.taxon_id {
            self.taxonomy_index.write().unwrap().insert(taxon_id, canonical_hash.clone())?;
        }

        // Add representation and save
        representations.add_representation(representation);
        self.backend.store_representations(&representations)?;

        // NOTE: Indices are saved in batch after all sequences are processed
        // to avoid 2N file writes for N sequences

        Ok(canonical_hash)
    }

    /// Save all indices to disk - call this after batch processing
    pub fn save_indices(&self) -> Result<()> {
        self.accession_index.write().unwrap().save()?;
        self.taxonomy_index.write().unwrap().save()?;
        Ok(())
    }

    /// Batch storage method for parallel processing
    pub fn store_sequences_batch(
        &self,
        sequences: Vec<(&str, &str, DatabaseSource)>,
    ) -> Result<Vec<(SHA256Hash, bool)>> {
        use dashmap::DashMap;
        use rayon::prelude::*;
        use std::sync::Arc;
        use std::collections::HashSet;

        // Pre-compute all hashes in parallel first
        let hashes_and_data: Vec<_> = sequences
            .par_iter()
            .map(|(sequence, header, source)| {
                let canonical_hash = SHA256Hash::compute(sequence.as_bytes());
                (sequence, header, source, canonical_hash)
            })
            .collect();

        // Batch check existence - parallel checking for performance
        let existing_hashes: HashSet<SHA256Hash> = {
            // Collect all hashes first
            let all_hashes: Vec<_> = hashes_and_data.iter()
                .map(|(_, _, _, hash)| hash.clone())
                .collect();

            // Check existence in parallel using rayon
            let existence_results: Vec<(SHA256Hash, bool)> = all_hashes
                .par_iter()
                .map(|hash| {
                    let exists = self.backend.sequence_exists(hash).unwrap_or(false);
                    (hash.clone(), exists)
                })
                .collect();

            // Build set of existing hashes
            existence_results.into_iter()
                .filter(|(_, exists)| *exists)
                .map(|(hash, _)| hash)
                .collect()
        };

        // Now create the final data with existence info
        let sequence_data: Vec<_> = hashes_and_data
            .into_iter()
            .map(|(sequence, header, source, hash)| {
                let is_new = !existing_hashes.contains(&hash);
                (sequence, header, source, hash, is_new)
            })
            .collect();

        // Group new sequences for batch writing
        let new_sequences: Vec<_> = sequence_data
            .iter()
            .filter(|(_, _, _, _, is_new)| *is_new)
            .map(|(sequence, _, _, hash, _)| {
                CanonicalSequence {
                    sequence_hash: hash.clone(),
                    sequence: sequence.as_bytes().to_vec(),
                    length: sequence.len(),
                    sequence_type: detect_sequence_type(sequence),
                    checksum: compute_crc64(sequence.as_bytes()),
                    first_seen: Utc::now(),
                    last_seen: Utc::now(),
                }
            })
            .collect();

        // Store all new canonical sequences in batch
        for canonical in &new_sequences {
            self.backend.store_canonical(&canonical)?;
        }

        // Group representations by hash
        let representations_map: Arc<DashMap<SHA256Hash, Vec<SequenceRepresentation>>> =
            Arc::new(DashMap::new());

        // Create all representations in parallel
        sequence_data
            .par_iter()
            .for_each(|(_, header, source, hash, _)| {
                let representation = SequenceRepresentation {
                    source: (*source).clone(),
                    header: header.to_string(),
                    accessions: extract_accessions_from_header(header),
                    description: extract_description(header),
                    taxon_id: extract_taxon_id(header),
                    metadata: parse_metadata(header),
                    timestamp: Utc::now(),
                };
                representations_map.entry(hash.clone()).or_default().push(representation);
            });

        // Load existing representations and merge
        for entry in representations_map.iter() {
            let (hash, new_reps) = entry.pair();
            let mut existing = self.backend.load_representations(hash)?;

            for rep in new_reps {
                // Update indices
                for accession in &rep.accessions {
                    self.accession_index.write().unwrap().insert(
                        accession.clone(),
                        hash.clone(),
                        rep.source.clone(),
                    )?;
                }

                if let Some(taxon_id) = rep.taxon_id {
                    self.taxonomy_index.write().unwrap().insert(taxon_id, hash.clone())?;
                }

                existing.add_representation(rep.clone());
            }

            self.backend.store_representations(&existing)?;
        }

        // Return results
        Ok(sequence_data
            .into_iter()
            .map(|(_, _, _, hash, is_new)| (hash, is_new))
            .collect())
    }

    /// Get sequence info without loading the full sequence
    pub fn get_sequence_info(&self, hash: &SHA256Hash) -> Result<SequenceInfo> {
        // Load canonical sequence to get basic info
        let canonical = self.backend.load_canonical(hash)?;

        // Load representations to get ID
        let representations = self.backend.load_representations(hash)?;

        // Get the first ID from accessions or header
        let id = representations.representations
            .first()
            .and_then(|r| r.accessions.first().cloned()
                .or_else(|| r.header.split_whitespace().next().map(|s| s.trim_start_matches('>').to_string())))
            .ok_or_else(|| anyhow!("No representations found for sequence"))?;

        Ok(SequenceInfo {
            id,
            hash: hash.clone(),
            length: canonical.length,
        })
    }

    /// Get a sequence with specific database formatting
    pub fn get_sequence_as_fasta(
        &self,
        hash: &SHA256Hash,
        preferred_source: Option<DatabaseSource>,
    ) -> Result<String> {
        // Load canonical sequence
        let canonical = self.backend.load_canonical(hash)?;

        // Load representations
        let representations = self.backend.load_representations(hash)?;

        // Choose best representation
        let repr = if let Some(source) = preferred_source {
            representations
                .get_representation(&source)
                .or_else(|| representations.representations().first())
        } else {
            representations.representations().first()
        }
        .ok_or_else(|| anyhow!("No representation found"))?;

        // Format as FASTA
        let sequence_str = String::from_utf8(canonical.sequence.clone())?;
        Ok(format!("{}\n{}", repr.header, sequence_str))
    }

    /// Find sequence by accession
    pub fn find_by_accession(&self, accession: &str) -> Result<Option<SHA256Hash>> {
        self.accession_index.read().unwrap().get(accession)
    }

    /// Find sequences by taxonomy
    pub fn find_by_taxon(&self, taxon_id: crate::types::TaxonId) -> Result<Vec<SHA256Hash>> {
        self.taxonomy_index.read().unwrap().get(taxon_id)
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats> {
        self.backend.get_stats()
    }

    /// Load a canonical sequence by hash
    pub fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        self.backend.load_canonical(hash)
    }

    /// Check if a canonical sequence exists
    pub fn canonical_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        self.backend.sequence_exists(hash)
    }

    /// Load representations for a sequence
    pub fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        self.backend.load_representations(hash)
    }

    /// List all sequence hashes
    pub fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>> {
        self.backend.list_all_hashes()
    }

    /// Get the size of a specific sequence
    pub fn get_size(&self, hash: &SHA256Hash) -> Result<usize> {
        self.backend.get_sequence_size(hash)
    }

    /// Remove a sequence from storage
    pub fn remove(&self, hash: &SHA256Hash) -> Result<()> {
        // Remove from indices
        self.accession_index.write().unwrap().remove_by_hash(hash);
        self.taxonomy_index.write().unwrap().remove_by_hash(hash);

        // Remove from backend
        self.backend.remove_sequence(hash)
    }

    /// Rebuild all indices by scanning the backend storage
    pub fn rebuild_index(&self) -> Result<()> {
        println!("Rebuilding sequence storage indices...");

        // First rebuild the backend's internal index
        if let Some(packed) = self.backend.as_any().downcast_ref::<PackedSequenceStorage>() {
            packed.rebuild_index()?;
        }

        // Clear existing secondary indices
        {
            let mut accession_idx = self.accession_index.write().unwrap();
            accession_idx.entries.clear();
        }
        {
            let mut taxonomy_idx = self.taxonomy_index.write().unwrap();
            taxonomy_idx.taxonomy_map.clear();
        }

        // Rebuild secondary indices from backend data
        let all_hashes = self.backend.list_all_hashes()?;
        println!("  Rebuilding secondary indices for {} sequences", all_hashes.len());

        for hash in &all_hashes {
            // Load representations to rebuild indices
            if let Ok(representations) = self.backend.load_representations(hash) {
                // Update accession index
                {
                    let mut accession_idx = self.accession_index.write().unwrap();
                    for repr in &representations.representations {
                        // Process all accessions for this representation
                        for accession in &repr.accessions {
                            accession_idx.insert(accession.clone(), hash.clone(), repr.source.clone())?;
                        }
                    }
                }

                // Update taxonomy index - check each representation for taxon_id
                for repr in &representations.representations {
                    if let Some(taxon_id) = repr.taxon_id {
                        let mut taxonomy_idx = self.taxonomy_index.write().unwrap();
                        taxonomy_idx.insert(taxon_id, hash.clone())?;
                    }
                }
            }
        }

        // Save rebuilt indices
        self.save_indices()?;
        println!("  Secondary indices rebuilt and saved");

        Ok(())
    }

    /// Flush any pending writes to disk
    pub fn flush(&self) -> Result<()> {
        // Flush the backend
        self.backend.flush()?;
        // Save indices
        self.save_indices()
    }
}

/// Index for accession to sequence hash mapping
struct AccessionIndex {
    index_path: PathBuf,
    entries: HashMap<String, AccessionEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AccessionEntry {
    sequence_hash: SHA256Hash,
    sources: Vec<DatabaseSource>,
}

impl AccessionIndex {
    fn load_or_create(indices_dir: &Path) -> Result<Self> {
        let index_path = indices_dir.join("accession_index.tal");

        let entries = if index_path.exists() {
            let data = fs::read(&index_path)?;
            let decompressed = zstd::decode_all(&data[..])?;
            rmp_serde::from_slice(&decompressed)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            index_path,
            entries,
        })
    }

    fn insert(&mut self, accession: String, hash: SHA256Hash, source: DatabaseSource) -> Result<()> {
        match self.entries.get_mut(&accession) {
            Some(entry) => {
                if !entry.sources.contains(&source) {
                    entry.sources.push(source);
                }
            }
            None => {
                self.entries.insert(
                    accession,
                    AccessionEntry {
                        sequence_hash: hash,
                        sources: vec![source],
                    },
                );
            }
        }
        Ok(())
    }

    fn get(&self, accession: &str) -> Result<Option<SHA256Hash>> {
        Ok(self.entries.get(accession).map(|e| e.sequence_hash.clone()))
    }

    fn save(&self) -> Result<()> {
        let data = rmp_serde::to_vec(&self.entries)?;
        let compressed = zstd::encode_all(&data[..], 3)?;
        fs::write(&self.index_path, compressed)?;
        Ok(())
    }

    fn remove_by_hash(&mut self, hash: &SHA256Hash) {
        // Remove all entries that map to this hash
        self.entries.retain(|_, entry| entry.sequence_hash != *hash);
    }
}

/// Index for taxonomy to sequence mapping
struct TaxonomyIndex {
    index_path: PathBuf,
    taxonomy_map: HashMap<crate::types::TaxonId, Vec<SHA256Hash>>,
}

impl TaxonomyIndex {
    fn load_or_create(indices_dir: &Path) -> Result<Self> {
        let index_path = indices_dir.join("taxonomy_index.tal");

        let taxonomy_map = if index_path.exists() {
            let data = fs::read(&index_path)?;
            let decompressed = zstd::decode_all(&data[..])?;
            rmp_serde::from_slice(&decompressed)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            index_path,
            taxonomy_map,
        })
    }

    fn insert(&mut self, taxon_id: crate::types::TaxonId, hash: SHA256Hash) -> Result<()> {
        self.taxonomy_map
            .entry(taxon_id)
            .or_insert_with(Vec::new)
            .push(hash);
        Ok(())
    }

    fn get(&self, taxon_id: crate::types::TaxonId) -> Result<Vec<SHA256Hash>> {
        Ok(self.taxonomy_map.get(&taxon_id).cloned().unwrap_or_default())
    }

    fn save(&self) -> Result<()> {
        let data = rmp_serde::to_vec(&self.taxonomy_map)?;
        let compressed = zstd::encode_all(&data[..], 3)?;
        fs::write(&self.index_path, compressed)?;
        Ok(())
    }

    fn remove_by_hash(&mut self, hash: &SHA256Hash) {
        // Remove this hash from all taxonomy mappings
        for (_, hashes) in self.taxonomy_map.iter_mut() {
            hashes.retain(|h| h != hash);
        }
        // Remove any empty entries
        self.taxonomy_map.retain(|_, hashes| !hashes.is_empty());
    }
}

fn extract_description(header: &str) -> Option<String> {
    // Get everything after the first space
    header.find(' ').map(|i| header[i + 1..].to_string())
}

fn extract_taxon_id(header: &str) -> Option<crate::types::TaxonId> {
    // Look for OX= pattern (UniProt)
    if let Some(ox_pos) = header.find("OX=") {
        let start = ox_pos + 3;
        let end = header[start..]
            .find(|c: char| !c.is_numeric())
            .map(|i| start + i)
            .unwrap_or(header.len());

        if let Ok(taxon_id) = header[start..end].parse::<u32>() {
            return Some(crate::types::TaxonId(taxon_id));
        }
    }

    // Look for TaxID= pattern
    if let Some(tax_pos) = header.find("TaxID=") {
        let start = tax_pos + 6;
        let end = header[start..]
            .find(|c: char| !c.is_numeric())
            .map(|i| start + i)
            .unwrap_or(header.len());

        if let Ok(taxon_id) = header[start..end].parse::<u32>() {
            return Some(crate::types::TaxonId(taxon_id));
        }
    }

    None
}

fn parse_metadata(header: &str) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    // Parse UniProt-style tags (OS=, GN=, PE=, SV=)
    for tag in &["OS=", "GN=", "PE=", "SV="] {
        if let Some(pos) = header.find(tag) {
            let start = pos + tag.len();
            // Find the end (next tag or end of string)
            let end = header[start..]
                .find(" OS=")
                .or_else(|| header[start..].find(" GN="))
                .or_else(|| header[start..].find(" PE="))
                .or_else(|| header[start..].find(" SV="))
                .or_else(|| header[start..].find(" OX="))
                .map(|i| start + i)
                .unwrap_or(header.len());

            let value = header[start..end].trim().to_string();
            metadata.insert(tag[..tag.len()-1].to_string(), value);
        }
    }

    metadata
}

// External dependencies we'll need to add to Cargo.toml:
// bloom = "0.3"
// walkdir = "2"
// zstd = "0.13"

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::types::DatabaseSource;

    #[test]
    fn test_canonical_sequence_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let seq_storage = SequenceStorage::new(temp_dir.path()).unwrap();

        // Store the same sequence twice
        let sequence = "ACGTACGTACGT";
        let header1 = ">seq1 description1";
        let header2 = ">seq2 different description";
        let source = DatabaseSource::Custom("custom/test".to_string());

        let hash1 = seq_storage.store_sequence(sequence, header1, source.clone()).unwrap();
        let hash2 = seq_storage.store_sequence(sequence, header2, source).unwrap();

        // Should get the same hash for identical sequences
        assert_eq!(hash1, hash2);

        // Save indices
        seq_storage.save_indices().unwrap();

        // Verify we can retrieve by both accessions
        assert!(seq_storage.find_by_accession("seq1").unwrap().is_some());
        assert!(seq_storage.find_by_accession("seq2").unwrap().is_some());
    }

    #[test]
    fn test_write_avoidance_optimization() {
        let temp_dir = TempDir::new().unwrap();
        let seq_storage = SequenceStorage::new(temp_dir.path()).unwrap();

        // Store a sequence
        let sequence = "ACGTACGTACGT";
        let header = ">test_seq test description";
        let source = DatabaseSource::Custom("custom/test".to_string());

        let hash = seq_storage.store_sequence(sequence, header, source.clone()).unwrap();
        seq_storage.save_indices().unwrap();

        // Get stats before re-storing
        let stats_before = seq_storage.get_stats().unwrap();

        // Store the same sequence again
        let hash2 = seq_storage.store_sequence(sequence, header, source).unwrap();
        assert_eq!(hash, hash2);

        // Get stats after re-storing
        let stats_after = seq_storage.get_stats().unwrap();

        // Verify that the sequence count didn't increase (deduplication worked)
        assert_eq!(
            stats_before.total_sequences,
            stats_after.total_sequences,
            "Sequence count should not increase for duplicate sequence"
        );
    }

    #[test]
    fn test_cross_database_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let seq_storage = SequenceStorage::new(temp_dir.path()).unwrap();

        let sequence = "MVALPRWFDK";

        // Store from UniProt
        let uniprot_source = DatabaseSource::UniProt(talaria_core::UniProtDatabase::SwissProt);
        let uniprot_header = ">sp|P12345|PROT_HUMAN Protein from human";
        let hash1 = seq_storage.store_sequence(sequence, uniprot_header, uniprot_source).unwrap();

        // Store from NCBI (same sequence)
        let ncbi_source = DatabaseSource::NCBI(talaria_core::NCBIDatabase::NR);
        let ncbi_header = ">gi|123456789|ref|NP_123456.1| protein [Homo sapiens]";
        let hash2 = seq_storage.store_sequence(sequence, ncbi_header, ncbi_source).unwrap();

        // Should be deduplicated
        assert_eq!(hash1, hash2);

        seq_storage.save_indices().unwrap();

        // Both accessions should exist
        // UniProt format extracts P12345 from sp|P12345|PROT_HUMAN
        assert!(seq_storage.find_by_accession("P12345").unwrap().is_some());
        // NCBI format extracts NP_123456 from gi|123456789|ref|NP_123456.1|
        assert!(seq_storage.find_by_accession("NP_123456").unwrap().is_some());
    }

    #[test]
    fn test_batch_index_saving() {
        let temp_dir = TempDir::new().unwrap();
        let seq_storage = SequenceStorage::new(temp_dir.path()).unwrap();

        let source = DatabaseSource::Custom("custom/test".to_string());

        // Store multiple sequences without saving indices each time
        for i in 0..100 {
            let sequence = format!("ACGT{}", "A".repeat(i));
            let header = format!(">seq{} description", i);
            seq_storage.store_sequence(&sequence, &header, source.clone()).unwrap();
        }

        // Save indices once at the end
        seq_storage.save_indices().unwrap();

        // Verify all sequences are accessible
        for i in 0..100 {
            let accession = format!("seq{}", i);
            assert!(
                seq_storage.find_by_accession(&accession).unwrap().is_some(),
                "Failed to find sequence {}",
                accession
            );
        }
    }
}
// rmp-serde = "1.1"