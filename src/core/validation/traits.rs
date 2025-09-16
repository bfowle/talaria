/// Trait definitions for validation components
///
/// Provides abstractions for validating sequences, chunks,
/// manifests, and other data structures.

use anyhow::Result;
use std::path::Path;
use crate::bio::sequence::Sequence;
use crate::casg::types::{TaxonomyAwareChunk, SHA256Hash};

/// Common interface for validators
pub trait Validator: Send + Sync {
    /// Validate a target
    fn validate(&self, target: &Path) -> Result<ValidationResult>;

    /// Get validator name
    fn name(&self) -> &str;

    /// Check if validator can handle this file type
    fn can_validate(&self, path: &Path) -> bool;

    /// Get validation rules
    fn rules(&self) -> &[ValidationRule];

    /// Set strictness level
    fn set_strictness(&mut self, level: StrictnessLevel);
}

/// Sequence validation
pub trait SequenceValidator: Validator {
    /// Validate a single sequence
    fn validate_sequence(&self, sequence: &Sequence) -> Result<SequenceValidation>;

    /// Validate multiple sequences
    fn validate_sequences(&self, sequences: &[Sequence]) -> Result<Vec<SequenceValidation>>;

    /// Check sequence integrity
    fn check_integrity(&self, sequence: &Sequence) -> bool;

    /// Repair sequence if possible
    fn repair_sequence(&self, sequence: &mut Sequence) -> Result<RepairResult>;

    /// Get allowed characters for sequence type
    fn allowed_characters(&self) -> &[u8];
}

/// Chunk validation for CASG
pub trait ChunkValidator: Validator {
    /// Validate a chunk
    fn validate_chunk(&self, chunk: &TaxonomyAwareChunk) -> Result<ChunkValidation>;

    /// Verify chunk hash
    fn verify_hash(&self, chunk: &TaxonomyAwareChunk) -> bool;

    /// Check chunk consistency
    fn check_consistency(&self, chunk: &TaxonomyAwareChunk) -> Result<ConsistencyCheck>;

    /// Validate chunk references
    fn validate_references(
        &self,
        chunk: &TaxonomyAwareChunk,
        available_chunks: &[SHA256Hash],
    ) -> Result<ReferenceValidation>;
}

/// FASTA file validation
pub trait FastaValidator: Validator {
    /// Validate FASTA format
    fn validate_fasta(&self, path: &Path) -> Result<FastaValidation>;

    /// Check for duplicate IDs
    fn check_duplicates(&self, path: &Path) -> Result<Vec<String>>;

    /// Validate headers
    fn validate_headers(&self, path: &Path) -> Result<HeaderValidation>;

    /// Get FASTA statistics
    fn get_stats(&self, path: &Path) -> Result<FastaStats>;
}

/// Delta validation
pub trait DeltaValidator: Validator {
    /// Validate delta records
    fn validate_deltas(
        &self,
        delta_path: &Path,
        reference_path: &Path,
    ) -> Result<DeltaValidation>;

    /// Verify reconstruction
    fn verify_reconstruction(
        &self,
        delta_path: &Path,
        reference_path: &Path,
        original_path: &Path,
    ) -> Result<ReconstructionValidation>;

    /// Check delta efficiency
    fn check_efficiency(
        &self,
        delta_path: &Path,
        reference_path: &Path,
    ) -> Result<EfficiencyReport>;
}

// Supporting types

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub info: Vec<String>,
    pub stats: ValidationStats,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub location: ErrorLocation,
    pub message: String,
    pub severity: ErrorSeverity,
    pub rule: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub location: ErrorLocation,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ErrorLocation {
    Line(usize),
    Sequence(String),
    Chunk(SHA256Hash),
    File(String),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone)]
pub struct ValidationStats {
    pub total_items: usize,
    pub valid_items: usize,
    pub invalid_items: usize,
    pub repaired_items: usize,
    pub processing_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub name: String,
    pub description: String,
    pub category: RuleCategory,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleCategory {
    Format,
    Integrity,
    Consistency,
    Performance,
    Compatibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictnessLevel {
    Permissive,
    Normal,
    Strict,
    Paranoid,
}

#[derive(Debug, Clone)]
pub struct SequenceValidation {
    pub id: String,
    pub valid: bool,
    pub issues: Vec<SequenceIssue>,
}

#[derive(Debug, Clone)]
pub enum SequenceIssue {
    InvalidCharacter { position: usize, character: u8 },
    TooShort { length: usize, min_length: usize },
    TooLong { length: usize, max_length: usize },
    InvalidHeader(String),
    MissingTaxonomy,
    DuplicateId,
}

#[derive(Debug, Clone)]
pub struct RepairResult {
    pub repaired: bool,
    pub changes: Vec<RepairChange>,
}

#[derive(Debug, Clone)]
pub enum RepairChange {
    CharacterReplaced { position: usize, old: u8, new: u8 },
    CharacterRemoved { position: usize, character: u8 },
    HeaderFixed(String),
    TaxonomyAdded(u32),
}

#[derive(Debug, Clone)]
pub struct ChunkValidation {
    pub hash: SHA256Hash,
    pub valid: bool,
    pub hash_matches: bool,
    pub references_valid: bool,
    pub taxonomy_valid: bool,
}

#[derive(Debug, Clone)]
pub struct ConsistencyCheck {
    pub consistent: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReferenceValidation {
    pub all_present: bool,
    pub missing: Vec<SHA256Hash>,
    pub circular: bool,
}

#[derive(Debug, Clone)]
pub struct FastaValidation {
    pub valid: bool,
    pub sequence_count: usize,
    pub format_errors: Vec<FormatError>,
    pub duplicate_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FormatError {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct HeaderValidation {
    pub valid_count: usize,
    pub invalid_count: usize,
    pub invalid_headers: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub struct FastaStats {
    pub sequence_count: usize,
    pub total_length: usize,
    pub min_length: usize,
    pub max_length: usize,
    pub avg_length: f64,
    pub gc_content: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct DeltaValidation {
    pub valid: bool,
    pub delta_count: usize,
    pub reference_coverage: f64,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReconstructionValidation {
    pub successful: bool,
    pub matches_original: bool,
    pub sequence_count: usize,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EfficiencyReport {
    pub compression_ratio: f64,
    pub avg_delta_ops: f64,
    pub max_delta_ops: usize,
    pub reconstruction_time_ms: u64,
}