/// Integration tests for trait implementations
///
/// These tests verify that all trait implementations work correctly
/// and can be used polymorphically.

use talaria::tools::Aligner;
use talaria::storage::ChunkStorage;
use talaria::casg::{Chunker, TaxonomicChunker, ChunkingStrategy};
use talaria::bio::sequence::Sequence;
use talaria::casg::types::{SHA256Hash, TaxonId, SpecialTaxon, ChunkStrategy};
use std::collections::HashMap;

#[cfg(test)]
mod aligner_tests {
    use super::*;

    /// Mock aligner for testing trait functionality
    struct MockAligner {
        name: String,
        supports_taxonomy: bool,
    }

    impl Aligner for MockAligner {
        fn search(
            &mut self,
            query: &[Sequence],
            reference: &[Sequence],
        ) -> anyhow::Result<Vec<talaria::tools::AlignmentResult>> {
            // Return mock results - each query against each reference
            let mut results = Vec::new();
            for q in query {
                for r in reference {
                    results.push(talaria::tools::AlignmentResult {
                        query_id: q.id.clone(),
                        subject_id: r.id.clone(),
                        identity: 0.95,
                        alignment_length: 100,
                        mismatches: 5,
                        gap_opens: 0,
                        query_start: 0,
                        query_end: 100,
                        subject_start: 0,
                        subject_end: 100,
                        evalue: 1e-10,
                        bit_score: 200.0,
                        taxon_id: None,
                    });
                }
            }
            Ok(results)
        }

        fn search_batched(
            &mut self,
            query: &[Sequence],
            reference: &[Sequence],
            batch_size: usize,
        ) -> anyhow::Result<Vec<talaria::tools::AlignmentResult>> {
            // Simulate batched processing
            let mut results = Vec::new();
            for chunk in query.chunks(batch_size) {
                results.extend(self.search(chunk, reference)?);
            }
            Ok(results)
        }

        fn build_index(
            &mut self,
            _reference_path: &std::path::Path,
            _index_path: &std::path::Path,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn verify_installation(&self) -> anyhow::Result<()> {
            Ok(())
        }

        fn supports_taxonomy(&self) -> bool {
            self.supports_taxonomy
        }

        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_aligner_trait_polymorphism() {
        // Test that different aligners can be used through the trait
        let aligners: Vec<Box<dyn Aligner>> = vec![
            Box::new(MockAligner {
                name: "Mock1".to_string(),
                supports_taxonomy: true,
            }),
            Box::new(MockAligner {
                name: "Mock2".to_string(),
                supports_taxonomy: false,
            }),
        ];

        for aligner in &aligners {
            assert!(aligner.verify_installation().is_ok());
            assert!(!aligner.name().is_empty());
        }
    }

    #[test]
    fn test_aligner_search() {
        let mut aligner = MockAligner {
            name: "TestAligner".to_string(),
            supports_taxonomy: true,
        };

        let query = vec![
            Sequence::new("seq1".to_string(), b"ACGT".to_vec()),
            Sequence::new("seq2".to_string(), b"TTGG".to_vec()),
        ];

        let reference = vec![
            Sequence::new("ref1".to_string(), b"ACGT".to_vec()),
            Sequence::new("ref2".to_string(), b"TTGG".to_vec()),
        ];

        let results = aligner.search(&query, &reference).unwrap();
        assert_eq!(results.len(), 4); // 2 queries x 2 references
        assert_eq!(results[0].query_id, "seq1");
        assert_eq!(results[0].subject_id, "ref1");
    }

    #[test]
    fn test_aligner_batched_search() {
        let mut aligner = MockAligner {
            name: "BatchAligner".to_string(),
            supports_taxonomy: false,
        };

        let query: Vec<Sequence> = (0..10)
            .map(|i| Sequence::new(format!("seq{}", i), b"ACGT".to_vec()))
            .collect();

        let reference = vec![Sequence::new("ref".to_string(), b"ACGT".to_vec())];

        let results = aligner.search_batched(&query, &reference, 3).unwrap();
        assert_eq!(results.len(), 10);
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    /// Mock storage implementation for testing
    struct MockStorage {
        chunks: HashMap<SHA256Hash, Vec<u8>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                chunks: HashMap::new(),
            }
        }
    }

    impl ChunkStorage for MockStorage {
        fn store_chunk(&self, data: &[u8], _compress: bool) -> anyhow::Result<SHA256Hash> {
            let hash = SHA256Hash::compute(data);
            Ok(hash)
        }

        fn get_chunk(&self, hash: &SHA256Hash) -> anyhow::Result<Vec<u8>> {
            self.chunks.get(hash)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Chunk not found"))
        }

        fn has_chunk(&self, hash: &SHA256Hash) -> bool {
            self.chunks.contains_key(hash)
        }

