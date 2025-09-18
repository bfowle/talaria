/// Trait for pluggable manifest serialization formats
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;


/// Magic bytes for Talaria format
pub const TALARIA_MAGIC: &[u8] = b"TAL\x01";

/// Trait for different manifest serialization formats
pub trait ManifestFormat: Send + Sync {
    /// Get the file extension for this format
    fn extension(&self) -> &str;

    /// Serialize a JSON value to bytes
    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>>;

    /// Deserialize bytes to a JSON value
    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value>;

    /// Check if a file uses this format (by magic bytes or extension)
    fn is_format(&self, path: &Path) -> bool;

    /// Get estimated compression ratio compared to JSON (1.0 = same size)
    fn compression_ratio(&self) -> f32;

    /// Get format name for display
    fn name(&self) -> &str;

    /// Check if format supports streaming (for large manifests)
    fn supports_streaming(&self) -> bool {
        false
    }
}

/// Helper functions for concrete types
pub fn serialize<T: Serialize>(format: &dyn ManifestFormat, manifest: &T) -> Result<Vec<u8>> {
    let json_value = serde_json::to_value(manifest)?;
    format.serialize_value(&json_value)
}

pub fn deserialize<T: for<'de> Deserialize<'de>>(format: &dyn ManifestFormat, data: &[u8]) -> Result<T> {
    let json_value = format.deserialize_value(data)?;
    Ok(serde_json::from_value(json_value)?)
}

/// Talaria binary format implementation
pub struct TalariaFormat;

impl ManifestFormat for TalariaFormat {
    fn extension(&self) -> &str {
        "tal"
    }

    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(TALARIA_MAGIC.len() + 1024 * 512);
        data.extend_from_slice(TALARIA_MAGIC);
        data.extend_from_slice(&rmp_serde::to_vec(value)?);
        Ok(data)
    }

    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value> {
        // Check and skip magic header if present
        let content = if data.starts_with(TALARIA_MAGIC) {
            &data[TALARIA_MAGIC.len()..]
        } else {
            data
        };
        Ok(rmp_serde::from_slice(content)?)
    }

    fn is_format(&self, path: &Path) -> bool {
        if path.extension().and_then(|e| e.to_str()) == Some("tal") {
            return true;
        }

        // Check magic bytes
        if let Ok(mut file) = std::fs::File::open(path) {
            let mut magic = vec![0u8; TALARIA_MAGIC.len()];
            if let Ok(_) = std::io::Read::read_exact(&mut file, &mut magic) {
                return magic == TALARIA_MAGIC;
            }
        }
        false
    }

    fn compression_ratio(&self) -> f32 {
        0.09  // ~91% size reduction compared to JSON
    }

    fn name(&self) -> &str {
        "Talaria Binary Format"
    }
}

/// JSON format implementation
pub struct JsonFormat {
    pretty: bool,
}

impl JsonFormat {
    pub fn new() -> Self {
        Self { pretty: true }
    }

    pub fn compact() -> Self {
        Self { pretty: false }
    }
}

impl ManifestFormat for JsonFormat {
    fn extension(&self) -> &str {
        "json"
    }

    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>> {
        if self.pretty {
            Ok(serde_json::to_vec_pretty(value)?)
        } else {
            Ok(serde_json::to_vec(value)?)
        }
    }

    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value> {
        Ok(serde_json::from_slice(data)?)
    }

    fn is_format(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("json")
    }

    fn compression_ratio(&self) -> f32 {
        1.0  // Baseline
    }

    fn name(&self) -> &str {
        "JSON"
    }
}

/// MessagePack format (without Talaria header)
pub struct MessagePackFormat;

impl ManifestFormat for MessagePackFormat {
    fn extension(&self) -> &str {
        "msgpack"
    }

    fn serialize_value(&self, value: &serde_json::Value) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec(value)?)
    }

    fn deserialize_value(&self, data: &[u8]) -> Result<serde_json::Value> {
        Ok(rmp_serde::from_slice(data)?)
    }

    fn is_format(&self, path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some("msgpack")
    }

    fn compression_ratio(&self) -> f32 {
        0.10  // Similar to Talaria format without header
    }

    fn name(&self) -> &str {
        "MessagePack"
    }
}

/// Format auto-detection
pub struct FormatDetector;

impl FormatDetector {
    /// Detect format from a file path
    pub fn detect(path: &Path) -> Box<dyn ManifestFormat> {
        // Check by extension first
        match path.extension().and_then(|e| e.to_str()) {
            Some("tal") => Box::new(TalariaFormat),
            Some("json") => Box::new(JsonFormat::new()),
            Some("msgpack") => Box::new(MessagePackFormat),
            _ => {
                // Try to detect by content
                if TalariaFormat.is_format(path) {
                    Box::new(TalariaFormat)
                } else {
                    // Default to JSON
                    Box::new(JsonFormat::new())
                }
            }
        }
    }

    /// List all supported formats
    pub fn supported_formats() -> Vec<Box<dyn ManifestFormat>> {
        vec![
            Box::new(TalariaFormat),
            Box::new(JsonFormat::new()),
            Box::new(MessagePackFormat),
        ]
    }
}