use crate::compression::{ChunkCompressor, CompressionConfig};
use crate::processing_state::{
    OperationType, ProcessingState, ProcessingStateManager, SourceInfo,
};
/// Content-addressed storage implementation for SEQUOIA
use crate::types::*;
use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Magic bytes for Talaria manifest format
const TALARIA_MAGIC: &[u8] = b"TAL\x01";

pub struct SEQUOIAStorage {
    pub(crate) base_path: PathBuf,
    chunk_index: Arc<DashMap<SHA256Hash, ChunkLocation>>,
    _index_lock: Arc<Mutex<()>>,
    state_manager: Arc<Mutex<ProcessingStateManager>>,
    current_operation_id: Arc<Mutex<Option<String>>>,
    compressor: Arc<Mutex<ChunkCompressor>>,
}

#[derive(Debug, Clone)]
struct ChunkLocation {
    path: PathBuf,
    compressed: bool,
    size: usize,
    format: ChunkFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeltaIndexEntry {
    child_id: String,
    delta_chunk_hash: SHA256Hash,
    reference_chunk_hash: SHA256Hash,
    reference_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeltaIndexEntryV2 {
    sequence_id: String,
    delta_chunk_hash: SHA256Hash,
    reference_hash: SHA256Hash,
    chunk_type: ChunkType,
    compression_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkMetadataExtended {
    hash: SHA256Hash,
    chunk_type: ChunkType,
    reference_hash: Option<SHA256Hash>,
    compression_ratio: Option<f32>,
    taxon_ids: Vec<TaxonId>,
}

impl SEQUOIAStorage {
    pub fn new(base_path: &Path) -> Result<Self> {
        let chunks_dir = base_path.join("chunks");
        fs::create_dir_all(&chunks_dir).context("Failed to create chunks directory")?;

        let state_manager = ProcessingStateManager::new(base_path)?;
        let compression_config = CompressionConfig::default();
        let compressor = ChunkCompressor::new(compression_config);

        Ok(Self {
            base_path: base_path.to_path_buf(),
            chunk_index: Arc::new(DashMap::new()),
            _index_lock: Arc::new(Mutex::new(())),
            state_manager: Arc::new(Mutex::new(state_manager)),
            current_operation_id: Arc::new(Mutex::new(None)),
            compressor: Arc::new(Mutex::new(compressor)),
        })
    }

    pub fn open(base_path: &Path) -> Result<Self> {
        let mut storage = Self::new(base_path)?;
        storage.rebuild_index()?;
        Ok(storage)
    }

    /// Rebuild the chunk index from disk
    fn rebuild_index(&mut self) -> Result<()> {
        let chunks_dir = self.base_path.join("chunks");
        // Rebuild index from chunks directory
        for entry in fs::read_dir(&chunks_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(hash_str) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(hash) = SHA256Hash::from_hex(hash_str) {
                        let metadata = fs::metadata(&path)?;

                        // Detect format from extension or file contents
                        let format = if path.extension().map(|e| e == "gz").unwrap_or(false) {
                            ChunkFormat::JsonGzip
                        } else if path.extension().map(|e| e == "zst").unwrap_or(false) {
                            // Try to detect if it's dictionary-compressed
                            // For now, assume regular Binary format
                            ChunkFormat::Binary
                        } else {
                            // Legacy uncompressed (shouldn't exist, but handle it)
                            ChunkFormat::JsonGzip
                        };

                        let compressed = !matches!(format, ChunkFormat::JsonGzip);

                        self.chunk_index.insert(
                            hash.clone(),
                            ChunkLocation {
                                path: path.clone(),
                                compressed,
                                size: metadata.len() as usize,
                                format,
                            },
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Store a chunk in content-addressed storage
    pub fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash> {
        let hash = SHA256Hash::compute(data);

        // Check if already stored (deduplication)
        if self.chunk_index.contains_key(&hash) {
            return Ok(hash);
        }

        // Use new compression if requested
        let (format, compressed_data, extension) = if compress {
            let format = ChunkFormat::default(); // Binary format
            let mut compressor = self.compressor.lock().unwrap();
            let compressed = compressor.compress(data, format, None)?;
            let ext = match format {
                ChunkFormat::JsonGzip => ".gz",
                ChunkFormat::Binary => ".zst",
                ChunkFormat::BinaryDict { .. } => ".dict.zst",
            };
            (format, compressed, ext)
        } else {
            // Store uncompressed (shouldn't happen normally)
            (ChunkFormat::JsonGzip, data.to_vec(), "")
        };

        let chunk_path =
            self.base_path
                .join("chunks")
                .join(format!("{}{}", hash.to_hex(), extension));

        fs::write(&chunk_path, &compressed_data).context("Failed to write chunk")?;

        self.chunk_index.insert(
            hash.clone(),
            ChunkLocation {
                path: chunk_path,
                compressed: compress,
                size: compressed_data.len(),
                format,
            },
        );

        Ok(hash)
    }

    /// Retrieve a chunk from storage
    pub fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        let location_ref = self
            .chunk_index
            .get(hash)
            .ok_or_else(|| anyhow::anyhow!("Chunk not found: {}", hash))?;
        let location = location_ref.value();

        let compressed_data = fs::read(&location.path).context("Failed to read chunk file")?;

        // Decompress based on format
        let compressor = self.compressor.lock().unwrap();
        compressor.decompress(&compressed_data, Some(location.format))
    }

    /// Check if a chunk exists
    pub fn has_chunk(&self, hash: &SHA256Hash) -> bool {
        self.chunk_index.contains_key(hash)
    }

    /// Load sequences from a chunk
    pub fn load_sequences_from_chunk(
        &self,
        hash: &SHA256Hash,
    ) -> Result<Vec<talaria_bio::sequence::Sequence>> {
        // Get the chunk data
        let chunk_data = self.get_chunk(hash)?;

        // Parse as FASTA sequences
        Ok(talaria_bio::fasta::parse_fasta_from_bytes(&chunk_data)?)
    }

    /// Get path for a chunk
    fn get_chunk_path(&self, hash: &SHA256Hash, compressed: bool) -> PathBuf {
        let chunks_dir = self.base_path.join("chunks");
        let filename = if compressed {
            format!("{}.gz", hash.to_hex())
        } else {
            hash.to_hex()
        };
        chunks_dir.join(filename)
    }

    /// Store a taxonomy-aware chunk
    pub fn store_taxonomy_chunk(&self, chunk: &TaxonomyAwareChunk) -> Result<SHA256Hash> {
        let chunk_hash = chunk.content_hash.clone();

        // Use new compression format
        let format = ChunkFormat::default(); // Uses Binary format

        // For Binary format (default), use direct zstd compression without mutex
        // This allows true parallel compression without contention
        let compressed_data = match format {
            ChunkFormat::Binary => {
                // Direct zstd compression - no mutex needed!
                let serialized = serde_json::to_vec(chunk)?;
                zstd::encode_all(&serialized[..], 19)
                    .context("Failed to compress chunk with zstd")?
            }
            _ => {
                // For other formats (JsonGzip or BinaryDict), use the compressor
                // These are rarely used, so mutex is acceptable
                let taxon_id = chunk.taxon_ids.first().copied();
                let mut compressor = self.compressor.lock().unwrap();
                compressor.compress(&serde_json::to_vec(chunk)?, format, taxon_id.map(|t| t.0))?
            }
        };

        // Determine file extension based on format
        let extension = match format {
            ChunkFormat::JsonGzip => ".gz",
            ChunkFormat::Binary => ".zst",
            ChunkFormat::BinaryDict { .. } => ".dict.zst",
        };

        // Create the chunk file path with appropriate extension
        let chunk_path =
            self.base_path
                .join("chunks")
                .join(format!("{}{}", chunk_hash.to_hex(), extension));

        // Write compressed data
        fs::write(&chunk_path, &compressed_data).context("Failed to write compressed chunk")?;

        // Update chunk index
        self.chunk_index.insert(
            chunk_hash.clone(),
            ChunkLocation {
                path: chunk_path,
                compressed: true,
                size: compressed_data.len(),
                format,
            },
        );

        // NOTE: chunk_index.json removed - it was redundant, mapping SHA to "chunks/SHA"
        // The in-memory chunk_index (DashMap) already tracks what we need with ChunkLocation
        // and rebuild_index() reconstructs it from the filesystem on startup.
        //
        // Future considerations for a persistent index file:
        // 1. Store full ChunkLocation metadata (path, compressed flag, size) to avoid filesystem scanning
        // 2. Track chunk access patterns for cache optimization
        // 3. Include chunk type metadata (reference vs delta chunks)
        // 4. Store taxonomy information for quick taxon-based queries
        // 5. Use binary format (MessagePack/bincode) for better performance with large chunk counts
        //
        // If implementing, consider:
        // - Incremental updates vs full rewrites
        // - Concurrent access handling
        // - Recovery from corrupted index
        // - Migration from filesystem-only to indexed approach

        Ok(chunk_hash)
    }

    /// Fetch chunks from remote repository with resume support
    pub async fn fetch_chunks(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<TaxonomyAwareChunk>> {
        self.fetch_chunks_with_resume(hashes, false).await
    }

    /// Fetch chunks with explicit resume control
    pub async fn fetch_chunks_with_resume(
        &mut self,
        hashes: &[SHA256Hash],
        check_resume: bool,
    ) -> Result<Vec<TaxonomyAwareChunk>> {
        use futures::stream::{self, StreamExt};

        let mut chunks = Vec::new();

        // Check for existing processing state if requested
        let mut already_completed = HashSet::new();
        if check_resume {
            if let Some(state) = self.get_current_state()? {
                already_completed = state.completed_chunks.clone();
                eprintln!("Resuming operation: {}", state.summary());
            }
        }

        // Filter out already completed chunks and locally available chunks
        let missing_hashes: Vec<_> = hashes
            .iter()
            .filter(|h| !already_completed.contains(h) && !self.has_chunk(h))
            .cloned()
            .collect();

        if missing_hashes.is_empty() {
            // All chunks already available locally or completed
            for hash in hashes {
                let data = self.get_chunk(hash)?;
                let chunk: TaxonomyAwareChunk = serde_json::from_slice(&data)?;
                chunks.push(chunk);
            }
            return Ok(chunks);
        }

        eprintln!(
            "Need to fetch {} chunks (already have {} locally, {} from previous run)",
            missing_hashes.len(),
            hashes.len() - missing_hashes.len() - already_completed.len(),
            already_completed.len()
        );

        // Fetch missing chunks in parallel (up to 4 concurrent)
        let base_path = self.base_path.clone();
        let mut successfully_fetched = Vec::new();

        let fetch_futures = missing_hashes.iter().map(|hash| {
            let hash_clone = hash.clone();
            let base_path_clone = base_path.clone();
            async move { Self::fetch_single_chunk_static(&hash_clone, &base_path_clone).await }
        });

        let mut fetch_stream = stream::iter(fetch_futures).buffer_unordered(4);

        let mut fetch_count = 0;
        let total_to_fetch = missing_hashes.len();

        while let Some(result) = fetch_stream.next().await {
            match result {
                Ok((hash, data)) => {
                    // Store fetched chunk locally
                    self.store_chunk(&data, true)?;
                    successfully_fetched.push(hash.clone());
                    fetch_count += 1;

                    eprintln!(
                        "[{}/{}] Fetched and stored chunk: {}",
                        fetch_count, total_to_fetch, hash
                    );

                    // Update processing state periodically (every 10 chunks)
                    if successfully_fetched.len() % 10 == 0 {
                        self.update_processing_state(&successfully_fetched)?;
                        successfully_fetched.clear();
                    }
                }
                Err(e) => {
                    eprintln!("Failed to fetch chunk: {}", e);
                }
            }
        }

        // Update state with remaining successfully fetched chunks
        if !successfully_fetched.is_empty() {
            self.update_processing_state(&successfully_fetched)?;
        }

        // Now load all requested chunks
        for hash in hashes {
            let data = self.get_chunk(hash)?;
            let chunk: TaxonomyAwareChunk = serde_json::from_slice(&data)?;
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    /// Fetch a single chunk from remote repository (static version for async)
    async fn fetch_single_chunk_static(
        hash: &SHA256Hash,
        _base_path: &PathBuf,
    ) -> Result<(SHA256Hash, Vec<u8>)> {
        // Configuration for remote repository
        let remote_base = std::env::var("TALARIA_REMOTE_REPO")
            .unwrap_or_else(|_| "https://sequoia.talaria.org".to_string());

        let chunk_url = format!("{}/chunks/{}", remote_base, hash.to_hex());

        // Use reqwest for HTTP fetching (would need to add as dependency)
        // For now, simulate with local filesystem fallback
        let remote_path =
            std::path::PathBuf::from("/tmp/talaria-remote-repo/chunks").join(hash.to_hex());

        if remote_path.exists() {
            let data = tokio::fs::read(&remote_path).await?;

            // Verify hash matches
            let computed_hash = SHA256Hash::compute(&data);
            if &computed_hash != hash {
                return Err(anyhow::anyhow!(
                    "Hash mismatch for chunk {}: expected {}, got {}",
                    hash,
                    hash,
                    computed_hash
                ));
            }

            Ok((hash.clone(), data))
        } else {
            // In production, would use HTTP client here
            Err(anyhow::anyhow!(
                "Remote chunk not available: {} (would fetch from {})",
                hash,
                chunk_url
            ))
        }
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> StorageStats {
        let total_chunks = self.chunk_index.len();
        let total_size: usize = self
            .chunk_index
            .iter()
            .map(|entry| entry.value().size)
            .sum();
        let compressed_chunks = self
            .chunk_index
            .iter()
            .filter(|entry| entry.value().compressed)
            .count();

        StorageStats {
            total_chunks,
            total_size,
            compressed_chunks,
            deduplication_ratio: self.calculate_dedup_ratio(),
        }
    }

    /// Get sequence root hash
    pub fn get_sequence_root(&self) -> Result<crate::MerkleHash> {
        use crate::merkle::MerkleDAG;

        // Collect all chunk metadata in sorted order for deterministic root
        let mut chunk_metadata: Vec<ChunkMetadata> = self
            .chunk_index
            .iter()
            .map(|entry| {
                let location = entry.value();
                ChunkMetadata {
                    hash: entry.key().clone(),
                    taxon_ids: Vec::new(), // Would need to be loaded from chunk
                    sequence_count: 0,     // Would need to be loaded from chunk
                    size: location.size,
                    compressed_size: if location.compressed {
                        Some(location.size)
                    } else {
                        None
                    },
                }
            })
            .collect();
        chunk_metadata.sort_by(|a, b| a.hash.0.cmp(&b.hash.0));

        // Build Merkle tree from chunk metadata
        let dag = MerkleDAG::build_from_items(chunk_metadata)?;

        dag.root_hash()
            .ok_or_else(|| anyhow::anyhow!("No chunks in storage"))
    }

    fn calculate_dedup_ratio(&self) -> f32 {
        // Track reference counts for each chunk
        let mut reference_counts = HashMap::new();

        // Count references from chunk index entries
        for entry in self.chunk_index.iter() {
            *reference_counts.entry(entry.key().clone()).or_insert(0) += 1;
        }

        // Count references from delta chunks if we track them
        let delta_index_path = self.base_path.join("delta_index.json");
        if delta_index_path.exists() {
            if let Ok(content) = fs::read_to_string(&delta_index_path) {
                if let Ok(entries) = serde_json::from_str::<Vec<DeltaIndexEntry>>(&content) {
                    for entry in entries {
                        *reference_counts.entry(entry.delta_chunk_hash).or_insert(0) += 1;
                        *reference_counts
                            .entry(entry.reference_chunk_hash)
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        // Calculate average references per chunk
        if reference_counts.is_empty() {
            return 1.0;
        }

        let total_refs: usize = reference_counts.values().sum();
        let unique_chunks = reference_counts.len();

        if unique_chunks == 0 {
            1.0
        } else {
            total_refs as f32 / unique_chunks as f32
        }
    }

    /// Enumerate all chunks in storage
    pub fn enumerate_chunks(&self) -> Vec<ChunkInfo> {
        self.chunk_index
            .iter()
            .map(|entry| ChunkInfo {
                hash: entry.key().clone(),
                path: entry.value().path.clone(),
                size: entry.value().size,
                compressed: entry.value().compressed,
                format: entry.value().format,
            })
            .collect()
    }

    /// Enumerate chunks with filtering
    pub fn enumerate_chunks_filtered<F>(&self, filter: F) -> Vec<ChunkInfo>
    where
        F: Fn(&SHA256Hash) -> bool,
    {
        self.chunk_index
            .iter()
            .filter(|entry| filter(entry.key()))
            .map(|entry| ChunkInfo {
                hash: entry.key().clone(),
                path: entry.value().path.clone(),
                size: entry.value().size,
                compressed: entry.value().compressed,
                format: entry.value().format,
            })
            .collect()
    }

    /// Store a delta chunk with type information
    pub fn store_delta_chunk(&self, chunk: &DeltaChunk) -> Result<SHA256Hash> {
        // Serialize the delta chunk
        let chunk_data = serde_json::to_vec(chunk)?;
        let chunk_hash = chunk.content_hash.clone();

        // Store with chunk type metadata
        let metadata = ChunkMetadataExtended {
            hash: chunk_hash.clone(),
            chunk_type: chunk.chunk_type.clone(),
            reference_hash: Some(chunk.reference_hash.clone()),
            compression_ratio: Some(chunk.compression_ratio),
            taxon_ids: chunk.taxon_ids.clone(),
        };

        // Store metadata separately for quick lookups
        let metadata_path = self
            .base_path
            .join("metadata")
            .join(format!("{}.meta", chunk_hash.to_hex()));
        fs::create_dir_all(metadata_path.parent().unwrap())?;
        fs::write(&metadata_path, serde_json::to_vec(&metadata)?)?;

        // Store the chunk data (compressed if beneficial)
        let compress = chunk.compression_ratio < 0.9;
        self.store_chunk(&chunk_data, compress)?;

        // Update delta index
        self.update_delta_index(chunk)?;

        Ok(chunk_hash)
    }

    /// Retrieve a delta chunk
    pub fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<DeltaChunk> {
        let data = self.get_chunk(hash)?;
        let chunk: DeltaChunk = serde_json::from_slice(&data)?;
        Ok(chunk)
    }

    /// Update delta index for a new delta chunk
    fn update_delta_index(&self, chunk: &DeltaChunk) -> Result<()> {
        let index_path = self.base_path.join("delta_index_v2.json");

        let mut index: HashMap<String, DeltaIndexEntryV2> = if index_path.exists() {
            serde_json::from_str(&fs::read_to_string(&index_path)?)?
        } else {
            HashMap::new()
        };

        // Index each sequence in the delta chunk
        for seq_ref in &chunk.sequences {
            let entry = DeltaIndexEntryV2 {
                sequence_id: seq_ref.sequence_id.clone(),
                delta_chunk_hash: chunk.content_hash.clone(),
                reference_hash: chunk.reference_hash.clone(),
                chunk_type: chunk.chunk_type.clone(),
                compression_ratio: chunk.compression_ratio,
            };
            index.insert(seq_ref.sequence_id.clone(), entry);
        }

        fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
        Ok(())
    }

    /// Add entry to legacy delta index
    #[allow(dead_code)]
    fn add_delta_index_entry(&self, entry: DeltaIndexEntry) -> Result<()> {
        let index_path = self.base_path.join("delta_index.json");

        let mut entries: Vec<DeltaIndexEntry> = if index_path.exists() {
            serde_json::from_str(&fs::read_to_string(&index_path)?)?
        } else {
            Vec::new()
        };

        entries.push(entry);
        fs::write(&index_path, serde_json::to_string_pretty(&entries)?)?;
        Ok(())
    }

    /// Find delta chunks for a reference
    pub fn find_delta_chunks_for_reference(
        &self,
        reference_hash: &SHA256Hash,
    ) -> Result<Vec<SHA256Hash>> {
        let index_path = self.base_path.join("delta_index_v2.json");

        if !index_path.exists() {
            return Ok(Vec::new());
        }

        let index: HashMap<String, DeltaIndexEntryV2> =
            serde_json::from_str(&fs::read_to_string(&index_path)?)?;

        let chunks: HashSet<SHA256Hash> = index
            .values()
            .filter(|entry| entry.reference_hash == *reference_hash)
            .map(|entry| entry.delta_chunk_hash.clone())
            .collect();

        Ok(chunks.into_iter().collect())
    }

    /// Get chunk type for a hash
    pub fn get_chunk_type(&self, hash: &SHA256Hash) -> Result<ChunkType> {
        let metadata_path = self
            .base_path
            .join("metadata")
            .join(format!("{}.meta", hash.to_hex()));

        if metadata_path.exists() {
            let metadata: ChunkMetadataExtended =
                serde_json::from_slice(&fs::read(&metadata_path)?)?;
            Ok(metadata.chunk_type)
        } else {
            // Default to full chunk if no metadata
            Ok(ChunkType::Full)
        }
    }

    /// Garbage collect unreferenced delta chunks
    pub fn garbage_collect_deltas(&self) -> Result<GarbageCollectionStats> {
        let mut stats = GarbageCollectionStats::default();

        // Build reference graph
        let (referenced_chunks, orphaned_chunks) = self.build_reference_graph()?;

        // Delete orphaned delta chunks
        for chunk_hash in orphaned_chunks {
            if let Ok(chunk_type) = self.get_chunk_type(&chunk_hash) {
                if matches!(chunk_type, ChunkType::Delta { .. }) {
                    // Remove chunk file
                    let chunk_path = self.get_chunk_path(&chunk_hash, false);
                    let gz_path = self.get_chunk_path(&chunk_hash, true);

                    if chunk_path.exists() {
                        fs::remove_file(&chunk_path)?;
                        stats.chunks_deleted += 1;
                        stats.bytes_freed += fs::metadata(&chunk_path)
                            .map(|m| m.len() as usize)
                            .unwrap_or(0);
                    }

                    if gz_path.exists() {
                        fs::remove_file(&gz_path)?;
                        stats.chunks_deleted += 1;
                        stats.bytes_freed += fs::metadata(&gz_path)
                            .map(|m| m.len() as usize)
                            .unwrap_or(0);
                    }

                    // Remove from index
                    self.chunk_index.remove(&chunk_hash);

                    // Remove metadata
                    let metadata_path = self
                        .base_path
                        .join("metadata")
                        .join(format!("{}.meta", chunk_hash.to_hex()));
                    if metadata_path.exists() {
                        fs::remove_file(&metadata_path)?;
                    }
                }
            }
        }

        // Compact delta chains that are too deep
        stats.chains_compacted = self.compact_deep_chains(&referenced_chunks)?;

        Ok(stats)
    }

    /// Build reference graph to identify orphaned chunks
    fn build_reference_graph(&self) -> Result<(HashSet<SHA256Hash>, HashSet<SHA256Hash>)> {
        let mut referenced = HashSet::new();
        let mut all_chunks = HashSet::new();

        // Collect all chunks
        for entry in self.chunk_index.iter() {
            all_chunks.insert(entry.key().clone());
        }

        // Load manifest (try .tal first, then .json for debugging)
        let manifest_path_tal = self.base_path.join("manifest.tal");
        let manifest_path_json = self.base_path.join("manifest.json");

        let manifest = if manifest_path_tal.exists() {
            let mut data = fs::read(&manifest_path_tal)?;
            // Skip magic header
            if data.starts_with(TALARIA_MAGIC) {
                data = data[TALARIA_MAGIC.len()..].to_vec();
            }
            rmp_serde::from_slice::<TemporalManifest>(&data).ok()
        } else if manifest_path_json.exists() {
            let data = fs::read_to_string(&manifest_path_json)?;
            serde_json::from_str::<TemporalManifest>(&data).ok()
        } else {
            None
        };

        if let Some(manifest) = manifest {
            // Add all chunks referenced in manifest
            for chunk_meta in &manifest.chunk_index {
                referenced.insert(chunk_meta.hash.clone());
            }
        }

        // Load delta index to find delta references
        let delta_index_path = self.base_path.join("delta_index_v2.json");
        if delta_index_path.exists() {
            let index: HashMap<String, DeltaIndexEntryV2> =
                serde_json::from_str(&fs::read_to_string(&delta_index_path)?)?;

            for entry in index.values() {
                referenced.insert(entry.delta_chunk_hash.clone());
                referenced.insert(entry.reference_hash.clone());
            }
        }

        // Find orphaned chunks (in storage but not referenced)
        let orphaned: HashSet<_> = all_chunks.difference(&referenced).cloned().collect();

        Ok((referenced, orphaned))
    }

    /// Compact delta chains that exceed maximum depth
    fn compact_deep_chains(&self, referenced_chunks: &HashSet<SHA256Hash>) -> Result<usize> {
        let mut chains_compacted = 0;

        // Load delta index to analyze chains
        let delta_index_path = self.base_path.join("delta_index_v2.json");
        if !delta_index_path.exists() {
            return Ok(0);
        }

        let index: HashMap<String, DeltaIndexEntryV2> =
            serde_json::from_str(&fs::read_to_string(&delta_index_path)?)?;

        // Build chain depth map
        let mut chain_depths: HashMap<SHA256Hash, usize> = HashMap::new();
        for entry in index.values() {
            // Simple depth calculation - in real implementation would traverse chain
            let depth = chain_depths.get(&entry.reference_hash).unwrap_or(&0) + 1;
            chain_depths.insert(entry.delta_chunk_hash.clone(), depth);
        }

        // Identify chains that need compaction (depth > 3)
        for (chunk_hash, depth) in chain_depths {
            if depth > 3 && referenced_chunks.contains(&chunk_hash) {
                // In a real implementation, we would:
                // 1. Load the delta chunk
                // 2. Reconstruct the full sequence
                // 3. Create a new delta directly from root reference
                // 4. Replace the deep delta with the shallow one
                chains_compacted += 1;

                tracing::info!(
                    "Would compact delta chain {} with depth {}",
                    chunk_hash.to_hex(),
                    depth
                );
            }
        }

        Ok(chains_compacted)
    }

    /// Get chunk metadata
    pub fn get_chunk_info(&self, hash: &SHA256Hash) -> Option<ChunkInfo> {
        self.chunk_index.get(hash).map(|entry| ChunkInfo {
            hash: hash.clone(),
            path: entry.value().path.clone(),
            size: entry.value().size,
            compressed: entry.value().compressed,
            format: entry.value().format,
        })
    }

    /// Verify integrity of all stored chunks
    pub fn verify_all(&self) -> Result<Vec<VerificationError>> {
        let mut errors = Vec::new();

        for entry in self.chunk_index.iter() {
            let expected_hash = entry.key();
            match self.get_chunk(expected_hash) {
                Ok(data) => {
                    let actual_hash = SHA256Hash::compute(&data);
                    if &actual_hash != expected_hash {
                        errors.push(VerificationError {
                            chunk_hash: expected_hash.clone(),
                            error_type: VerificationErrorType::HashMismatch {
                                expected: expected_hash.clone(),
                                actual: actual_hash,
                            },
                        });
                    }
                }
                Err(e) => {
                    errors.push(VerificationError {
                        chunk_hash: expected_hash.clone(),
                        error_type: VerificationErrorType::ReadError(e.to_string()),
                    });
                }
            }
        }

        Ok(errors)
    }

    /// Garbage collect unreferenced chunks
    pub fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<GCResult> {
        use std::collections::HashSet;

        let referenced_set: HashSet<_> = referenced.iter().cloned().collect();
        let mut removed_count = 0;
        let mut freed_space = 0;

        let chunks_to_remove: Vec<_> = self
            .chunk_index
            .iter()
            .filter(|entry| !referenced_set.contains(entry.key()))
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        for (hash, location) in chunks_to_remove {
            freed_space += location.size;
            fs::remove_file(&location.path)?;
            self.chunk_index.remove(&hash);
            removed_count += 1;
        }

        Ok(GCResult {
            removed_count,
            freed_space,
        })
    }

    /// Get all chunk hashes in storage
    pub fn get_all_chunk_hashes(&self) -> Vec<SHA256Hash> {
        self.chunk_index
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Store raw chunk data (for manifests, etc.)
    pub fn store_raw_chunk(&self, hash: &SHA256Hash, data: Vec<u8>) -> Result<()> {
        // Verify the hash matches
        let computed_hash = SHA256Hash::compute(&data);
        if computed_hash != *hash {
            return Err(anyhow::anyhow!(
                "Hash mismatch: expected {}, got {}",
                hash,
                computed_hash
            ));
        }

        // Store the chunk
        self.store_chunk(&data, true)?;
        Ok(())
    }

    /// Find the delta chunk containing a specific child sequence
    pub fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>> {
        let index_path = self
            .base_path
            .join("delta_index")
            .join(format!("{}.idx", child_id));

        if !index_path.exists() {
            return Ok(None);
        }

        let index_data = fs::read(&index_path)?;
        let index_entry: DeltaIndexEntry = serde_json::from_slice(&index_data)?;
        Ok(Some(index_entry.delta_chunk_hash))
    }

    /// Get all delta chunks for a reference chunk
    pub fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>> {
        let index_dir = self.base_path.join("delta_index");
        let mut delta_hashes = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if index_dir.exists() {
            for entry in fs::read_dir(&index_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension() == Some(std::ffi::OsStr::new("idx")) {
                    let index_data = fs::read(&path)?;
                    let index_entry: DeltaIndexEntry = serde_json::from_slice(&index_data)?;

                    if index_entry.reference_chunk_hash == *reference_hash
                        && seen.insert(index_entry.delta_chunk_hash.clone()) {
                            delta_hashes.push(index_entry.delta_chunk_hash);
                        }
                }
            }
        }

        Ok(delta_hashes)
    }

    /// Store a reduction manifest (deprecated - use store_database_reduction_manifest)
    pub fn store_reduction_manifest(
        &self,
        manifest: &crate::reduction::ReductionManifest,
    ) -> Result<SHA256Hash> {
        let manifest_data = serde_json::to_vec(manifest)?;
        let hash = self.store_chunk(&manifest_data, true)?;

        // Store profile mapping for easy lookup
        let profiles_dir = self.base_path.join("profiles");
        fs::create_dir_all(&profiles_dir)?;

        let profile_path = profiles_dir.join(&manifest.profile);
        fs::write(profile_path, hash.to_hex())?;

        Ok(hash)
    }

    /// Store a reduction manifest for a specific database version
    pub fn store_database_reduction_manifest(
        &self,
        manifest: &crate::reduction::ReductionManifest,
        source: &str,
        dataset: &str,
        version: &str,
    ) -> Result<SHA256Hash> {
        // Serialize to MessagePack for efficient binary storage
        let msgpack_data = rmp_serde::to_vec(manifest)?;

        // Create .tal format with magic header
        let mut tal_content = Vec::new();
        tal_content.extend_from_slice(TALARIA_MAGIC); // Magic + version
        tal_content.extend_from_slice(&msgpack_data);

        // Store the manifest as a chunk for deduplication
        let hash = self.store_chunk(&tal_content, true)?;

        // Store profile in version-specific directory
        let profiles_dir = self
            .base_path
            .join("versions")
            .join(source)
            .join(dataset)
            .join(version)
            .join("profiles");
        fs::create_dir_all(&profiles_dir)?;

        // Check for JSON_FORMAT environment variable (for debugging)
        let use_json = std::env::var("TALARIA_JSON_FORMAT").is_ok();

        if use_json {
            // Store as JSON for debugging
            let json_data = serde_json::to_vec(manifest)?;
            let profile_path = profiles_dir.join(format!("{}.json", &manifest.profile));
            fs::write(profile_path, &json_data)?;
        } else {
            // Store as .tal binary format (default)
            let profile_path = profiles_dir.join(format!("{}.tal", &manifest.profile));
            fs::write(profile_path, &tal_content)?;
        }

        Ok(hash)
    }

    /// Get a reduction manifest by profile name (deprecated - use get_database_reduction_by_profile)
    pub fn get_reduction_by_profile(
        &self,
        profile: &str,
    ) -> Result<Option<crate::reduction::ReductionManifest>> {
        let profile_path = self.base_path.join("profiles").join(profile);

        if !profile_path.exists() {
            return Ok(None);
        }

        let hash_str = fs::read_to_string(&profile_path)?;
        let hash = SHA256Hash::from_hex(&hash_str)?;

        let data = self.get_chunk(&hash)?;
        let manifest: crate::reduction::ReductionManifest = serde_json::from_slice(&data)?;
        Ok(Some(manifest))
    }

    /// Get a reduction manifest for a specific database version
    pub fn get_database_reduction_by_profile(
        &self,
        profile: &str,
        source: &str,
        dataset: &str,
        version: Option<&str>,
    ) -> Result<Option<crate::reduction::ReductionManifest>> {
        // Helper function to load manifest from a directory
        let load_from_dir =
            |dir: PathBuf| -> Result<Option<crate::reduction::ReductionManifest>> {
                // Try .tal first (preferred binary format)
                let tal_path = dir.join(format!("{}.tal", profile));
                if tal_path.exists() {
                    let mut data = fs::read(&tal_path)?;

                    // Check and skip magic header
                    if data.starts_with(TALARIA_MAGIC) {
                        data = data[TALARIA_MAGIC.len()..].to_vec();
                    }

                    let manifest: crate::reduction::ReductionManifest =
                        rmp_serde::from_slice(&data)?;
                    return Ok(Some(manifest));
                }

                // Fall back to .json for backwards compatibility or debugging
                let json_path = dir.join(format!("{}.json", profile));
                if json_path.exists() {
                    let manifest_data = fs::read(&json_path)?;
                    let manifest: crate::reduction::ReductionManifest =
                        serde_json::from_slice(&manifest_data)?;
                    return Ok(Some(manifest));
                }

                Ok(None)
            };

        // If version specified, look in that specific version
        if let Some(ver) = version {
            let profiles_dir = self
                .base_path
                .join("versions")
                .join(source)
                .join(dataset)
                .join(ver)
                .join("profiles");

            if let Some(manifest) = load_from_dir(profiles_dir)? {
                return Ok(Some(manifest));
            }
        } else {
            // Look in 'current' symlink first, then latest version
            let current_profiles = self
                .base_path
                .join("versions")
                .join(source)
                .join(dataset)
                .join("current")
                .join("profiles");

            if let Some(manifest) = load_from_dir(current_profiles)? {
                return Ok(Some(manifest));
            }
        }

        Ok(None)
    }

    /// List all reduction profiles (deprecated - use list_database_reduction_profiles)
    pub fn list_reduction_profiles(&self) -> Result<Vec<String>> {
        let profiles_dir = self.base_path.join("profiles");
        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                profiles.push(name.to_string());
            }
        }

        Ok(profiles)
    }

    /// List reduction profiles for a specific database
    pub fn list_database_reduction_profiles(
        &self,
        source: &str,
        dataset: &str,
        version: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut profiles = Vec::new();

        // Determine which version(s) to check
        let versions_to_check = if let Some(ver) = version {
            vec![ver.to_string()]
        } else {
            // Check all versions
            let dataset_path = self.base_path.join("versions").join(source).join(dataset);

            if !dataset_path.exists() {
                return Ok(Vec::new());
            }

            let mut versions = Vec::new();
            for entry in fs::read_dir(&dataset_path)? {
                let entry = entry?;
                if let Some(name) = entry.file_name().to_str() {
                    if name != "current" && entry.path().is_dir() {
                        versions.push(name.to_string());
                    }
                }
            }
            versions
        };

        // Check each version for profiles
        for ver in versions_to_check {
            let profiles_dir = self
                .base_path
                .join("versions")
                .join(source)
                .join(dataset)
                .join(&ver)
                .join("profiles");

            if profiles_dir.exists() {
                for entry in fs::read_dir(&profiles_dir)? {
                    let entry = entry?;
                    if let Some(name) = entry.file_name().to_str() {
                        // Remove extension (.tal or .json) if present
                        let profile_name = if name.ends_with(".tal") {
                            &name[..name.len() - 4]
                        } else if name.ends_with(".json") {
                            &name[..name.len() - 5]
                        } else {
                            name
                        };
                        if !profiles.contains(&profile_name.to_string()) {
                            profiles.push(profile_name.to_string());
                        }
                    }
                }
            }
        }

        Ok(profiles)
    }

    // Processing state management methods

    /// Start a new processing operation
    pub fn start_processing(
        &self,
        operation: OperationType,
        manifest_hash: SHA256Hash,
        manifest_version: String,
        total_chunks: usize,
        source_info: SourceInfo,
    ) -> Result<String> {
        let operation_id =
            ProcessingStateManager::generate_operation_id(&source_info.database, &operation);

        let state = ProcessingState::new(
            operation,
            manifest_hash,
            manifest_version,
            total_chunks,
            source_info,
        );

        let state_manager = self.state_manager.lock().unwrap();
        state_manager.save_state(&state, &operation_id)?;

        // Set current operation
        let mut current = self.current_operation_id.lock().unwrap();
        *current = Some(operation_id.clone());

        Ok(operation_id)
    }

    /// Check for resumable operation
    pub fn check_resumable(
        &self,
        database: &str,
        operation: &OperationType,
        manifest_hash: &SHA256Hash,
        manifest_version: &str,
    ) -> Result<Option<ProcessingState>> {
        let operation_id = ProcessingStateManager::generate_operation_id(database, operation);

        let state_manager = self.state_manager.lock().unwrap();
        if let Some(state) = state_manager.load_state(&operation_id)? {
            if state.can_resume_with(manifest_hash, manifest_version) {
                // Set as current operation
                let mut current = self.current_operation_id.lock().unwrap();
                *current = Some(operation_id);
                return Ok(Some(state));
            }
        }

        Ok(None)
    }

    /// Update processing state with completed chunks
    pub fn update_processing_state(&self, completed_chunks: &[SHA256Hash]) -> Result<()> {
        let current = self.current_operation_id.lock().unwrap();
        if let Some(ref operation_id) = *current {
            let state_manager = self.state_manager.lock().unwrap();
            if let Some(mut state) = state_manager.load_state(operation_id)? {
                state.mark_chunks_completed(completed_chunks);
                state_manager.save_state(&state, operation_id)?;
            }
        }
        Ok(())
    }

    /// Complete current processing operation
    pub fn complete_processing(&self) -> Result<()> {
        let mut current = self.current_operation_id.lock().unwrap();
        if let Some(ref operation_id) = *current {
            let state_manager = self.state_manager.lock().unwrap();
            state_manager.delete_state(operation_id)?;
        }
        *current = None;
        Ok(())
    }

    /// Get current processing state
    pub fn get_current_state(&self) -> Result<Option<ProcessingState>> {
        let current = self.current_operation_id.lock().unwrap();
        if let Some(ref operation_id) = *current {
            let state_manager = self.state_manager.lock().unwrap();
            return state_manager.load_state(operation_id);
        }
        Ok(None)
    }

    /// List all resumable operations
    pub fn list_resumable_operations(&self) -> Result<Vec<(String, ProcessingState)>> {
        let state_manager = self.state_manager.lock().unwrap();
        state_manager.list_states()
    }

    /// Clean up expired processing states
    pub fn cleanup_expired_states(&self) -> Result<usize> {
        let state_manager = self.state_manager.lock().unwrap();
        state_manager.cleanup_expired()
    }

    /// Get chunks that still need to be fetched for current operation
    pub fn get_remaining_chunks(&self, all_chunks: &[SHA256Hash]) -> Result<Vec<SHA256Hash>> {
        if let Some(state) = self.get_current_state()? {
            let remaining: Vec<SHA256Hash> = all_chunks
                .iter()
                .filter(|h| !state.completed_chunks.contains(h))
                .cloned()
                .collect();
            Ok(remaining)
        } else {
            Ok(all_chunks.to_vec())
        }
    }

    /// List all chunk hashes in storage
    pub fn list_all_chunks(&self) -> Result<Vec<SHA256Hash>> {
        Ok(self.chunk_index.iter().map(|entry| entry.key().clone()).collect())
    }

    /// Get the size of a specific chunk
    pub fn get_chunk_size(&self, hash: &SHA256Hash) -> Result<usize> {
        self.chunk_index
            .get(hash)
            .map(|entry| entry.value().size)
            .ok_or_else(|| anyhow::anyhow!("Chunk not found: {}", hash))
    }

    /// Remove a chunk from storage
    pub fn remove_chunk(&self, hash: &SHA256Hash) -> Result<()> {
        if let Some((_, location)) = self.chunk_index.remove(hash) {
            // Remove the actual file
            if location.path.exists() {
                fs::remove_file(&location.path)
                    .context("Failed to remove chunk file")?;
            }
        }
        Ok(())
    }

    /// Verify the integrity of the storage
    pub fn verify_integrity(&self) -> Result<()> {
        // Verify that chunks directory exists
        let chunks_dir = self.base_path.join("chunks");
        if !chunks_dir.exists() {
            anyhow::bail!("Chunks directory does not exist: {:?}", chunks_dir);
        }

        // Verify each chunk in the index
        let mut errors = Vec::new();
        for entry in self.chunk_index.iter() {
            let (hash, location) = entry.pair();

            // Check file exists
            if !location.path.exists() {
                errors.push(format!("Missing chunk file for hash {}: {:?}", hash, location.path));
                continue;
            }

            // Verify hash matches content
            let data = match fs::read(&location.path) {
                Ok(d) => d,
                Err(e) => {
                    errors.push(format!("Failed to read chunk {}: {}", hash, e));
                    continue;
                }
            };

            let computed_hash = SHA256Hash::from_bytes(&data);
            if &computed_hash != hash {
                errors.push(format!(
                    "Hash mismatch for chunk at {:?}: expected {}, got {}",
                    location.path, hash, computed_hash
                ));
            }
        }

        if !errors.is_empty() {
            anyhow::bail!("Storage integrity check failed:\n{}", errors.join("\n"));
        }

        Ok(())
    }
}

// TODO: Implement storage traits for SEQUOIAStorage once traits are defined
// /*
// impl crate::storage::traits::ChunkStorage for SEQUOIAStorage {
//     fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash> {
//         self.store_chunk(data, compress)
//     }
//
//     fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
//         self.get_chunk(hash)
//     }
//
//     fn has_chunk(&self, hash: &SHA256Hash) -> bool {
//         self.has_chunk(hash)
//     }
//
//     fn enumerate_chunks(&self) -> Vec<ChunkInfo> {
//         self.enumerate_chunks()
//     }
//
//     fn verify_all(&self) -> Result<Vec<crate::storage::traits::VerificationError>> {
//         self.verify_all().map(|errors| {
//             errors.into_iter().map(|e| crate::storage::traits::VerificationError {
//                 chunk_hash: e.chunk_hash,
//                 error_type: match e.error_type {
//                     VerificationErrorType::HashMismatch { expected, actual } => {
//                         crate::storage::traits::VerificationErrorType::HashMismatch { expected, actual }
//                     }
//                     VerificationErrorType::ReadError(msg) => {
//                         crate::storage::traits::VerificationErrorType::ReadError(msg)
//                     }
//                 }
//             }).collect()
//         })
//     }
//
//     fn get_stats(&self) -> crate::storage::traits::StorageStats {
//         let stats = self.get_stats();
//         crate::storage::traits::StorageStats {
//             total_chunks: stats.total_chunks,
//             total_size: stats.total_size,
//             compressed_chunks: stats.compressed_chunks,
//             deduplication_ratio: stats.deduplication_ratio,
//         }
//     }
//
//     fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<crate::storage::traits::GCResult> {
//         self.gc(referenced).map(|result| crate::storage::traits::GCResult {
//             removed_count: result.removed_count,
//             freed_space: result.freed_space,
//         })
//     }
// }
//
// impl crate::storage::traits::DeltaStorage for SEQUOIAStorage {
//     fn store_delta_chunk(&self, chunk: &DeltaChunk) -> Result<SHA256Hash> {
//         self.store_delta_chunk(chunk)
//     }
//
//     fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<DeltaChunk> {
//         self.get_delta_chunk(hash)
//     }
//
//     fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>> {
//         self.find_delta_for_child(child_id)
//     }
//
//     fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>> {
//         self.get_deltas_for_reference(reference_hash)
//     }
//
//     fn find_delta_chunks_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>> {
//         self.find_delta_chunks_for_reference(reference_hash)
//     }
//
//     fn get_chunk_type(&self, hash: &SHA256Hash) -> Result<ChunkType> {
//         self.get_chunk_type(hash)
//     }
// }
//
// impl crate::storage::traits::ReductionStorage for SEQUOIAStorage {
//     fn store_reduction_manifest(&self, manifest: &crate::reduction::ReductionManifest) -> Result<SHA256Hash> {
//         self.store_reduction_manifest(manifest)
//     }
//
//     fn get_reduction_by_profile(&self, profile: &str) -> Result<Option<crate::reduction::ReductionManifest>> {
//         self.get_reduction_by_profile(profile)
//     }
//
//     fn list_reduction_profiles(&self) -> Result<Vec<String>> {
//         let profiles_dir = self.base_path.join("profiles");
//         if !profiles_dir.exists() {
//             return Ok(Vec::new());
//         }
//
//         let mut profiles = Vec::new();
//         for entry in fs::read_dir(&profiles_dir)? {
//             let entry = entry?;
//             if let Some(name) = entry.file_name().to_str() {
//                 profiles.push(name.to_string());
//             }
//         }
//         Ok(profiles)
//     }
//
//     fn delete_reduction_profile(&self, profile: &str) -> Result<()> {
//         let profile_path = self.base_path.join("profiles").join(profile);
//         if profile_path.exists() {
//             fs::remove_file(profile_path)?;
//         }
//         Ok(())
//     }
// }
//
// impl crate::storage::traits::TaxonomyStorage for SEQUOIAStorage {
//     fn store_taxonomy_chunk(&self, chunk: &TaxonomyAwareChunk) -> Result<SHA256Hash> {
//         self.store_taxonomy_chunk(chunk)
//     }
//
//     fn get_taxonomy_chunk(&self, hash: &SHA256Hash) -> Result<TaxonomyAwareChunk> {
//         let data = self.get_chunk(hash)?;
//         let chunk: TaxonomyAwareChunk = serde_json::from_slice(&data)?;
//         Ok(chunk)
//     }
//
//     fn find_chunks_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>> {
//         // Load manifest to find chunks containing specific taxon
//         let manifest_path_tal = self.base_path.join("manifest.tal");
//         let manifest_path_json = self.base_path.join("manifest.json");
//
//         let manifest = if manifest_path_tal.exists() {
//             let mut data = fs::read(&manifest_path_tal)?;
//             // Skip magic header
//             if data.starts_with(TALARIA_MAGIC) {
//                 data = data[TALARIA_MAGIC.len()..].to_vec();
//             }
//             rmp_serde::from_slice::<TemporalManifest>(&data).ok()
//         } else if manifest_path_json.exists() {
//             let data = fs::read_to_string(&manifest_path_json)?;
//             serde_json::from_str::<TemporalManifest>(&data).ok()
//         } else {
//             return Ok(Vec::new());
//         };
//
//         if let Some(manifest) = manifest {
//             let chunks: Vec<SHA256Hash> = manifest.chunk_index
//                 .iter()
//                 .filter(|meta| meta.taxon_ids.contains(&taxon_id))
//                 .map(|meta| meta.hash.clone())
//                 .collect();
//             Ok(chunks)
//         } else {
//             Ok(Vec::new())
//         }
//     }
//
//     fn get_taxonomy_stats(&self) -> Result<crate::storage::traits::TaxonomyStats> {
//         let mut sequences_per_taxon = std::collections::HashMap::new();
//         let mut chunks_per_taxon = std::collections::HashMap::new();
//
//         // Load manifest to gather taxonomy statistics
//         let manifest_path_tal = self.base_path.join("manifest.tal");
//         let manifest_path_json = self.base_path.join("manifest.json");
//
//         let manifest = if manifest_path_tal.exists() {
//             let mut data = fs::read(&manifest_path_tal)?;
//             // Skip magic header
//             if data.starts_with(TALARIA_MAGIC) {
//                 data = data[TALARIA_MAGIC.len()..].to_vec();
//             }
//             rmp_serde::from_slice::<TemporalManifest>(&data).ok()
//         } else if manifest_path_json.exists() {
//             let data = fs::read_to_string(&manifest_path_json)?;
//             serde_json::from_str::<TemporalManifest>(&data).ok()
//         } else {
//             None
//         };
//
//         if let Some(manifest) = manifest {
//             for chunk_meta in &manifest.chunk_index {
//                 for taxon_id in &chunk_meta.taxon_ids {
//                     *chunks_per_taxon.entry(*taxon_id).or_insert(0) += 1;
//                     *sequences_per_taxon.entry(*taxon_id).or_insert(0) += chunk_meta.sequence_count;
//                 }
//             }
//         }
//
//         Ok(crate::storage::traits::TaxonomyStats {
//             total_taxa: chunks_per_taxon.len(),
//             sequences_per_taxon,
//             chunks_per_taxon,
//         })
//     }
// }
//
// impl crate::storage::traits::RemoteStorage for SEQUOIAStorage {
//     fn fetch_chunks(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<TaxonomyAwareChunk>> {
//         futures::executor::block_on(self.fetch_chunks(hashes))
//     }
//
//     fn push_chunks(&self, _hashes: &[SHA256Hash]) -> Result<()> {
//         // TODO: Implement push to remote repository
//         Ok(())
//     }
//
//     fn sync(&mut self) -> Result<crate::storage::traits::SyncResult> {
//         // TODO: Implement full sync with remote
//         Ok(crate::storage::traits::SyncResult {
//             uploaded: Vec::new(),
//             downloaded: Vec::new(),
//             conflicts: Vec::new(),
//             bytes_transferred: 0,
//         })
//     }
//
//     fn get_remote_status(&self) -> Result<crate::storage::traits::RemoteStatus> {
//         // TODO: Check actual remote status
//         Ok(crate::storage::traits::RemoteStatus {
//             connected: false,
//             remote_chunks: 0,
//             local_chunks: self.chunk_index.len(),
//             pending_sync: 0,
//         })
//     }
// }
//
// impl crate::storage::traits::StatefulStorage for SEQUOIAStorage {
//     fn start_processing(
//         &self,
//         operation: OperationType,
//         manifest_hash: SHA256Hash,
//         manifest_version: String,
//         total_chunks: usize,
//         source_info: SourceInfo,
//     ) -> Result<String> {
//         self.start_processing(operation, manifest_hash, manifest_version, total_chunks, source_info)
//     }
//
//     fn check_resumable(
//         &self,
//         database: &str,
//         operation: &OperationType,
//         manifest_hash: &SHA256Hash,
//         manifest_version: &str,
//     ) -> Result<Option<ProcessingState>> {
//         self.check_resumable(database, operation, manifest_hash, manifest_version)
//     }
//
//     fn update_processing_state(&self, completed_chunks: &[SHA256Hash]) -> Result<()> {
//         self.update_processing_state(completed_chunks)
//     }
//
//     fn complete_processing(&self) -> Result<()> {
//         self.complete_processing()
//     }
//
//     fn get_current_state(&self) -> Result<Option<ProcessingState>> {
//         self.get_current_state()
//     }
//
//     fn list_resumable_operations(&self) -> Result<Vec<(String, ProcessingState)>> {
//         self.list_resumable_operations()
//     }
//
//     fn cleanup_expired_states(&self) -> Result<usize> {
//         self.cleanup_expired_states()
//     }
// }

#[derive(Debug)]
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
}

#[derive(Debug, Default)]
pub struct GarbageCollectionStats {
    pub chunks_deleted: usize,
    pub bytes_freed: usize,
    pub chains_compacted: usize,
}

#[derive(Debug)]
pub struct GCResult {
    pub removed_count: usize,
    pub freed_space: usize,
}

#[derive(Debug)]
pub struct VerificationError {
    pub chunk_hash: SHA256Hash,
    pub error_type: VerificationErrorType,
}

#[derive(Debug)]
pub enum VerificationErrorType {
    HashMismatch {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },
    ReadError(String),
}
