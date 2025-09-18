/// Trait for validating manifest integrity and consistency
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::casg::types::{SHA256Hash, ChunkMetadata, TemporalManifest};

/// Validation error types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationError {
    /// Chunk hash doesn't match content
    HashMismatch {
        chunk_hash: SHA256Hash,
        expected: SHA256Hash,
        actual: SHA256Hash,
    },
    /// Chunk is missing from storage
    MissingChunk {
        chunk_hash: SHA256Hash,
    },
    /// Chunk size doesn't match manifest
    SizeMismatch {
        chunk_hash: SHA256Hash,
        expected: usize,
        actual: usize,
    },
    /// Duplicate chunk entries
    DuplicateChunk {
        chunk_hash: SHA256Hash,
        count: usize,
    },
    /// Invalid chunk offset or length
    InvalidChunkBounds {
        chunk_hash: SHA256Hash,
        offset: usize,
        length: usize,
    },
    /// Overlapping chunks
    OverlappingChunks {
        chunk1: SHA256Hash,
        chunk2: SHA256Hash,
    },
    /// TemporalManifest version is unsupported
    UnsupportedVersion {
        version: String,
    },
    /// TemporalManifest is corrupted
    CorruptedTemporalManifest {
        reason: String,
    },
    /// Missing required field
    MissingField {
        field: String,
    },
    /// Invalid metadata
    InvalidMetadata {
        key: String,
        reason: String,
    },
}

/// Result of validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the manifest is valid
    pub is_valid: bool,
    /// All validation errors found
    pub errors: Vec<ValidationError>,
    /// Validation warnings (non-fatal)
    pub warnings: Vec<String>,
    /// Statistics about the validation
    pub stats: ValidationStats,
}

/// Statistics from validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationStats {
    /// Number of chunks validated
    pub chunks_validated: usize,
    /// Total size validated
    pub bytes_validated: usize,
    /// Time taken for validation (ms)
    pub validation_time_ms: u64,
    /// Number of chunks that were verified against storage
    pub chunks_verified: usize,
}

/// Options for validation
#[derive(Debug, Clone, Default)]
pub struct ValidationOptions {
    /// Verify chunk hashes against actual content
    pub verify_hashes: bool,
    /// Check for missing chunks in storage
    pub check_storage: bool,
    /// Verify chunk sizes
    pub verify_sizes: bool,
    /// Check for overlapping chunks
    pub check_overlaps: bool,
    /// Check metadata validity
    pub check_metadata: bool,
    /// Stop on first error
    pub fail_fast: bool,
    /// Maximum chunks to validate (0 = all)
    pub max_chunks: usize,
}

/// Trait for manifest validation
#[async_trait]
pub trait TemporalManifestValidator: Send + Sync {
    /// Validate a manifest
    async fn validate(&self, manifest: &TemporalManifest, options: ValidationOptions) -> Result<ValidationResult>;

    /// Validate a manifest file
    async fn validate_file(&self, path: &PathBuf, options: ValidationOptions) -> Result<ValidationResult>;

    /// Quick validation (structure only, no content checks)
    async fn quick_validate(&self, manifest: &TemporalManifest) -> Result<bool>;

    /// Validate chunk integrity
    async fn validate_chunk(&self, chunk: &ChunkMetadata, content: &[u8]) -> Result<Vec<ValidationError>>;

    /// Repair a manifest if possible
    async fn repair(&mut self, manifest: &mut TemporalManifest, errors: &[ValidationError]) -> Result<usize>;

    /// Check compatibility between two manifests
    async fn check_compatibility(&self, old: &TemporalManifest, new: &TemporalManifest) -> Result<bool>;

    /// Validate manifest metadata
    async fn validate_metadata(&self, metadata: &HashMap<String, String>) -> Result<Vec<ValidationError>>;
}

/// Standard implementation of TemporalManifestValidator
pub struct StandardTemporalManifestValidator {
    /// Base path for chunk storage
    chunks_dir: PathBuf,
    /// Supported manifest versions
    supported_versions: HashSet<String>,
}

