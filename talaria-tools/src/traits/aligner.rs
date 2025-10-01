use anyhow::Result;
use std::path::Path;
/// Traits for alignment tools
use talaria_bio::sequence::Sequence;

/// Summary of an alignment search result
#[derive(Debug, Clone)]
pub struct AlignmentSummary {
    pub query_id: String,
    pub reference_id: String,
    pub identity: f32,
    pub alignment_length: usize,
    pub mismatches: usize,
    pub gap_opens: usize,
    pub query_start: usize,
    pub query_end: usize,
    pub ref_start: usize,
    pub ref_end: usize,
    pub e_value: f64,
    pub bit_score: f32,
}

/// Configuration for alignment tools
#[derive(Debug, Clone)]
pub struct AlignmentConfig {
    pub max_results: Option<usize>,
    pub min_identity: Option<f32>,
    pub max_evalue: Option<f64>,
    pub threads: Option<usize>,
}

/// Trait for alignment tools
pub trait Aligner: Send + Sync {
    /// Perform alignment search
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentSummary>>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use talaria_bio::sequence::Sequence;

    // ===== Mock Aligner Implementation =====

    struct MockAligner {
        available: bool,
        version: String,
        results: Vec<AlignmentSummary>,
        batch_size: usize,
        supports_prot: bool,
        supports_nucl: bool,
        search_called: Arc<Mutex<bool>>,
    }

    impl MockAligner {
        fn new() -> Self {
            Self {
                available: true,
                version: "1.0.0".to_string(),
                results: vec![],
                batch_size: 1000,
                supports_prot: true,
                supports_nucl: true,
                search_called: Arc::new(Mutex::new(false)),
            }
        }

        fn with_results(mut self, results: Vec<AlignmentSummary>) -> Self {
            self.results = results;
            self
        }
    }

    impl Aligner for MockAligner {
        fn search(
            &mut self,
            _query: &[Sequence],
            _reference: &[Sequence],
        ) -> Result<Vec<AlignmentSummary>> {
            *self.search_called.lock().unwrap() = true;
            Ok(self.results.clone())
        }

        fn version(&self) -> Result<String> {
            Ok(self.version.clone())
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn recommended_batch_size(&self) -> usize {
            self.batch_size
        }

        fn supports_protein(&self) -> bool {
            self.supports_prot
        }

        fn supports_nucleotide(&self) -> bool {
            self.supports_nucl
        }
    }

    // ===== Mock Configurable Aligner =====

    struct MockConfigurableAligner {
        aligner: MockAligner,
        config: AlignmentConfig,
        output_path: Option<PathBuf>,
        temp_dir: Option<PathBuf>,
    }

    impl MockConfigurableAligner {
        fn new() -> Self {
            Self {
                aligner: MockAligner::new(),
                config: AlignmentConfig {
                    max_results: None,
                    min_identity: None,
                    max_evalue: None,
                    threads: None,
                },
                output_path: None,
                temp_dir: None,
            }
        }
    }

    impl Aligner for MockConfigurableAligner {
        fn search(
            &mut self,
            query: &[Sequence],
            reference: &[Sequence],
        ) -> Result<Vec<AlignmentSummary>> {
            self.aligner.search(query, reference)
        }

        fn version(&self) -> Result<String> {
            self.aligner.version()
        }

        fn is_available(&self) -> bool {
            self.aligner.is_available()
        }
    }

    impl ConfigurableAligner for MockConfigurableAligner {
        fn set_config(&mut self, config: AlignmentConfig) {
            self.config = config;
        }

        fn get_config(&self) -> &AlignmentConfig {
            &self.config
        }

        fn set_output_path(&mut self, path: &Path) {
            self.output_path = Some(path.to_path_buf());
        }

        fn set_temp_dir(&mut self, path: &Path) {
            self.temp_dir = Some(path.to_path_buf());
        }
    }

    // ===== AlignmentSummary Tests =====