        fn enumerate_chunks(&self) -> Vec<talaria::casg::types::ChunkInfo> {
            Vec::new()
        }

        fn verify_all(&self) -> anyhow::Result<Vec<talaria::storage::VerificationError>> {
            Ok(Vec::new())
        }

        fn get_stats(&self) -> talaria::storage::StorageStats {
            talaria::storage::StorageStats {
                total_chunks: self.chunks.len(),
                total_size: self.chunks.values().map(|v| v.len()).sum(),
                compressed_chunks: 0,
                deduplication_ratio: 1.0,
            }
        }

        fn gc(&mut self, _referenced: &[SHA256Hash]) -> anyhow::Result<talaria::storage::GCResult> {
            Ok(talaria::storage::GCResult {
                removed_count: 0,
                freed_space: 0,
            })
        }
    }

    #[test]
    fn test_storage_trait_operations() {
        let storage = MockStorage::new();
        
        // Test basic operations through trait
        let data = b"test data";
        let hash = storage.store_chunk(data, false).unwrap();
        assert_eq!(hash, SHA256Hash::compute(data));
        
        // Test stats
        let stats = storage.get_stats();
        assert_eq!(stats.total_chunks, 0); // Mock doesn't actually store
    }
}

#[cfg(test)]
mod chunker_tests {
    use super::*;

    #[test]
    fn test_chunker_trait() {
        let strategy = ChunkingStrategy {
            target_chunk_size: 1024 * 1024,
            max_chunk_size: 10 * 1024 * 1024,
            min_sequences_per_chunk: 1,
            taxonomic_coherence: 0.8,
            special_taxa: vec![
                SpecialTaxon {
                    taxon_id: TaxonId(562),
                    name: "E. coli".to_string(),
                    strategy: ChunkStrategy::OwnChunks,
                },
            ],
        };

        let chunker = TaxonomicChunker::new(strategy.clone());
        
        // Test through trait
        let chunker_trait: &dyn Chunker = &chunker;
        assert_eq!(chunker_trait.name(), "TaxonomicChunker");
        assert_eq!(chunker_trait.strategy().target_chunk_size, 1024 * 1024);
        
        // Test should_add_to_chunk
        let chunk_taxa = vec![TaxonId(562)];
        assert!(chunker_trait.should_add_to_chunk(
            500_000,
            100_000,
            &chunk_taxa,
            Some(TaxonId(562))
        ));
        
        // Should reject if it would exceed max size
        assert!(!chunker_trait.should_add_to_chunk(
            9_000_000,
            2_000_000,
            &chunk_taxa,
            Some(TaxonId(562))
        ));
    }

    #[test]
    fn test_taxonomy_aware_chunker() {
        use talaria::casg::chunker::TaxonomyAwareChunker;
        
        let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());
        
        // Load taxonomy mapping
        let mut mapping = HashMap::new();
        mapping.insert("seq1".to_string(), TaxonId(562));
        mapping.insert("seq2".to_string(), TaxonId(9606));
        
        chunker.load_taxonomy_mapping(mapping);
        
        // Test taxon ID retrieval
        let seq = Sequence::new("seq1".to_string(), vec![]);
        let taxon_id = chunker.get_taxon_id(&seq).unwrap();
        assert_eq!(taxon_id.0, 562);
    }
}

#[cfg(test)]
mod delta_generator_tests {
    use super::*;
    use talaria::casg::delta::DeltaGenerator as DeltaGeneratorTrait;
    use talaria::casg::delta_generator::DeltaGenerator;

    #[test]
    fn test_delta_generator_trait() {
        let config = talaria::casg::delta::DeltaGeneratorConfig::default();
        let generator = DeltaGenerator::new(config.clone());
        
        // Test through trait
        let generator_trait: &dyn DeltaGeneratorTrait = &generator;
        assert_eq!(generator_trait.name(), "DeltaGenerator");
        assert_eq!(generator_trait.config().max_chunk_size, 16 * 1024 * 1024);
        
        // Test similarity calculation
        let seq1 = Sequence::new("seq1".to_string(), b"ACGTACGT".to_vec());
        let seq2 = Sequence::new("seq2".to_string(), b"ACGTACGT".to_vec());
        let similarity = generator_trait.calculate_similarity(&seq1, &seq2);
        assert_eq!(similarity, 1.0);
        
        // Test should_use_delta
        assert!(generator_trait.should_use_delta(&seq1, &seq2, 1.0));
        
        // Should reject if similarity is too low
        assert!(!generator_trait.should_use_delta(&seq1, &seq2, 0.5));
    }
}

#[cfg(test)]
mod selector_tests {
    use super::*;
    use talaria::core::selection::{ReferenceSelector, SelectionResult};

