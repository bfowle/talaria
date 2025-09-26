//! Mock aligner for testing

use std::collections::HashMap;
use anyhow::Result;

/// Configuration for mock aligner behavior
#[derive(Debug, Clone)]
pub struct MockAlignerConfig {
    /// Fixed alignments to return
    pub alignments: HashMap<String, Vec<MockAlignment>>,
    /// Whether to simulate failures
    pub should_fail: bool,
    /// Failure message if should_fail is true
    pub failure_message: String,
}

impl Default for MockAlignerConfig {
    fn default() -> Self {
        Self {
            alignments: HashMap::new(),
            should_fail: false,
            failure_message: "Mock aligner failure".to_string(),
        }
    }
}

/// Mock alignment result
#[derive(Debug, Clone)]
pub struct MockAlignment {
    pub query_id: String,
    pub reference_id: String,
    pub score: f64,
    pub identity: f64,
}

/// Mock aligner for testing
pub struct MockAligner {
    config: MockAlignerConfig,
    call_count: usize,
}

impl MockAligner {
    /// Create a new mock aligner
    pub fn new() -> Self {
        Self {
            config: MockAlignerConfig::default(),
            call_count: 0,
        }
    }

    /// Create with custom config
    pub fn with_config(config: MockAlignerConfig) -> Self {
        Self {
            config,
            call_count: 0,
        }
    }

    /// Configure to return specific alignments
    pub fn with_alignments(mut self, query: &str, alignments: Vec<MockAlignment>) -> Self {
        self.config.alignments.insert(query.to_string(), alignments);
        self
    }

    /// Configure to fail
    pub fn with_failure(mut self, message: &str) -> Self {
        self.config.should_fail = true;
        self.config.failure_message = message.to_string();
        self
    }

    /// Perform mock alignment
    pub fn align(&mut self, query_id: &str) -> Result<Vec<MockAlignment>> {
        self.call_count += 1;

        if self.config.should_fail {
            anyhow::bail!("{}", self.config.failure_message);
        }

        Ok(self.config.alignments
            .get(query_id)
            .cloned()
            .unwrap_or_else(|| vec![
                // Default alignment if not configured
                MockAlignment {
                    query_id: query_id.to_string(),
                    reference_id: "ref_1".to_string(),
                    score: 100.0,
                    identity: 0.95,
                }
            ]))
    }

    /// Get number of times align was called
    pub fn call_count(&self) -> usize {
        self.call_count
    }

    /// Reset call count
    pub fn reset(&mut self) {
        self.call_count = 0;
    }
}

impl Default for MockAligner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_aligner() {
        let mut aligner = MockAligner::new()
            .with_alignments("seq1", vec![
                MockAlignment {
                    query_id: "seq1".to_string(),
                    reference_id: "ref_a".to_string(),
                    score: 150.0,
                    identity: 0.98,
                }
            ]);

        let results = aligner.align("seq1").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].reference_id, "ref_a");
        assert_eq!(aligner.call_count(), 1);
    }

    #[test]
    fn test_mock_aligner_failure() {
        let mut aligner = MockAligner::new()
            .with_failure("Test failure");

        let result = aligner.align("seq1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Test failure"));
    }
}