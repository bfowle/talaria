/// Compression module for efficient chunk storage
///
/// Provides specialized compression for biological sequence data,
/// using Zstandard with trained dictionaries for taxonomy-aware compression.
use crate::types::{ChunkFormat, ChunkManifest, SequenceRef};
use talaria_core::types::{SHA256Hash, TaxonId};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Compression level (1-22 for zstd, higher = better compression, slower)
    pub level: i32,
    /// Enable dictionary training for taxonomy groups
    pub use_dictionaries: bool,
    /// Minimum sequences needed to train a dictionary
    pub dict_min_samples: usize,
    /// Maximum dictionary size in bytes
    pub dict_max_size: usize,
    /// Cache trained dictionaries
    pub cache_dictionaries: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            level: 19, // High compression for long-term storage
            use_dictionaries: true,
            dict_min_samples: 100,
            dict_max_size: 100_000, // 100KB max dictionary
            cache_dictionaries: true,
        }
    }
}

/// Chunk compressor with format-specific handling
pub struct ChunkCompressor {
    config: CompressionConfig,
    dictionary_cache: HashMap<u32, Vec<u8>>, // TaxonID -> Dictionary
}

impl ChunkCompressor {
    pub fn new(config: CompressionConfig) -> Self {
        Self {
            config,
            dictionary_cache: HashMap::new(),
        }
    }

    /// Compress chunk data using the specified format
    pub fn compress(
        &mut self,
        data: &[u8],
        format: ChunkFormat,
        _taxon_id: Option<u32>,
    ) -> Result<Vec<u8>> {
        match format {
            ChunkFormat::Binary => self.compress_binary(data),
            ChunkFormat::BinaryDict { dict_id } => self.compress_with_dictionary(data, dict_id),
        }
    }

    /// Decompress chunk data based on detected or specified format
    pub fn decompress(&self, data: &[u8], format: Option<ChunkFormat>) -> Result<Vec<u8>> {
        let format = format.unwrap_or_else(|| ChunkFormat::detect(data));

        match format {
            ChunkFormat::Binary => self.decompress_binary(data),
            ChunkFormat::BinaryDict { dict_id } => self.decompress_with_dictionary(data, dict_id),
        }
    }

    /// Binary format with Zstandard compression
    fn compress_binary(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.config.level).context("Failed to compress with Zstandard")
    }

    /// Compress with maximum compression level
    pub fn compress_max(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Use maximum zstd compression level (22)
        zstd::encode_all(data, 22).context("Failed to compress with maximum level")
    }

    /// Binary format decompression
    fn decompress_binary(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Check if data is actually compressed (zstd magic bytes: 0x28 0xB5 0x2F 0xFD)
        if ChunkFormat::is_compressed(data) {
            zstd::decode_all(data).context("Failed to decompress Zstandard data")
        } else {
            // Data is not compressed, return as-is
            Ok(data.to_vec())
        }
    }

    /// Compress with trained dictionary
    fn compress_with_dictionary(&mut self, data: &[u8], dict_id: u32) -> Result<Vec<u8>> {
        let dict = self.get_or_train_dictionary(dict_id, data)?;

        // Use zstd with dictionary
        let mut encoder = zstd::Encoder::with_dictionary(Vec::new(), self.config.level, &dict)?;
        encoder.write_all(data)?;
        encoder
            .finish()
            .context("Failed to compress with dictionary")
    }

    /// Decompress with dictionary
    fn decompress_with_dictionary(&self, data: &[u8], dict_id: u32) -> Result<Vec<u8>> {
        let dict = self
            .dictionary_cache
            .get(&dict_id)
            .ok_or_else(|| anyhow::anyhow!("Dictionary {} not found in cache", dict_id))?;

        let mut decoder = zstd::Decoder::with_dictionary(data, dict)?;
        let mut result = Vec::new();
        decoder.read_to_end(&mut result)?;
        Ok(result)
    }

    /// Get dictionary from cache or train a new one
    fn get_or_train_dictionary(&mut self, taxon_id: u32, sample_data: &[u8]) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(dict) = self.dictionary_cache.get(&taxon_id) {
            return Ok(dict.clone());
        }

        // Train new dictionary (simplified - in production would use multiple samples)
        let dict = self.train_dictionary(&[sample_data])?;

        if self.config.cache_dictionaries {
            self.dictionary_cache.insert(taxon_id, dict.clone());
        }

        Ok(dict)
    }

    /// Train a Zstandard dictionary from samples
    fn train_dictionary(&self, _samples: &[&[u8]]) -> Result<Vec<u8>> {
        // For now, return empty dictionary (zstd will use default)
        // Full implementation would use zstd::dict::from_samples
        Ok(Vec::new())
    }
}

/// Split chunk data into metadata and sequences for optimal compression
pub struct ChunkSplitter;

impl ChunkSplitter {
    /// Split a ChunkManifest into components (for compression)
    pub fn split_manifest(manifest: &ChunkManifest) -> (Vec<u8>, Vec<u8>) {
        // Serialize the manifest metadata
        let metadata_bytes = rmp_serde::to_vec(&manifest).unwrap_or_default();
        // No sequence data in manifests - they only contain references
        let empty_bytes = Vec::new();
        (metadata_bytes, empty_bytes)
    }

    /// Combine metadata back into a manifest
    pub fn combine_manifest(metadata_bytes: &[u8]) -> Result<ChunkManifest> {
        let manifest: ChunkManifest = rmp_serde::from_slice(metadata_bytes)
            .context("Failed to deserialize chunk manifest")?;
        Ok(manifest)
    }
}

/// Chunk metadata (without sequence data)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkMetadata {
    pub content_hash: SHA256Hash,
    pub taxonomy_version: SHA256Hash,
    pub sequence_version: SHA256Hash,
    pub taxon_ids: Vec<TaxonId>,
    pub sequences: Vec<SequenceRef>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub size: usize,
    pub compressed_size: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection() {
        // Zstandard magic bytes (primary format)
        let zstd_data = vec![0x28, 0xb5, 0x2f, 0xfd];
        assert_eq!(ChunkFormat::detect(&zstd_data), ChunkFormat::Binary);

        // Non-Zstandard data defaults to Binary (no gzip support anymore)
        let other_data = vec![0x1f, 0x8b, 0x08, 0x00];
        assert_eq!(ChunkFormat::detect(&other_data), ChunkFormat::Binary);

        // Unknown data also defaults to Binary
        let unknown = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(ChunkFormat::detect(&unknown), ChunkFormat::Binary);
    }

    #[test]
    fn test_compression_roundtrip() {
        let config = CompressionConfig::default();
        let mut compressor = ChunkCompressor::new(config);

        let test_data = b"ACGTACGTACGTACGT".repeat(1000);

        // Test Binary format (the only supported format now)
        let format = ChunkFormat::Binary;
        let compressed = compressor.compress(&test_data, format, None).unwrap();
        let decompressed = compressor.decompress(&compressed, Some(format)).unwrap();

        assert_eq!(test_data, decompressed);
        // Compression should reduce size
        assert!(compressed.len() < test_data.len());
    }
}