    #[test]
    fn test_alignment_summary_creation() {
        let summary = AlignmentSummary {
            query_id: "query1".to_string(),
            reference_id: "ref1".to_string(),
            identity: 95.5,
            alignment_length: 100,
            mismatches: 5,
            gap_opens: 2,
            query_start: 10,
            query_end: 110,
            ref_start: 20,
            ref_end: 120,
            e_value: 1e-10,
            bit_score: 150.5,
        };

        assert_eq!(summary.query_id, "query1");
        assert_eq!(summary.reference_id, "ref1");
        assert_eq!(summary.identity, 95.5);
        assert_eq!(summary.alignment_length, 100);
        assert_eq!(summary.e_value, 1e-10);
    }

    #[test]
    fn test_alignment_summary_clone() {
        let summary = AlignmentSummary {
            query_id: "query1".to_string(),
            reference_id: "ref1".to_string(),
            identity: 95.5,
            alignment_length: 100,
            mismatches: 5,
            gap_opens: 2,
            query_start: 10,
            query_end: 110,
            ref_start: 20,
            ref_end: 120,
            e_value: 1e-10,
            bit_score: 150.5,
        };

        let cloned = summary.clone();
        assert_eq!(cloned.query_id, summary.query_id);
        assert_eq!(cloned.identity, summary.identity);
    }

    #[test]
    fn test_alignment_summary_debug() {
        let summary = AlignmentSummary {
            query_id: "query1".to_string(),
            reference_id: "ref1".to_string(),
            identity: 95.5,
            alignment_length: 100,
            mismatches: 5,
            gap_opens: 2,
            query_start: 10,
            query_end: 110,
            ref_start: 20,
            ref_end: 120,
            e_value: 1e-10,
            bit_score: 150.5,
        };

        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("query1"));
        assert!(debug_str.contains("ref1"));
        assert!(debug_str.contains("95.5"));
    }

    // ===== AlignmentConfig Tests =====

    #[test]
    fn test_alignment_config_creation() {
        let config = AlignmentConfig {
            max_results: Some(100),
            min_identity: Some(90.0),
            max_evalue: Some(1e-5),
            threads: Some(8),
        };

        assert_eq!(config.max_results, Some(100));
        assert_eq!(config.min_identity, Some(90.0));
        assert_eq!(config.max_evalue, Some(1e-5));
        assert_eq!(config.threads, Some(8));
    }

    #[test]
    fn test_alignment_config_with_none_values() {
        let config = AlignmentConfig {
            max_results: None,
            min_identity: None,
            max_evalue: None,
            threads: None,
        };

        assert!(config.max_results.is_none());
        assert!(config.min_identity.is_none());
        assert!(config.max_evalue.is_none());
        assert!(config.threads.is_none());
    }

    #[test]
    fn test_alignment_config_clone() {
        let config = AlignmentConfig {
            max_results: Some(50),
            min_identity: Some(85.0),
            max_evalue: Some(0.001),
            threads: Some(4),
        };

        let cloned = config.clone();
        assert_eq!(cloned.max_results, config.max_results);
        assert_eq!(cloned.min_identity, config.min_identity);
        assert_eq!(cloned.max_evalue, config.max_evalue);
        assert_eq!(cloned.threads, config.threads);
    }

    // ===== Aligner Trait Tests =====

    #[test]
    fn test_mock_aligner_basic_functionality() {
        let aligner = MockAligner::new();

        assert!(aligner.is_available());
        assert_eq!(aligner.version().unwrap(), "1.0.0");
        assert_eq!(aligner.recommended_batch_size(), 1000);
        assert!(aligner.supports_protein());
        assert!(aligner.supports_nucleotide());
    }

