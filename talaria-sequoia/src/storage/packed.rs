/// Packed sequence storage backend - stores sequences in large pack files
/// instead of individual files to avoid filesystem overhead
use super::sequence::SequenceStorageBackend;
use talaria_core::StorageStats;
use crate::types::{
    CanonicalSequence, Representable, SequenceRepresentations, SHA256Hash,
};
use anyhow::{anyhow, Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tracing as log;  // Use tracing for logging

const PACK_MAGIC: &[u8; 4] = b"PKSQ"; // Pack SeQuence
const PACK_VERSION: u8 = 1;
const MAX_PACK_SIZE: usize = 64 * 1024 * 1024; // 64MB per pack

/// Location of a sequence within a pack file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackLocation {
    pack_id: u32,
    offset: u64,
    length: u32,
    compressed: bool,
}

/// Entry in a pack file
#[derive(Debug, Serialize, Deserialize)]
struct PackEntry {
    hash: SHA256Hash,
    sequence_length: u32,
    representations_length: u32,
    // Data follows: [sequence_data][representations_data]
}

/// Pack file writer
struct PackWriter {
    id: u32,
    path: PathBuf,
    writer: BufWriter<File>,
    current_size: usize,
    entries: Vec<(SHA256Hash, u64, u32)>, // (hash, offset, total_length)
}

impl PackWriter {
    fn new(packs_dir: &Path, id: u32) -> Result<Self> {
        let path = packs_dir.join(format!("pack_{:04}.tal", id));
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        // Write header
        writer.write_all(PACK_MAGIC)?;
        writer.write_all(&[PACK_VERSION])?;
        writer.write_all(&id.to_le_bytes())?;

        Ok(Self {
            id,
            path,
            writer,
            current_size: 9, // Header size
            entries: Vec::new(),
        })
    }

    fn write_entry(
        &mut self,
        hash: &SHA256Hash,
        sequence_data: &[u8],
        representations_data: &[u8],
    ) -> Result<PackLocation> {
        let offset = self.current_size as u64;

        // Write entry header
        let entry = PackEntry {
            hash: hash.clone(),
            sequence_length: sequence_data.len() as u32,
            representations_length: representations_data.len() as u32,
        };

        let entry_header = rmp_serde::to_vec(&entry)?;
        let header_len = entry_header.len() as u32;

        // Note: sequence_data and representations_data are already serialized MessagePack bytes
        // We just need to store them directly

        // Write: [header_len:4][header][sequence][representations]
        self.writer.write_all(&header_len.to_le_bytes())?;
        self.writer.write_all(&entry_header)?;
        self.writer.write_all(sequence_data)?;
        self.writer.write_all(representations_data)?;

        // Flush to ensure data is written
        self.writer.flush()?;

        let total_length = 4 + header_len + entry.sequence_length + entry.representations_length;

        self.entries.push((hash.clone(), offset, total_length));
        self.current_size += total_length as usize;

        Ok(PackLocation {
            pack_id: self.id,
            offset,
            length: total_length,
            compressed: false,
        })
    }

    fn should_rotate(&self) -> bool {
        self.current_size >= MAX_PACK_SIZE
    }

    fn finalize(mut self) -> Result<()> {
        // If no entries were written, remove the empty pack file
        if self.entries.is_empty() {
            drop(self.writer); // Close the writer first
            // Remove the empty pack file
            if self.path.exists() {
                fs::remove_file(&self.path)?;
            }
            return Ok(());
        }

        // Write footer with entry count
        let entry_count = self.entries.len() as u32;
        self.writer.write_all(&entry_count.to_le_bytes())?;
        self.writer.flush()?;
        drop(self.writer); // Close the writer

        // Compress the entire pack file with Zstandard
        let pack_data = fs::read(&self.path)?;
        let compressed = zstd::encode_all(&pack_data[..], 3)?;
        fs::write(&self.path, compressed)?;

        Ok(())
    }
}

/// Pack file reader
struct PackReader {
    data: Vec<u8>,
}