impl StandardTemporalManifestValidator {
    pub fn new(chunks_dir: PathBuf) -> Self {
        let mut supported_versions = HashSet::new();
        supported_versions.insert("1.0.0".to_string());
        supported_versions.insert("1.1.0".to_string());

        Self {
            chunks_dir,
            supported_versions,
        }
    }

    fn check_structure(&self, manifest: &TemporalManifest) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        // Check version
        if !self.supported_versions.contains(&manifest.version) {
            errors.push(ValidationError::UnsupportedVersion {
                version: manifest.version.clone(),
            });
        }

        // Check for duplicate chunks
        let mut seen_hashes = HashMap::new();
        for chunk in &manifest.chunk_index {
            *seen_hashes.entry(chunk.hash.clone()).or_insert(0) += 1;
        }

        for (hash, count) in seen_hashes {
            if count > 1 {
                errors.push(ValidationError::DuplicateChunk {
                    chunk_hash: hash,
                    count,
                });
            }
        }

        // Check for invalid bounds
        for chunk in &manifest.chunk_index {
            if chunk.size == 0 {
                errors.push(ValidationError::InvalidChunkBounds {
                    chunk_hash: chunk.hash.clone(),
                    offset: 0,  // ChunkMetadata doesn't have offset
                    length: chunk.size,
                });
            }
        }

        errors
    }

    fn check_overlaps(&self, _manifest: &TemporalManifest) -> Vec<ValidationError> {
        // ChunkMetadata doesn't have offset field in current implementation
        // Overlap checking would require different approach
        Vec::new()
    }

    async fn verify_chunk_storage(&self, chunk: &ChunkMetadata) -> Option<ValidationError> {
        let chunk_path = self.chunks_dir.join(chunk.hash.to_hex());

        if !chunk_path.exists() {
            // Check compressed version
            let compressed_path = chunk_path.with_extension("gz");
            if !compressed_path.exists() {
                return Some(ValidationError::MissingChunk {
                    chunk_hash: chunk.hash.clone(),
                });
            }
        }

        // Verify size if file exists
        if let Ok(metadata) = std::fs::metadata(&chunk_path) {
            let file_size = metadata.len() as usize;
            if file_size != chunk.size {
                return Some(ValidationError::SizeMismatch {
                    chunk_hash: chunk.hash.clone(),
                    expected: chunk.size,
                    actual: file_size,
                });
            }
        }

        None
    }

    async fn verify_chunk_hash(&self, chunk: &ChunkMetadata) -> Option<ValidationError> {
        let chunk_path = self.chunks_dir.join(chunk.hash.to_hex());

        if let Ok(content) = std::fs::read(&chunk_path) {
            let actual_hash = SHA256Hash::compute(&content);
            if actual_hash != chunk.hash {
                return Some(ValidationError::HashMismatch {
                    chunk_hash: chunk.hash.clone(),
                    expected: chunk.hash.clone(),
                    actual: actual_hash,
                });
            }
        }

        None
    }
}

#[async_trait]
impl TemporalManifestValidator for StandardTemporalManifestValidator {
    async fn validate(&self, manifest: &TemporalManifest, options: ValidationOptions) -> Result<ValidationResult> {
        let start_time = std::time::Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut chunks_verified = 0;

        // Always check structure
        errors.extend(self.check_structure(manifest));

        if options.fail_fast && !errors.is_empty() {
            return Ok(ValidationResult {
                is_valid: false,
                errors,
                warnings,
                stats: ValidationStats {
                    chunks_validated: 0,
                    bytes_validated: 0,
                    validation_time_ms: start_time.elapsed().as_millis() as u64,
                    chunks_verified: 0,
                },
            });
        }

        // Check overlaps if requested
        if options.check_overlaps {
            errors.extend(self.check_overlaps(manifest));
            if options.fail_fast && !errors.is_empty() {
                return Ok(ValidationResult {
                    is_valid: false,
                    errors,
                    warnings,
                    stats: ValidationStats {
                        chunks_validated: 0,
                        bytes_validated: 0,
                        validation_time_ms: start_time.elapsed().as_millis() as u64,
                        chunks_verified: 0,
                    },
                });
            }
        }

        // Validate individual chunks
        let max_chunks = if options.max_chunks > 0 {
            options.max_chunks.min(manifest.chunk_index.len())
        } else {
            manifest.chunk_index.len()
        };

        let mut bytes_validated = 0;
        for chunk in manifest.chunk_index.iter().take(max_chunks) {
            if options.check_storage {
                if let Some(error) = self.verify_chunk_storage(chunk).await {
                    errors.push(error);
                    if options.fail_fast {
                        break;
                    }
                }
                chunks_verified += 1;
            }

            if options.verify_hashes {
                if let Some(error) = self.verify_chunk_hash(chunk).await {
                    errors.push(error);
                    if options.fail_fast {
                        break;
                    }
                }
                chunks_verified += 1;
            }

            bytes_validated += chunk.size;
        }

        // Add warnings for partial validation
        if max_chunks < manifest.chunk_index.len() {
            warnings.push(format!(
                "Only validated {} of {} chunks",
                max_chunks,
                manifest.chunk_index.len()
            ));
        }

        let is_valid = errors.is_empty();

        Ok(ValidationResult {
            is_valid,
            errors,
            warnings,
            stats: ValidationStats {
                chunks_validated: max_chunks,
                bytes_validated,
                validation_time_ms: start_time.elapsed().as_millis() as u64,
                chunks_verified,
            },
        })
    }

