//! Mock aligner for testing and graph-based selection

use crate::traits::{Aligner, AlignmentSummary};
use anyhow::Result;

/// Mock aligner for testing and graph-based selection
pub struct MockAligner;

impl Default for MockAligner {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAligner {
    pub fn new() -> Self {
        MockAligner
    }
}

impl Aligner for MockAligner {
    fn search(
        &mut self,
        _query: &[talaria_bio::sequence::Sequence],
        _reference: &[talaria_bio::sequence::Sequence],
    ) -> Result<Vec<AlignmentSummary>> {
        // Return empty results - actual alignments are provided separately
        Ok(Vec::new())
    }

    fn version(&self) -> Result<String> {
        Ok("MockAligner 1.0.0".to_string())
    }

    fn is_available(&self) -> bool {
        true
    }
}
