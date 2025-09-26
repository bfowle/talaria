#!/usr/bin/env cargo script
//! ```cargo
//! [dependencies]
//! anyhow = "1.0"
//! rmp-serde = "1.1"
//! dashmap = "5.5"
//! zstd = "0.12"
//! serde = { version = "1.0", features = ["derive"] }
//! ```

use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, Context};
use dashmap::DashMap;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SHA256Hash([u8; 32]);

impl SHA256Hash {
    fn from_hex(hex: &str) -> Result<Self> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(hex, &mut bytes)?;
        Ok(SHA256Hash(bytes))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackLocation {
    pack_id: u32,
    offset: u64,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackHeader {
    version: u8,
    entry_count: u32,
    index_offset: u64,
}

fn main() -> Result<()> {
    let home = std::env::var("HOME")?;
    let packs_dir = PathBuf::from(home).join(".talaria/databases/sequences/packs");
    let indices_dir = PathBuf::from(home).join(".talaria/databases/sequences/indices");

    println!("Rebuilding sequence index from pack files...");
    println!("Pack directory: {:?}", packs_dir);

    let index = DashMap::new();

    // Scan all pack files
    for entry in fs::read_dir(&packs_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension() == Some(std::ffi::OsStr::new("tal")) {
            let filename = path.file_stem().unwrap().to_string_lossy();
            if let Some(pack_id_str) = filename.strip_prefix("pack_") {
                let pack_id: u32 = pack_id_str.parse()?;
                println!("Processing pack file: {} (ID: {})", filename, pack_id);

                // Read and decompress pack file
                let compressed_data = fs::read(&path)?;
                let data = zstd::decode_all(&compressed_data[..])?;

                // Parse pack format
                // Pack files contain:
                // - Header
                // - Multiple entries (each with hash, sequence data, representations)
                // - Index at the end

                // For now, just report the size
                println!("  Decompressed size: {} bytes", data.len());

                // TODO: Parse the actual pack format and extract sequence hashes
                // This would require understanding the exact binary format used
            }
        }
    }

    // Save the index
    let index_path = indices_dir.join("sequence_index.tal");

    // Convert DashMap to HashMap for serialization
    let index_map: HashMap<SHA256Hash, PackLocation> = index.into_iter().collect();

    // Serialize and compress
    let serialized = rmp_serde::to_vec(&index_map)?;
    let compressed = zstd::encode_all(&serialized[..], 3)?;

    fs::write(&index_path, compressed)?;
    println!("Saved index to: {:?}", index_path);

    Ok(())
}