impl PackReader {
    fn open(path: &Path) -> Result<Self> {
        let compressed_data = if path.exists() {
            fs::read(path)?
        } else {
            return Err(anyhow!("Pack file not found: {:?}", path));
        };

        // Skip empty or too-small files
        if compressed_data.is_empty() {
            return Err(anyhow!("Pack file is empty: {:?}", path));
        }

        // Decompress if it's Zstandard compressed
        let data = if compressed_data.len() >= 4
            && compressed_data[0] == 0x28
            && compressed_data[1] == 0xb5 {
            // Zstandard magic bytes detected - decompress
            match zstd::decode_all(&compressed_data[..]) {
                Ok(d) => d,
                Err(e) => {
                    return Err(anyhow!("Failed to decompress pack file {:?}: {}", path, e));
                }
            }
        } else {
            // Not compressed (shouldn't happen in new files)
            compressed_data
        };

        // Verify header
        if data.len() < 9 {
            return Err(anyhow!("Pack file too small: {} bytes at {:?}", data.len(), path));
        }
        if &data[0..4] != PACK_MAGIC {
            return Err(anyhow!("Invalid pack file magic bytes: {:?} at {:?}", &data[0..4], path));
        }

        // Verify there's at least a footer (entry count)
        if data.len() < 13 {  // 9 bytes header + 4 bytes footer minimum
            return Err(anyhow!("Pack file incomplete (no footer): {} bytes at {:?}", data.len(), path));
        }

        Ok(Self { data })
    }

    fn read_entry(&self, location: &PackLocation) -> Result<(Vec<u8>, Vec<u8>)> {
        let offset = location.offset as usize;

        // Bounds check
        if offset + 4 > self.data.len() {
            return Err(anyhow!("Pack offset {} out of bounds (pack size: {})", offset, self.data.len()));
        }

        // Read header length
        let header_len = u32::from_le_bytes(
            self.data[offset..offset + 4]
                .try_into()
                .context("Invalid header length")?
        ) as usize;

        // Read and parse header
        let header_data = &self.data[offset + 4..offset + 4 + header_len];
        let entry: PackEntry = rmp_serde::from_slice(header_data)?;

        // Read sequence and representations data
        let seq_start = offset + 4 + header_len;
        let seq_end = seq_start + entry.sequence_length as usize;
        let repr_end = seq_end + entry.representations_length as usize;

        // The data is already MessagePack serialized, just return it as-is
        let sequence_data = self.data[seq_start..seq_end].to_vec();
        let representations_data = self.data[seq_end..repr_end].to_vec();

        Ok((sequence_data, representations_data))
    }
}

/// Packed storage backend implementation
pub struct PackedSequenceStorage {
    packs_dir: PathBuf,
    indices_dir: PathBuf,
    current_pack: Arc<Mutex<Option<PackWriter>>>,
    next_pack_id: Arc<RwLock<u32>>,
    pack_index: Arc<DashMap<SHA256Hash, PackLocation>>,
    pack_readers: Arc<DashMap<u32, Arc<PackReader>>>,
}

impl PackedSequenceStorage {
    pub fn new(base_path: &Path) -> Result<Self> {
        let packs_dir = base_path.join("packs");
        let indices_dir = base_path.join("indices");

        fs::create_dir_all(&packs_dir)?;
        fs::create_dir_all(&indices_dir)?;

        // Load or create index
        let pack_index = Arc::new(Self::load_or_create_index(&indices_dir)?);

        // Find next pack ID - start from 1 if no packs exist
        let next_pack_id = Self::find_next_pack_id(&packs_dir)?;

        Ok(Self {
            packs_dir,
            indices_dir,
            current_pack: Arc::new(Mutex::new(None)),
            next_pack_id: Arc::new(RwLock::new(next_pack_id)),
            pack_index,
            pack_readers: Arc::new(DashMap::new()),
        })
    }


    fn load_or_create_index(indices_dir: &Path) -> Result<DashMap<SHA256Hash, PackLocation>> {
        let index_path = indices_dir.join("sequence_index.tal");

        if index_path.exists() {
            // Try to load existing index, but if it fails, start fresh
            match Self::try_load_index(&index_path) {
                Ok(index) => Ok(index),
                Err(e) => {
                    log::warn!("Failed to load existing index, starting fresh: {}", e);
                    // Remove corrupt index file
                    let _ = fs::remove_file(&index_path);
                    Ok(DashMap::new())
                }
            }
        } else {
            // Create new index
            Ok(DashMap::new())
        }
    }

    fn try_load_index(index_path: &Path) -> Result<DashMap<SHA256Hash, PackLocation>> {
        // Load existing index (TALARIA format: MessagePack + Zstandard)
        let compressed_data = fs::read(index_path)?;

        // Decompress if needed
        let data = if compressed_data.len() >= 4
            && compressed_data[0] == 0x28
            && compressed_data[1] == 0xb5 {
            // Zstandard compressed
            zstd::decode_all(&compressed_data[..])?
        } else {
            // Legacy uncompressed (for backwards compatibility)
            compressed_data
        };

        let index_map: std::collections::HashMap<SHA256Hash, PackLocation> =
            rmp_serde::from_slice(&data)?;

        let dash_map = DashMap::new();
        for (k, v) in index_map {
            dash_map.insert(k, v);
        }
        Ok(dash_map)
    }

