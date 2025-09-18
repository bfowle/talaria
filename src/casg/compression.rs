/// Compression module for efficient chunk storage
///
/// Provides specialized compression for biological sequence data,
/// using Zstandard with trained dictionaries for taxonomy-aware compression.

use crate::casg::types::{ChunkFormat, TaxonomyAwareChunk, SHA256Hash, TaxonId, SequenceRef};
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
            level: 19,  // High compression for long-term storage
            use_dictionaries: true,
            dict_min_samples: 100,
            dict_max_size: 100_000,  // 100KB max dictionary
            cache_dictionaries: true,
        }
    }
}

/// Chunk compressor with format-specific handling
pub struct ChunkCompressor {
    config: CompressionConfig,
    dictionary_cache: HashMap<u32, Vec<u8>>,  // TaxonID -> Dictionary
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
            ChunkFormat::JsonGzip => self.compress_json_gzip(data),
            ChunkFormat::Binary => self.compress_binary(data),
            ChunkFormat::BinaryDict { dict_id } => {
                self.compress_with_dictionary(data, dict_id)
            }
        }
    }

    /// Decompress chunk data based on detected or specified format
    pub fn decompress(&self, data: &[u8], format: Option<ChunkFormat>) -> Result<Vec<u8>> {
        let format = format.unwrap_or_else(|| ChunkFormat::detect(data));

        match format {
            ChunkFormat::JsonGzip => self.decompress_json_gzip(data),
            ChunkFormat::Binary => self.decompress_binary(data),
            ChunkFormat::BinaryDict { dict_id } => {
                self.decompress_with_dictionary(data, dict_id)
            }
        }
    }

    /// Legacy JSON + gzip compression (for compatibility)
    fn compress_json_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)
            .context("Failed to write to gzip encoder")?;
        encoder.finish()
            .context("Failed to finish gzip compression")
    }

    /// Legacy JSON + gzip decompression
    fn decompress_json_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::read::GzDecoder;

        let mut decoder = GzDecoder::new(data);
        let mut result = Vec::new();
        decoder.read_to_end(&mut result)
            .context("Failed to decompress gzip data")?;
        Ok(result)
    }

    /// Binary format with Zstandard compression
    fn compress_binary(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.config.level)
            .context("Failed to compress with Zstandard")
    }

    /// Binary format decompression
    fn decompress_binary(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::decode_all(data)
            .context("Failed to decompress Zstandard data")
    }

    /// Compress with trained dictionary
    fn compress_with_dictionary(&mut self, data: &[u8], dict_id: u32) -> Result<Vec<u8>> {
        let dict = self.get_or_train_dictionary(dict_id, data)?;

        // Use zstd with dictionary
        let mut encoder = zstd::Encoder::with_dictionary(Vec::new(), self.config.level, &dict)?;
        encoder.write_all(data)?;
        encoder.finish()
            .context("Failed to compress with dictionary")
    }

    /// Decompress with dictionary
    fn decompress_with_dictionary(&self, data: &[u8], dict_id: u32) -> Result<Vec<u8>> {
        let dict = self.dictionary_cache.get(&dict_id)
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
    /// Split a TaxonomyAwareChunk into components
    pub fn split(chunk: &TaxonomyAwareChunk) -> (Vec<u8>, Vec<u8>) {
        // Serialize metadata without sequence_data
        let metadata = ChunkMetadata {
            content_hash: chunk.content_hash.clone(),
            taxonomy_version: chunk.taxonomy_version.clone(),
            sequence_version: chunk.sequence_version.clone(),
            taxon_ids: chunk.taxon_ids.clone(),
            sequences: chunk.sequences.clone(),
            created_at: chunk.created_at,
            valid_from: chunk.valid_from,
            valid_until: chunk.valid_until,
            size: chunk.size,
            compressed_size: chunk.compressed_size,
        };

        let metadata_bytes = rmp_serde::to_vec(&metadata).unwrap_or_default();
        let sequence_bytes = chunk.sequence_data.clone();

        (metadata_bytes, sequence_bytes)
    }

    /// Combine metadata and sequences back into a chunk
    pub fn combine(
        metadata_bytes: &[u8],
        sequence_bytes: &[u8],
    ) -> Result<TaxonomyAwareChunk> {
        let metadata: ChunkMetadata = rmp_serde::from_slice(metadata_bytes)
            .context("Failed to deserialize chunk metadata")?;

        Ok(TaxonomyAwareChunk {
            content_hash: metadata.content_hash,
            taxonomy_version: metadata.taxonomy_version,
            sequence_version: metadata.sequence_version,
            taxon_ids: metadata.taxon_ids,
            sequences: metadata.sequences,
            sequence_data: sequence_bytes.to_vec(),
            created_at: metadata.created_at,
            valid_from: metadata.valid_from,
            valid_until: metadata.valid_until,
            size: metadata.size,
            compressed_size: metadata.compressed_size,
        })
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
        // Gzip magic bytes
        let gzip_data = vec![0x1f, 0x8b, 0x08, 0x00];
        assert_eq!(ChunkFormat::detect(&gzip_data), ChunkFormat::JsonGzip);

        // Zstandard magic bytes
        let zstd_data = vec![0x28, 0xb5, 0x2f, 0xfd];
        assert_eq!(ChunkFormat::detect(&zstd_data), ChunkFormat::Binary);

        // Unknown data defaults to JsonGzip
        let unknown = vec![0x00, 0x01, 0x02, 0x03];
        assert_eq!(ChunkFormat::detect(&unknown), ChunkFormat::JsonGzip);
    }

    #[test]
    fn test_compression_roundtrip() {
        let config = CompressionConfig::default();
        let mut compressor = ChunkCompressor::new(config);

        let test_data = b"ACGTACGTACGTACGT".repeat(1000);

        // Test each format
        for format in [ChunkFormat::JsonGzip, ChunkFormat::Binary] {
            let compressed = compressor.compress(&test_data, format, None).unwrap();
            let decompressed = compressor.decompress(&compressed, Some(format)).unwrap();

            assert_eq!(test_data, decompressed);
            // Compression should reduce size
            assert!(compressed.len() < test_data.len());
        }
    }
}