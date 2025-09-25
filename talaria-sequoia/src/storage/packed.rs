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

        // Decompress if it's Zstandard compressed
        let data = if compressed_data.len() >= 4
            && compressed_data[0] == 0x28
            && compressed_data[1] == 0xb5 {
            // Zstandard magic bytes detected - decompress
            zstd::decode_all(&compressed_data[..])?
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
            // Load existing index (TALARIA format: MessagePack + Zstandard)
            let compressed_data = fs::read(&index_path)?;

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
        } else {
            // Create new index
            Ok(DashMap::new())
        }
    }

    fn save_index(&self) -> Result<()> {
        let index_path = self.indices_dir.join("sequence_index.tal");

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

                if name_str.starts_with("pack_") {
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

        let reader = Arc::new(PackReader::open(&pack_path)?);
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

        // Calculate total size from pack files
        let mut total_size = 0u64;
        for entry in fs::read_dir(&self.packs_dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("tal") {
                total_size += entry.metadata()?.len();
            }
        }

        Ok(StorageStats {
            total_sequences: Some(total_sequences),
            total_representations: Some(total_sequences), // Assume 1:1 for now
            total_size: total_size as usize,
            deduplication_ratio: 1.0, // No dedup metrics tracked currently
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
}

impl Drop for PackedSequenceStorage {
    fn drop(&mut self) {
        // Finalize current pack if it exists
        let mut current_pack = self.current_pack.lock().unwrap();
        if let Some(pack) = current_pack.take() {
            let _ = pack.finalize();
        }

        // Save index
        let _ = self.save_index();
    }
}