    fn save_index(&self) -> Result<()> {
        let index_path = self.indices_dir.join("sequence_index.tal");

        // IMPORTANT: If our index is empty but a non-empty index exists on disk,
        // don't overwrite it! This can happen when multiple instances are created.
        if self.pack_index.is_empty() && index_path.exists() {
            let file_size = fs::metadata(&index_path)?.len();
            // A valid empty index compressed is ~10 bytes, anything larger means it has data
            if file_size > 15 {
                log::debug!("Skipping save of empty index - good index already exists on disk (size: {} bytes)", file_size);
                return Ok(());
            }
        }

        // Debug logging through tracing
        log::debug!("Saving index with {} entries to {:?}",
                  self.pack_index.len(), index_path);

        // Convert DashMap to HashMap for serialization
        let mut index_map = std::collections::HashMap::new();
        for entry in self.pack_index.iter() {
            index_map.insert(entry.key().clone(), entry.value().clone());
        }

        // TALARIA format: MessagePack + Zstandard
        let msgpack_data = rmp_serde::to_vec(&index_map)?;
        let compressed_data = zstd::encode_all(&msgpack_data[..], 3)?;
        fs::write(&index_path, compressed_data)?;

        Ok(())
    }

    fn find_next_pack_id(packs_dir: &Path) -> Result<u32> {
        let mut max_id = 0;

        if packs_dir.exists() {
            for entry in fs::read_dir(packs_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let path = entry.path();

                if name_str.starts_with("pack_") {
                    // Check if the file is valid (not empty or incomplete)
                    if let Ok(metadata) = fs::metadata(&path) {
                        // Skip files that are too small to be valid packs
                        // Minimum valid pack: compressed header (9 bytes) + footer (4 bytes) after compression
                        // But compressed size can vary, so just check for non-empty
                        if metadata.len() == 0 {
                            // Remove empty pack files
                            let _ = fs::remove_file(&path);
                            continue;
                        }

                        // Try to validate the pack file is readable
                        // If it fails, it's likely incomplete or corrupt, so skip it
                        if PackReader::open(&path).is_err() {
                            log::warn!("Removing invalid pack file: {:?}", path);
                            let _ = fs::remove_file(&path);
                            continue;
                        }
                    }

                    if let Some(id_str) = name_str.strip_prefix("pack_").and_then(|s| s.split('.').next()) {
                        if let Ok(id) = id_str.parse::<u32>() {
                            max_id = max_id.max(id);
                        }
                    }
                }
            }
        }

        Ok(max_id + 1)
    }

    fn get_or_create_pack(&self) -> Result<Arc<Mutex<Option<PackWriter>>>> {
        let mut current_pack = self.current_pack.lock().unwrap();

        if current_pack.is_none() || current_pack.as_ref().map(|p| p.should_rotate()).unwrap_or(false) {
            // Finalize current pack if it exists and should rotate
            if let Some(pack) = current_pack.take() {
                pack.finalize()?;
                // Save index after finalizing pack
                self.save_index()?;
            }

            // Create new pack
            let pack_id = {
                let mut id = self.next_pack_id.write().unwrap();
                let current = *id;
                *id += 1;
                current
            };

            *current_pack = Some(PackWriter::new(&self.packs_dir, pack_id)?);
        }

        Ok(self.current_pack.clone())
    }

    fn get_pack_reader(&self, pack_id: u32) -> Result<Arc<PackReader>> {
        // Check if we already have a reader cached
        if let Some(reader) = self.pack_readers.get(&pack_id) {
            return Ok(reader.clone());
        }

        // Check if this is the current pack being written
        {
            let mut current_pack = self.current_pack.lock().unwrap();
            if let Some(pack) = current_pack.as_ref() {
                if pack.id == pack_id {
                    // Finalize the current pack before reading from it
                    if let Some(pack) = current_pack.take() {
                        pack.finalize()?;
                    }
                }
            }
        }

        let pack_path = self.packs_dir.join(format!("pack_{:04}.tal", pack_id));

        // Make sure the file exists before trying to open
        if !pack_path.exists() {
            return Err(anyhow!("Pack file {} not found at {:?}", pack_id, pack_path));
        }

        // Try to open the pack reader, handling incomplete files
        let reader = match PackReader::open(&pack_path) {
            Ok(r) => Arc::new(r),
            Err(e) => {
                // If the pack file is incomplete or corrupt, log and skip it
                log::warn!("Failed to open pack file {:?}: {}", pack_path, e);
                return Err(e);
            }
        };
        self.pack_readers.insert(pack_id, reader.clone());

        Ok(reader)
    }
}