    #[test]
    fn test_mock_aligner_search() {
        let results = vec![AlignmentSummary {
            query_id: "q1".to_string(),
            reference_id: "r1".to_string(),
            identity: 98.0,
            alignment_length: 100,
            mismatches: 2,
            gap_opens: 0,
            query_start: 1,
            query_end: 100,
            ref_start: 1,
            ref_end: 100,
            e_value: 1e-50,
            bit_score: 200.0,
        }];

        let mut aligner = MockAligner::new().with_results(results.clone());

        let query = vec![Sequence::new("q1".to_string(), b"ATCG".to_vec())];
        let reference = vec![Sequence::new("r1".to_string(), b"ATCG".to_vec())];

        let search_results = aligner.search(&query, &reference).unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].query_id, "q1");
        assert_eq!(search_results[0].identity, 98.0);
    }

    #[test]
    fn test_mock_aligner_unavailable() {
        let mut aligner = MockAligner::new();
        aligner.available = false;

        assert!(!aligner.is_available());
    }

    #[test]
    fn test_mock_aligner_custom_batch_size() {
        let mut aligner = MockAligner::new();
        aligner.batch_size = 5000;

        assert_eq!(aligner.recommended_batch_size(), 5000);
    }

    #[test]
    fn test_mock_aligner_sequence_support() {
        let mut aligner = MockAligner::new();

        // Test protein only
        aligner.supports_prot = true;
        aligner.supports_nucl = false;
        assert!(aligner.supports_protein());
        assert!(!aligner.supports_nucleotide());

        // Test nucleotide only
        aligner.supports_prot = false;
        aligner.supports_nucl = true;
        assert!(!aligner.supports_protein());
        assert!(aligner.supports_nucleotide());
    }

    #[test]
    fn test_aligner_search_called() {
        let mut aligner = MockAligner::new();
        let search_called = aligner.search_called.clone();

        assert!(!*search_called.lock().unwrap());

        let query = vec![];
        let reference = vec![];
        let _ = aligner.search(&query, &reference);

        assert!(*search_called.lock().unwrap());
    }

    // ===== ConfigurableAligner Trait Tests =====

    #[test]
    fn test_configurable_aligner_config() {
        let mut aligner = MockConfigurableAligner::new();

        let config = AlignmentConfig {
            max_results: Some(200),
            min_identity: Some(75.0),
            max_evalue: Some(0.01),
            threads: Some(16),
        };

        aligner.set_config(config.clone());
        let stored_config = aligner.get_config();

        assert_eq!(stored_config.max_results, Some(200));
        assert_eq!(stored_config.min_identity, Some(75.0));
        assert_eq!(stored_config.max_evalue, Some(0.01));
        assert_eq!(stored_config.threads, Some(16));
    }

    #[test]
    fn test_configurable_aligner_paths() {
        let mut aligner = MockConfigurableAligner::new();

        let output_path = PathBuf::from("/tmp/output.txt");
        let temp_dir = PathBuf::from("/tmp/work");

        aligner.set_output_path(&output_path);
        aligner.set_temp_dir(&temp_dir);

        assert_eq!(aligner.output_path, Some(output_path));
        assert_eq!(aligner.temp_dir, Some(temp_dir));
    }

    #[test]
    fn test_configurable_aligner_inherits_aligner_methods() {
        let aligner = MockConfigurableAligner::new();

        assert!(aligner.is_available());
        assert_eq!(aligner.version().unwrap(), "1.0.0");
    }

    #[test]
    fn test_configurable_aligner_update_config() {
        let mut aligner = MockConfigurableAligner::new();

        // Set initial config
        let config1 = AlignmentConfig {
            max_results: Some(100),
            min_identity: Some(80.0),
            max_evalue: None,
            threads: None,
        };
        aligner.set_config(config1);
        assert_eq!(aligner.get_config().max_results, Some(100));

        // Update config
        let config2 = AlignmentConfig {
            max_results: Some(500),
            min_identity: Some(90.0),
            max_evalue: Some(1e-10),
            threads: Some(32),
        };
        aligner.set_config(config2);
        assert_eq!(aligner.get_config().max_results, Some(500));
        assert_eq!(aligner.get_config().min_identity, Some(90.0));
    }

    // ===== Edge Case Tests =====

    #[test]
    fn test_extreme_alignment_values() {
        let summary = AlignmentSummary {
            query_id: String::new(),
            reference_id: String::new(),
            identity: 0.0,
            alignment_length: 0,
            mismatches: usize::MAX,
            gap_opens: usize::MAX,
            query_start: 0,
            query_end: 0,
            ref_start: 0,
            ref_end: 0,
            e_value: f64::INFINITY,
            bit_score: f32::NEG_INFINITY,
        };

        assert_eq!(summary.identity, 0.0);
        assert_eq!(summary.mismatches, usize::MAX);
        assert!(summary.e_value.is_infinite());
        assert!(summary.bit_score.is_infinite());
    }

    #[test]
    fn test_empty_search_results() {
        let mut aligner = MockAligner::new();

        let query = vec![];
        let reference = vec![];

        let results = aligner.search(&query, &reference).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_multiple_search_results() {
        let results = vec![
            AlignmentSummary {
                query_id: "q1".to_string(),
                reference_id: "r1".to_string(),
                identity: 95.0,
                alignment_length: 100,
                mismatches: 5,
                gap_opens: 0,
                query_start: 1,
                query_end: 100,
                ref_start: 1,
                ref_end: 100,
                e_value: 1e-40,
                bit_score: 180.0,
            },
            AlignmentSummary {
                query_id: "q1".to_string(),
                reference_id: "r2".to_string(),
                identity: 85.0,
                alignment_length: 100,
                mismatches: 15,
                gap_opens: 2,
                query_start: 1,
                query_end: 100,
                ref_start: 5,
                ref_end: 105,
                e_value: 1e-20,
                bit_score: 120.0,
            },
            AlignmentSummary {
                query_id: "q2".to_string(),
                reference_id: "r1".to_string(),
                identity: 99.0,
                alignment_length: 50,
                mismatches: 1,
                gap_opens: 0,
                query_start: 1,
                query_end: 50,
                ref_start: 50,
                ref_end: 100,
                e_value: 1e-30,
                bit_score: 150.0,
            },
        ];

        let mut aligner = MockAligner::new().with_results(results.clone());

        let query = vec![
            Sequence::new("q1".to_string(), b"ATCG".to_vec()),
            Sequence::new("q2".to_string(), b"GCTA".to_vec()),
        ];
        let reference = vec![
            Sequence::new("r1".to_string(), b"ATCG".to_vec()),
            Sequence::new("r2".to_string(), b"ATGG".to_vec()),
        ];

        let search_results = aligner.search(&query, &reference).unwrap();
        assert_eq!(search_results.len(), 3);

        // Verify first result
        assert_eq!(search_results[0].query_id, "q1");
        assert_eq!(search_results[0].reference_id, "r1");
        assert_eq!(search_results[0].identity, 95.0);

        // Verify second result
        assert_eq!(search_results[1].query_id, "q1");
        assert_eq!(search_results[1].reference_id, "r2");
        assert_eq!(search_results[1].identity, 85.0);

        // Verify third result
        assert_eq!(search_results[2].query_id, "q2");
        assert_eq!(search_results[2].reference_id, "r1");
        assert_eq!(search_results[2].identity, 99.0);
    }

    // ===== Property-based Tests =====

    #[quickcheck_macros::quickcheck]
    fn prop_alignment_summary_fields_preserved(
        identity: f32,
        length: usize,
        mismatches: usize,
        e_value: f64,
    ) -> bool {
        let summary = AlignmentSummary {
            query_id: "test".to_string(),
            reference_id: "ref".to_string(),
            identity,
            alignment_length: length,
            mismatches,
            gap_opens: 0,
            query_start: 0,
            query_end: length,
            ref_start: 0,
            ref_end: length,
            e_value,
            bit_score: 0.0,
        };

        let cloned = summary.clone();

        // Handle NaN and infinity properly in floating point comparisons
        let identity_match = if identity.is_nan() {
            cloned.identity.is_nan()
        } else {
            cloned.identity == identity
        };

        let e_value_match = if e_value.is_nan() {
            cloned.e_value.is_nan()
        } else {
            cloned.e_value == e_value
        };

        identity_match
            && cloned.alignment_length == length
            && cloned.mismatches == mismatches
            && e_value_match
    }

    #[quickcheck_macros::quickcheck]
    fn prop_config_fields_preserved(max_results: Option<usize>, threads: Option<usize>) -> bool {
        let config = AlignmentConfig {
            max_results,
            min_identity: None,
            max_evalue: None,
            threads,
        };

        let cloned = config.clone();

        cloned.max_results == max_results && cloned.threads == threads
    }
}