    /// Mock selector for testing
    struct MockSelector {
        name: String,
        ratio: f64,
    }

    impl ReferenceSelector for MockSelector {
        fn select_references(
            &self,
            sequences: Vec<Sequence>,
            target_ratio: f64,
        ) -> anyhow::Result<SelectionResult> {
            let target_count = (sequences.len() as f64 * target_ratio) as usize;
            let references = sequences.into_iter().take(target_count).collect();
            
            Ok(SelectionResult {
                references,
                children: HashMap::new(),
                discarded: std::collections::HashSet::new(),
                stats: talaria::core::selection::SelectionStats {
                    total_sequences: 100,
                    selected_references: target_count,
                    assigned_children: 0,
                    discarded_sequences: 0,
                    coverage_ratio: target_ratio,
                    avg_children_per_reference: 0.0,
                    selection_time_ms: 10,
                },
            })
        }

        fn calculate_coverage(
            &self,
            references: &[Sequence],
            all_sequences: &[Sequence],
        ) -> f64 {
            references.len() as f64 / all_sequences.len() as f64
        }

        fn strategy_name(&self) -> &str {
            &self.name
        }

        fn estimate_memory_usage(&self, num_sequences: usize) -> usize {
            num_sequences * 1000 // Rough estimate
        }

        fn recommend_parameters(
            &self,
            _num_sequences: usize,
            _avg_sequence_length: usize,
        ) -> talaria::core::selection::RecommendedParams {
            talaria::core::selection::RecommendedParams {
                target_ratio: self.ratio,
                min_length: 50,
                similarity_threshold: 0.9,
                use_taxonomy: false,
                batch_size: Some(1000),
            }
        }
    }

    #[test]
    fn test_selector_trait() {
        let selector = MockSelector {
            name: "MockSelector".to_string(),
            ratio: 0.3,
        };

        let sequences: Vec<Sequence> = (0..100)
            .map(|i| Sequence::new(format!("seq{}", i), vec![]))
            .collect();

        let result = selector.select_references(sequences.clone(), 0.3).unwrap();
        assert_eq!(result.references.len(), 30);
        
        // Test coverage calculation
        let coverage = selector.calculate_coverage(&result.references, &sequences);
        assert_eq!(coverage, 0.3);
        
        // Test memory estimation
        let mem = selector.estimate_memory_usage(1000);
        assert_eq!(mem, 1_000_000);
    }
}

#[cfg(test)]
mod validator_tests {
    use super::*;
    use talaria::core::validation::{
        Validator, SequenceValidator, ValidationResult,
        SequenceValidation, SequenceIssue,
    };

    struct MockValidator;

    impl Validator for MockValidator {
        fn validate(&self, _target: &std::path::Path) -> anyhow::Result<ValidationResult> {
            Ok(ValidationResult {
                valid: true,
                errors: Vec::new(),
                warnings: Vec::new(),
                info: vec!["Validation complete".to_string()],
                stats: talaria::core::validation::ValidationStats {
                    total_items: 100,
                    valid_items: 95,
                    invalid_items: 5,
                    repaired_items: 0,
                    processing_time_ms: 50,
                },
            })
        }

        fn name(&self) -> &str {
            "MockValidator"
        }

        fn can_validate(&self, path: &std::path::Path) -> bool {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "fasta" || e == "fa")
                .unwrap_or(false)
        }

        fn rules(&self) -> &[talaria::core::validation::ValidationRule] {
            &[]
        }

        fn set_strictness(&mut self, _level: talaria::core::validation::StrictnessLevel) {}
    }

    impl SequenceValidator for MockValidator {
        fn validate_sequence(&self, sequence: &Sequence) -> anyhow::Result<SequenceValidation> {
            let mut issues = Vec::new();
            
            if sequence.sequence.len() < 10 {
                issues.push(SequenceIssue::TooShort {
                    length: sequence.sequence.len(),
                    min_length: 10,
                });
            }
            
            Ok(SequenceValidation {
                id: sequence.id.clone(),
                valid: issues.is_empty(),
                issues,
            })
        }

        fn validate_sequences(&self, sequences: &[Sequence]) -> anyhow::Result<Vec<SequenceValidation>> {
            sequences.iter()
                .map(|s| self.validate_sequence(s))
                .collect()
        }

        fn check_integrity(&self, sequence: &Sequence) -> bool {
            !sequence.sequence.is_empty()
        }

        fn repair_sequence(&self, _sequence: &mut Sequence) -> anyhow::Result<talaria::core::validation::RepairResult> {
            Ok(talaria::core::validation::RepairResult {
                repaired: false,
                changes: Vec::new(),
            })
        }

        fn allowed_characters(&self) -> &[u8] {
            b"ACGTUWSMKRYBDHVN"
        }
    }

    #[test]
    fn test_validator_trait() {
        let validator = MockValidator;
        
        // Test path validation
        let fasta_path = std::path::Path::new("test.fasta");
        assert!(validator.can_validate(fasta_path));
        
        let other_path = std::path::Path::new("test.txt");
        assert!(!validator.can_validate(other_path));
        
        // Test sequence validation
        let short_seq = Sequence::new("short".to_string(), b"ACGT".to_vec());
        let validation = validator.validate_sequence(&short_seq).unwrap();
        assert!(!validation.valid);
        assert_eq!(validation.issues.len(), 1);
        
        let long_seq = Sequence::new("long".to_string(), b"ACGTACGTACGT".to_vec());
        let validation = validator.validate_sequence(&long_seq).unwrap();
        assert!(validation.valid);
    }
}