    async fn validate_file(&self, path: &PathBuf, options: ValidationOptions) -> Result<ValidationResult> {
        let data = std::fs::read(path)?;

        // Auto-detect format
        // For now, use JSON deserialization
        let manifest: TemporalManifest = serde_json::from_slice(&data)?;

        self.validate(&manifest, options).await
    }

    async fn quick_validate(&self, manifest: &TemporalManifest) -> Result<bool> {
        let errors = self.check_structure(manifest);
        Ok(errors.is_empty())
    }

    async fn validate_chunk(&self, chunk: &ChunkMetadata, content: &[u8]) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check hash
        let actual_hash = SHA256Hash::compute(content);
        if actual_hash != chunk.hash {
            errors.push(ValidationError::HashMismatch {
                chunk_hash: chunk.hash.clone(),
                expected: chunk.hash.clone(),
                actual: actual_hash,
            });
        }

        // Check size
        if content.len() != chunk.size {
            errors.push(ValidationError::SizeMismatch {
                chunk_hash: chunk.hash.clone(),
                expected: chunk.size,
                actual: content.len(),
            });
        }

        Ok(errors)
    }

    async fn repair(&mut self, manifest: &mut TemporalManifest, errors: &[ValidationError]) -> Result<usize> {
        let mut repaired = 0;

        for error in errors {
            match error {
                ValidationError::DuplicateChunk { chunk_hash, .. } => {
                    // Remove duplicate entries
                    let mut seen = false;
                    manifest.chunk_index.retain(|c| {
                        if c.hash == *chunk_hash {
                            if seen {
                                repaired += 1;
                                false
                            } else {
                                seen = true;
                                true
                            }
                        } else {
                            true
                        }
                    });
                }
                ValidationError::InvalidChunkBounds { chunk_hash, .. } => {
                    // Remove invalid chunks
                    manifest.chunk_index.retain(|c| {
                        if c.hash == *chunk_hash {
                            repaired += 1;
                            false
                        } else {
                            true
                        }
                    });
                }
                _ => {
                    // Other errors can't be automatically repaired
                }
            }
        }

        Ok(repaired)
    }

    async fn check_compatibility(&self, old: &TemporalManifest, new: &TemporalManifest) -> Result<bool> {
        // Check if versions are compatible
        Ok(self.supported_versions.contains(&old.version) &&
           self.supported_versions.contains(&new.version))
    }

    async fn validate_metadata(&self, metadata: &HashMap<String, String>) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check for required metadata fields
        let required_fields = ["created_at", "database_source"];
        for field in &required_fields {
            if !metadata.contains_key(*field) {
                errors.push(ValidationError::MissingField {
                    field: field.to_string(),
                });
            }
        }

        // Validate specific metadata values
        if let Some(created_at) = metadata.get("created_at") {
            if chrono::DateTime::parse_from_rfc3339(created_at).is_err() {
                errors.push(ValidationError::InvalidMetadata {
                    key: "created_at".to_string(),
                    reason: "Invalid datetime format".to_string(),
                });
            }
        }

        Ok(errors)
    }
}