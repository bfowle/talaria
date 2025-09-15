/// Content-addressed storage implementation for CASG

use crate::casg::types::*;
use crate::casg::processing_state::{ProcessingState, ProcessingStateManager, OperationType, SourceInfo};
use anyhow::{Context, Result};
use dashmap::DashMap;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct CASGStorage {
    pub(crate) base_path: PathBuf,
    chunk_index: Arc<DashMap<SHA256Hash, ChunkLocation>>,
    index_lock: Arc<Mutex<()>>,
    state_manager: Arc<Mutex<ProcessingStateManager>>,
    current_operation_id: Arc<Mutex<Option<String>>>,
}

#[derive(Debug, Clone)]
struct ChunkLocation {
    path: PathBuf,
    compressed: bool,
    size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeltaIndexEntry {
    child_id: String,
    delta_chunk_hash: SHA256Hash,
    reference_chunk_hash: SHA256Hash,
    reference_id: String,
}

impl CASGStorage {
    pub fn new(base_path: &Path) -> Result<Self> {
        let chunks_dir = base_path.join("chunks");
        fs::create_dir_all(&chunks_dir)
            .context("Failed to create chunks directory")?;

        let state_manager = ProcessingStateManager::new(base_path)?;

        Ok(Self {
            base_path: base_path.to_path_buf(),
            chunk_index: Arc::new(DashMap::new()),
            index_lock: Arc::new(Mutex::new(())),
            state_manager: Arc::new(Mutex::new(state_manager)),
            current_operation_id: Arc::new(Mutex::new(None)),
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
                        let compressed = path.extension()
                            .map(|e| e == "gz")
                            .unwrap_or(false);

                        self.chunk_index.insert(hash.clone(), ChunkLocation {
                            path: path.clone(),
                            compressed,
                            size: metadata.len() as usize,
                        });
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

        let chunk_path = self.get_chunk_path(&hash, compress);

        if compress {
            let file = fs::File::create(&chunk_path)
                .context("Failed to create chunk file")?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(data)
                .context("Failed to write compressed chunk")?;
            encoder.finish()
                .context("Failed to finish compression")?;
        } else {
            fs::write(&chunk_path, data)
                .context("Failed to write chunk")?;
        }

        let metadata = fs::metadata(&chunk_path)?;
        self.chunk_index.insert(hash.clone(), ChunkLocation {
            path: chunk_path,
            compressed: compress,
            size: metadata.len() as usize,
        });

        Ok(hash)
    }

    /// Retrieve a chunk from storage
    pub fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        let location_ref = self.chunk_index.get(hash)
            .ok_or_else(|| anyhow::anyhow!("Chunk not found: {}", hash))?;
        let location = location_ref.value();

        if location.compressed {
            let file = fs::File::open(&location.path)
                .context("Failed to open chunk file")?;
            let mut decoder = GzDecoder::new(file);
            let mut data = Vec::new();
            decoder.read_to_end(&mut data)
                .context("Failed to decompress chunk")?;
            Ok(data)
        } else {
            fs::read(&location.path)
                .context("Failed to read chunk")
        }
    }

    /// Check if a chunk exists
    pub fn has_chunk(&self, hash: &SHA256Hash) -> bool {
        self.chunk_index.contains_key(hash)
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
        // Serialize the entire chunk including sequences
        let chunk_data = serde_json::to_vec(chunk)?;
        let chunk_hash = chunk.content_hash.clone();

        // Create the chunk file path
        let chunk_path = self.get_chunk_path(&chunk_hash, true);

        // Write compressed chunk data directly
        let file = fs::File::create(&chunk_path)
            .context("Failed to create chunk file")?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&chunk_data)
            .context("Failed to write compressed chunk")?;
        encoder.finish()
            .context("Failed to finish compression")?;

        // Update chunk index
        self.chunk_index.insert(
            chunk_hash.clone(),
            ChunkLocation {
                path: chunk_path,
                compressed: true,
                size: chunk_data.len(),
            },
        );

        // Update persistent index (thread-safe)
        {
            let _lock = self.index_lock.lock().unwrap();
            let index_path = self.base_path.join("chunk_index.json");
            let mut index: HashMap<String, String> = if index_path.exists() {
                serde_json::from_str(&fs::read_to_string(&index_path)?)?
            } else {
                HashMap::new()
            };

            index.insert(chunk_hash.to_hex(), format!("chunks/{}", chunk_hash.to_hex()));
            fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
        }

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
        let missing_hashes: Vec<_> = hashes.iter()
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

        eprintln!("Need to fetch {} chunks (already have {} locally, {} from previous run)",
            missing_hashes.len(),
            hashes.len() - missing_hashes.len() - already_completed.len(),
            already_completed.len()
        );

        // Fetch missing chunks in parallel (up to 4 concurrent)
        let base_path = self.base_path.clone();
        let mut successfully_fetched = Vec::new();

        let fetch_futures = missing_hashes.iter()
            .map(|hash| {
                let hash_clone = hash.clone();
                let base_path_clone = base_path.clone();
                async move {
                    Self::fetch_single_chunk_static(&hash_clone, &base_path_clone).await
                }
            });

        let mut fetch_stream = stream::iter(fetch_futures)
            .buffer_unordered(4);

        let mut fetch_count = 0;
        let total_to_fetch = missing_hashes.len();

        while let Some(result) = fetch_stream.next().await {
            match result {
                Ok((hash, data)) => {
                    // Store fetched chunk locally
                    self.store_chunk(&data, true)?;
                    successfully_fetched.push(hash.clone());
                    fetch_count += 1;

                    eprintln!("[{}/{}] Fetched and stored chunk: {}",
                        fetch_count, total_to_fetch, hash);

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
    async fn fetch_single_chunk_static(hash: &SHA256Hash, _base_path: &PathBuf) -> Result<(SHA256Hash, Vec<u8>)> {
        // Configuration for remote repository
        let remote_base = std::env::var("TALARIA_REMOTE_REPO")
            .unwrap_or_else(|_| "https://casg.talaria.org".to_string());

        let chunk_url = format!("{}/chunks/{}", remote_base, hash.to_hex());

        // Use reqwest for HTTP fetching (would need to add as dependency)
        // For now, simulate with local filesystem fallback
        let remote_path = std::path::PathBuf::from("/tmp/talaria-remote-repo/chunks")
            .join(hash.to_hex());

        if remote_path.exists() {
            let data = tokio::fs::read(&remote_path).await?;

            // Verify hash matches
            let computed_hash = SHA256Hash::compute(&data);
            if &computed_hash != hash {
                return Err(anyhow::anyhow!(
                    "Hash mismatch for chunk {}: expected {}, got {}",
                    hash, hash, computed_hash
                ));
            }

            Ok((hash.clone(), data))
        } else {
            // In production, would use HTTP client here
            Err(anyhow::anyhow!(
                "Remote chunk not available: {} (would fetch from {})",
                hash, chunk_url
            ))
        }
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> StorageStats {
        let total_chunks = self.chunk_index.len();
        let total_size: usize = self.chunk_index.iter()
            .map(|entry| entry.value().size)
            .sum();
        let compressed_chunks = self.chunk_index.iter()
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
    pub fn get_sequence_root(&self) -> Result<crate::casg::MerkleHash> {
        use crate::casg::merkle::MerkleDAG;

        // Collect all chunk hashes in sorted order for deterministic root
        let mut chunk_hashes: Vec<Vec<u8>> = self.chunk_index
            .iter()
            .map(|entry| entry.key().as_bytes().to_vec())
            .collect();
        chunk_hashes.sort();

        // Build Merkle tree from chunk hashes
        let dag = MerkleDAG::build_from_chunks(chunk_hashes)?;

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
                        *reference_counts.entry(entry.reference_chunk_hash).or_insert(0) += 1;
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
            })
            .collect()
    }

    /// Get chunk metadata
    pub fn get_chunk_info(&self, hash: &SHA256Hash) -> Option<ChunkInfo> {
        self.chunk_index.get(hash).map(|entry| ChunkInfo {
            hash: hash.clone(),
            path: entry.value().path.clone(),
            size: entry.value().size,
            compressed: entry.value().compressed,
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

        let chunks_to_remove: Vec<_> = self.chunk_index
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
        self.chunk_index.iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    // Delta-specific storage operations

    /// Store a delta chunk in content-addressed storage
    pub fn store_delta_chunk(&self, delta_chunk: &crate::casg::delta::DeltaChunk) -> Result<SHA256Hash> {
        // Serialize the delta chunk
        let chunk_data = serde_json::to_vec(delta_chunk)?;

        // Store with compression (deltas compress well)
        let hash = self.store_chunk(&chunk_data, true)?;

        // Store delta index metadata separately for fast lookups
        self.update_delta_index(delta_chunk)?;

        Ok(hash)
    }

    /// Retrieve a delta chunk from storage
    pub fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<crate::casg::delta::DeltaChunk> {
        let data = self.get_chunk(hash)?;
        let delta_chunk: crate::casg::delta::DeltaChunk = serde_json::from_slice(&data)?;
        Ok(delta_chunk)
    }

    /// Store raw chunk data (for manifests, etc.)
    pub fn store_raw_chunk(&self, hash: &SHA256Hash, data: Vec<u8>) -> Result<()> {
        // Verify the hash matches
        let computed_hash = SHA256Hash::compute(&data);
        if computed_hash != *hash {
            return Err(anyhow::anyhow!("Hash mismatch: expected {}, got {}", hash, computed_hash));
        }

        // Store the chunk
        self.store_chunk(&data, true)?;
        Ok(())
    }

    /// Update the delta index for fast child lookups
    fn update_delta_index(&self, delta_chunk: &crate::casg::delta::DeltaChunk) -> Result<()> {
        // Store delta index information
        let index_dir = self.base_path.join("delta_index");
        fs::create_dir_all(&index_dir)?;

        // Create index entries for each child
        for record in &delta_chunk.delta_records {
            let child_index_path = index_dir.join(format!("{}.idx", record.child_id));
            let index_entry = DeltaIndexEntry {
                child_id: record.child_id.clone(),
                delta_chunk_hash: delta_chunk.content_hash.clone(),
                reference_chunk_hash: delta_chunk.reference_chunk_hash.clone(),
                reference_id: record.reference_id.clone(),
            };

            let index_data = serde_json::to_vec(&index_entry)?;
            fs::write(child_index_path, index_data)?;
        }

        Ok(())
    }

    /// Find the delta chunk containing a specific child sequence
    pub fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>> {
        let index_path = self.base_path.join("delta_index").join(format!("{}.idx", child_id));

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

                    if index_entry.reference_chunk_hash == *reference_hash {
                        if seen.insert(index_entry.delta_chunk_hash.clone()) {
                            delta_hashes.push(index_entry.delta_chunk_hash);
                        }
                    }
                }
            }
        }

        Ok(delta_hashes)
    }

    /// Store a reduction manifest
    pub fn store_reduction_manifest(&self, manifest: &crate::casg::reduction::ReductionManifest) -> Result<SHA256Hash> {
        let manifest_data = serde_json::to_vec(manifest)?;
        let hash = self.store_chunk(&manifest_data, true)?;

        // Store profile mapping for easy lookup
        let profiles_dir = self.base_path.join("profiles");
        fs::create_dir_all(&profiles_dir)?;

        let profile_path = profiles_dir.join(&manifest.profile);
        fs::write(profile_path, hash.to_hex())?;

        Ok(hash)
    }

    /// Get a reduction manifest by profile name
    pub fn get_reduction_by_profile(&self, profile: &str) -> Result<Option<crate::casg::reduction::ReductionManifest>> {
        let profile_path = self.base_path.join("profiles").join(profile);

        if !profile_path.exists() {
            return Ok(None);
        }

        let hash_str = fs::read_to_string(&profile_path)?;
        let hash = SHA256Hash::from_hex(&hash_str)?;

        let data = self.get_chunk(&hash)?;
        let manifest: crate::casg::reduction::ReductionManifest = serde_json::from_slice(&data)?;
        Ok(Some(manifest))
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
        let operation_id = ProcessingStateManager::generate_operation_id(
            &source_info.database,
            &operation,
        );

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
}

#[derive(Debug)]
pub struct StorageStats {
    pub total_chunks: usize,
    pub total_size: usize,
    pub compressed_chunks: usize,
    pub deduplication_ratio: f32,
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