#[cfg(test)]
mod processor_tests {
    use super::*;
    use talaria::processing::{
        SequenceProcessor, BatchProcessor, ProcessingResult,
        ProcessorConfig, SequenceType,
    };

    struct MockProcessor {
        name: String,
    }

    impl SequenceProcessor for MockProcessor {
        fn process(&self, sequences: &mut [Sequence]) -> anyhow::Result<ProcessingResult> {
            // Simulate processing by adding prefix to IDs
            for seq in sequences.iter_mut() {
                seq.id = format!("processed_{}", seq.id);
            }
            
            Ok(ProcessingResult {
                processed: sequences.len(),
                failed: 0,
                skipped: 0,
                processing_time_ms: 10,
                errors: Vec::new(),
            })
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn supports_type(&self, _seq_type: SequenceType) -> bool {
            true
        }

        fn config(&self) -> ProcessorConfig {
            ProcessorConfig::default()
        }

        fn estimate_time(&self, num_sequences: usize) -> std::time::Duration {
            std::time::Duration::from_millis(num_sequences as u64)
        }
    }

    impl BatchProcessor for MockProcessor {
        fn process_batch(
            &self,
            sequences: &mut [Sequence],
            batch_size: usize,
        ) -> anyhow::Result<ProcessingResult> {
            let mut total_processed = 0;
            
            for chunk in sequences.chunks_mut(batch_size) {
                self.process(chunk)?;
                total_processed += chunk.len();
            }
            
            Ok(ProcessingResult {
                processed: total_processed,
                failed: 0,
                skipped: 0,
                processing_time_ms: 20,
                errors: Vec::new(),
            })
        }

        fn optimal_batch_size(&self) -> usize {
            100
        }

        fn process_with_progress<F>(
            &self,
            sequences: &mut [Sequence],
            batch_size: usize,
            progress_fn: F,
        ) -> anyhow::Result<ProcessingResult>
        where
            F: Fn(usize, usize) + Send + Sync,
        {
            let total = sequences.len();
            let mut processed = 0;
            
            for chunk in sequences.chunks_mut(batch_size) {
                self.process(chunk)?;
                processed += chunk.len();
                progress_fn(processed, total);
            }
            
            Ok(ProcessingResult {
                processed,
                failed: 0,
                skipped: 0,
                processing_time_ms: 30,
                errors: Vec::new(),
            })
        }
    }

    #[test]
    fn test_processor_trait() {
        let processor = MockProcessor {
            name: "TestProcessor".to_string(),
        };
        
        let mut sequences = vec![
            Sequence::new("seq1".to_string(), vec![]),
            Sequence::new("seq2".to_string(), vec![]),
        ];
        
        let result = processor.process(&mut sequences).unwrap();
        assert_eq!(result.processed, 2);
        assert_eq!(sequences[0].id, "processed_seq1");
        
        // Test batch processing
        let mut batch_sequences: Vec<Sequence> = (0..10)
            .map(|i| Sequence::new(format!("seq{}", i), vec![]))
            .collect();
        
        let batch_result = processor.process_batch(&mut batch_sequences, 3).unwrap();
        assert_eq!(batch_result.processed, 10);
        
        // Test with progress callback
        let mut progress_sequences: Vec<Sequence> = (0..5)
            .map(|i| Sequence::new(format!("seq{}", i), vec![]))
            .collect();

        use std::sync::atomic::{AtomicUsize, Ordering};
        let progress_calls = AtomicUsize::new(0);
        let progress_result = processor.process_with_progress(
            &mut progress_sequences,
            2,
            |current, total| {
                assert!(current <= total);
                progress_calls.fetch_add(1, Ordering::SeqCst);
            },
        ).unwrap();

        assert_eq!(progress_result.processed, 5);
        assert!(progress_calls.load(Ordering::SeqCst) > 0);
    }
}