impl SequenceStorageBackend for PackedSequenceStorage {
    fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool> {
        Ok(self.pack_index.contains_key(hash))
    }

    fn store_canonical(&self, sequence: &CanonicalSequence) -> Result<()> {
        // Check if already exists
        if self.sequence_exists(&sequence.sequence_hash)? {
            return Ok(());
        }

        // Serialize sequence data
        let sequence_data = rmp_serde::to_vec(sequence)?;

        // Create empty representations for now (will be updated separately)
        let empty_representations = SequenceRepresentations {
            canonical_hash: sequence.sequence_hash.clone(),
            representations: Vec::new(),
        };
        let representations_data = rmp_serde::to_vec(&empty_representations)?;

        // Get or create pack
        let pack_arc = self.get_or_create_pack()?;
        let mut pack_guard = pack_arc.lock().unwrap();

        // Write to pack
        if let Some(pack) = pack_guard.as_mut() {
            let location = pack.write_entry(
                &sequence.sequence_hash,
                &sequence_data,
                &representations_data,
            )?;

            // Update index
            self.pack_index.insert(sequence.sequence_hash.clone(), location);

            // Don't save index here - it will be saved when pack is finalized or flushed
        } else {
            // This should never happen after get_or_create_pack()
            return Err(anyhow!("Failed to get pack writer after creation"));
        }

        Ok(())
    }

    fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence> {
        let location = self
            .pack_index
            .get(hash)
            .ok_or_else(|| anyhow!("Sequence not found: {}", hash))?;

        let reader = self.get_pack_reader(location.pack_id)?;
        let (sequence_data, _) = reader.read_entry(&location)?;

        Ok(rmp_serde::from_slice(&sequence_data)?)
    }

    fn store_representations(&self, representations: &SequenceRepresentations) -> Result<()> {
        // Get current location
        let location = match self.pack_index.get(&representations.canonical_hash) {
            Some(loc) => loc.clone(),
            None => {
                // Sequence doesn't exist yet - this shouldn't happen in normal flow
                return Err(anyhow!("Cannot store representations for non-existent sequence"));
            }
        };

        // Read current data
        let reader = self.get_pack_reader(location.pack_id)?;
        let (sequence_data, existing_repr_data) = reader.read_entry(&location)?;

        // Merge representations
        let merged = if existing_repr_data.is_empty() {
            representations.clone()
        } else {
            let mut existing: SequenceRepresentations = rmp_serde::from_slice(&existing_repr_data)?;
            for repr in &representations.representations {
                existing.add_representation(repr.clone());
            }
            existing
        };

        // Serialize merged representations
        let new_repr_data = rmp_serde::to_vec(&merged)?;

        // Write updated entry to new location (append-only)
        let pack_arc = self.get_or_create_pack()?;
        let mut pack_guard = pack_arc.lock().unwrap();

        if let Some(pack) = pack_guard.as_mut() {
            let new_location = pack.write_entry(
                &representations.canonical_hash,
                &sequence_data,
                &new_repr_data,
            )?;

            // Update index to point to new location
            self.pack_index.insert(representations.canonical_hash.clone(), new_location);
            // Don't save index here - it will be saved when pack is finalized or flushed
        }

        Ok(())
    }

    fn load_representations(&self, hash: &SHA256Hash) -> Result<SequenceRepresentations> {
        let location = self
            .pack_index
            .get(hash)
            .ok_or_else(|| anyhow!("Sequence not found: {}", hash))?;

        let reader = self.get_pack_reader(location.pack_id)?;
        let (_, representations_data) = reader.read_entry(&location)?;

        if representations_data.is_empty() {
            // Return empty representations
            Ok(SequenceRepresentations {
                canonical_hash: hash.clone(),
                representations: Vec::new(),
            })
        } else {
            Ok(rmp_serde::from_slice(&representations_data)?)
        }
    }

