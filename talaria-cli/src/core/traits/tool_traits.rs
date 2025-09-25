#![allow(dead_code)]

// This module defines tool and aligner trait abstractions for future integrations.
// These traits will be implemented by various aligners and analysis tools
// to provide a unified interface for sequence analysis.
// TODO: Implement adapters for additional aligners (HMMER, USEARCH, etc.)

/// Traits for alignment tools
use talaria_bio::sequence::Sequence;
use anyhow::Result;
use std::path::Path;

// Re-export from talaria-tools
pub use talaria_tools::traits::{AlignmentSummary as AlignmentResult, AlignmentConfig};

/// Trait for alignment tools
pub trait Aligner: Send + Sync {
    /// Perform alignment search
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>>;

    /// Get tool version
    fn version(&self) -> Result<String>;

    /// Check if tool is available
    fn is_available(&self) -> bool;

    /// Get recommended batch size
    fn recommended_batch_size(&self) -> usize {
        1000
    }

    /// Check if supports protein sequences
    fn supports_protein(&self) -> bool {
        true
    }

    /// Check if supports nucleotide sequences
    fn supports_nucleotide(&self) -> bool {
        true
    }
}

/// Trait for configurable alignment tools
pub trait ConfigurableAligner: Aligner {
    /// Set configuration
    fn set_config(&mut self, config: AlignmentConfig);

    /// Get current configuration
    fn get_config(&self) -> &AlignmentConfig;

    /// Set output path
    fn set_output_path(&mut self, path: &Path);

    /// Set temporary directory
    fn set_temp_dir(&mut self, path: &Path);
}