    fn get_stats(&self) -> Result<StorageStats> {
        let total_sequences = self.pack_index.len();

        // Calculate total representations by loading and counting
        let mut total_representations = 0usize;
        for hash in self.pack_index.iter() {
            if let Ok(reprs) = self.load_representations(hash.key()) {
                total_representations += reprs.representations.len();
            }
        }

        // Calculate total size from pack files
        let mut total_size = 0u64;
        for entry in fs::read_dir(&self.packs_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("tal") {
                total_size += entry.metadata()?.len();
            }
        }

        // Calculate deduplication ratio
        let dedup_ratio = if total_sequences > 0 {
            total_representations as f32 / total_sequences as f32
        } else {
            1.0
        };

        Ok(StorageStats {
            total_sequences: Some(total_sequences),
            total_representations: Some(total_representations),
            total_size: total_size as usize,
            deduplication_ratio: dedup_ratio,
            total_chunks: 0, // Not used for sequence storage
            compressed_chunks: 0, // Not used for sequence storage
        })
    }

    fn list_all_hashes(&self) -> Result<Vec<SHA256Hash>> {
        // Return all hashes from the pack index
        Ok(self.pack_index.iter().map(|entry| entry.key().clone()).collect())
    }

    fn get_sequence_size(&self, hash: &SHA256Hash) -> Result<usize> {
        // Get the canonical sequence to find its size
        let canonical = self.load_canonical(hash)?;
        Ok(canonical.sequence.len())
    }

    fn remove_sequence(&self, hash: &SHA256Hash) -> Result<()> {
        // Remove from pack index
        self.pack_index.remove(hash);

        // Note: We don't actually delete from pack files as that would require
        // repacking. This is handled by garbage collection separately.
        // For now, just mark as removed in index.

        // Save updated index
        self.save_index()?;

        Ok(())
    }

    fn flush(&self) -> Result<()> {
        // Finalize current pack and save index
        let mut current_pack = self.current_pack.lock().unwrap();
        if let Some(pack) = current_pack.take() {
            pack.finalize()?;
        }
        self.save_index()?;
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl PackedSequenceStorage {
    /// Rebuild the sequence index by scanning all pack files
    pub fn rebuild_index(&self) -> Result<()> {
        println!("Rebuilding sequence index from pack files...");

        // Clear existing index
        self.pack_index.clear();

        // Scan all pack files
        let mut pack_count = 0;
        let mut sequence_count = 0;

        for entry in fs::read_dir(&self.packs_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only process .tal pack files
            if path.extension().and_then(|s| s.to_str()) != Some("tal") {
                continue;
            }

            // Extract pack ID from filename (pack_XXXX.tal)
            let filename = path.file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("Invalid pack filename"))?;

            if !filename.starts_with("pack_") {
                continue;
            }

            let pack_id: u32 = filename[5..].parse()
                .context("Invalid pack ID in filename")?;

            println!("  Scanning pack file: {}", filename);

            // Open pack reader
            let reader = PackReader::open(&path)?;

            // Scan through the pack file to extract all entries
            let mut offset = 9u64; // Skip header (MAGIC:4 + VERSION:1 + ID:4)

            while offset < reader.data.len() as u64 {
                // Read header length
                if offset + 4 > reader.data.len() as u64 {
                    break; // End of pack
                }

                let header_len = u32::from_le_bytes(
                    reader.data[offset as usize..offset as usize + 4]
                        .try_into()?
                ) as usize;

                // Read and parse header
                let header_start = offset as usize + 4;
                let header_end = header_start + header_len;

                if header_end > reader.data.len() {
                    break; // Incomplete entry
                }

                let header_data = &reader.data[header_start..header_end];
                let entry: PackEntry = rmp_serde::from_slice(header_data)?;

                // Record location in index
                let length = 4 + header_len as u32 + entry.sequence_length + entry.representations_length;
                let location = PackLocation {
                    pack_id,
                    offset,
                    length,
                    compressed: true, // All new packs are compressed
                };

                self.pack_index.insert(entry.hash.clone(), location);
                sequence_count += 1;

                // Move to next entry
                offset += length as u64;
            }

            // Cache the reader for future use
            self.pack_readers.insert(pack_id, Arc::new(reader));
            pack_count += 1;
        }

        println!("  Found {} sequences in {} pack files", sequence_count, pack_count);

        // Save the rebuilt index
        self.save_index()?;
        println!("  Index rebuilt and saved");

        Ok(())
    }
}

impl Drop for PackedSequenceStorage {
    fn drop(&mut self) {
        // Finalize current pack if it exists
        let mut current_pack = self.current_pack.lock().unwrap();
        if let Some(pack) = current_pack.take() {
            // Only finalize if there are entries, otherwise just drop it
            if !pack.entries.is_empty() {
                let _ = pack.finalize();
            } else {
                // Clean up empty pack file if it was created
                if pack.path.exists() {
                    let _ = fs::remove_file(&pack.path);
                }
            }
        }

        // Save index
        let _ = self.save_index();
    }
}