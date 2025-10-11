use std::collections::HashMap;
use talaria_bio::sequence::Sequence;
use talaria_sequoia::types::{ChunkStrategy, SHA256Hash, SpecialTaxon, TaxonId};
use talaria_sequoia::{ChunkingStrategy, TaxonomicChunker};
use talaria_sequoia::storage::ChunkStorage;
/// Integration tests for trait implementations
///
/// These tests verify that all trait implementations work correctly
/// and can be used polymorphically.
use talaria_tools::traits::{Aligner, AlignmentResult as ToolAlignmentResult};

#[cfg(test)]
mod aligner_tests {
    use super::*;

    /// Mock aligner for testing trait functionality
    struct MockAligner {
        _name: String,
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
                    results.push(ToolAlignmentResult {
                        query_id: q.id.clone(),
                        reference_id: r.id.clone(),
                        identity: 0.95,
                        alignment_length: 100,
                        mismatches: 5,
                        gap_opens: 0,
                        query_start: 0,
                        query_end: 100,
                        ref_start: 0,
                        ref_end: 100,
                        e_value: 1e-10,
                        bit_score: 200.0,
                    });
                }
            }
            Ok(results)
        }

        fn version(&self) -> anyhow::Result<String> {
            Ok("1.0.0-mock".to_string())
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_aligner_trait_polymorphism() {
        // Test that different aligners can be used through the trait
        let aligners: Vec<Box<dyn Aligner>> = vec![
            Box::new(MockAligner {
                _name: "Mock1".to_string(),
            }),
            Box::new(MockAligner {
                _name: "Mock2".to_string(),
            }),
        ];

        for aligner in &aligners {
            assert!(aligner.is_available());
            // name() method doesn't exist in the trait
            assert!(aligner.version().is_ok());
        }
    }

    #[test]
    fn test_aligner_search() {
        let mut aligner = MockAligner {
            _name: "TestAligner".to_string(),
        };

        let query = vec![
            Sequence {
                id: "seq1".to_string(),
                description: None,
                sequence: b"ACGT".to_vec(),
                taxon_id: None,
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "seq2".to_string(),
                description: None,
                sequence: b"TTGG".to_vec(),
                taxon_id: None,
                taxonomy_sources: Default::default(),
            },
        ];

        let reference = vec![
            Sequence {
                id: "ref1".to_string(),
                description: None,
                sequence: b"ACGT".to_vec(),
                taxon_id: None,
                taxonomy_sources: Default::default(),
            },
            Sequence {
                id: "ref2".to_string(),
                description: None,
                sequence: b"TTGG".to_vec(),
                taxon_id: None,
                taxonomy_sources: Default::default(),
            },
        ];

        let results = aligner.search(&query, &reference).unwrap();
        assert_eq!(results.len(), 4); // 2 queries x 2 references
        assert_eq!(results[0].query_id, "seq1");
        assert_eq!(results[0].reference_id, "ref1");
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
            self.chunks
                .get(hash)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Chunk not found"))
        }

        fn has_chunk(&self, hash: &SHA256Hash) -> bool {
            self.chunks.contains_key(hash)
        }

        fn enumerate_chunks(&self) -> Vec<talaria::sequoia::types::ChunkInfo> {
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
        use talaria_sequoia::Chunker;
        let strategy = ChunkingStrategy {
            target_chunk_size: 1024 * 1024,
            max_chunk_size: 10 * 1024 * 1024,
            min_sequences_per_chunk: 1,
            taxonomic_coherence: 0.8,
            special_taxa: vec![SpecialTaxon {
                taxon_id: TaxonId(562),
                name: "E. coli".to_string(),
                strategy: ChunkStrategy::OwnChunks,
            }],
        };

        let mut chunker = TaxonomicChunker::new(strategy.clone());

        // Test through trait
        let sequences = vec![Sequence {
            id: "seq1".to_string(),
            description: Some("test".to_string()),
            sequence: b"ACGT".to_vec(),
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        }];

        let chunks = chunker.chunk_sequences(&sequences).unwrap();
        assert!(!chunks.is_empty());

        // Test stats
        let stats = chunker.get_stats();
        assert_eq!(stats.total_chunks, chunks.len());

        // Test set_chunk_size
        chunker.set_chunk_size(1000, 10_000_000);
    }

    #[test]
    fn test_taxonomy_aware_chunker() {
        use talaria_sequoia::Chunker;
        let mut chunker = TaxonomicChunker::new(ChunkingStrategy::default());

        // Load taxonomy mapping
        let mut mapping = HashMap::new();
        mapping.insert("seq1".to_string(), TaxonId(562));
        mapping.insert("seq2".to_string(), TaxonId(9606));

        chunker.load_taxonomy_mapping(mapping.clone());

        // Test that sequences get chunked with the loaded taxonomy
        let sequences = vec![Sequence {
            id: "seq1".to_string(),
            description: Some("test".to_string()),
            sequence: b"ACGT".to_vec(),
            taxon_id: None, // Will use mapping
            taxonomy_sources: Default::default(),
        }];

        // Test chunking with loaded taxonomy mapping
        let chunks = chunker.chunk_sequences(&sequences).unwrap();
        assert!(!chunks.is_empty());

        // The chunker should have used the taxonomy mapping
        assert!(chunks[0].taxon_ids.contains(&TaxonId(562)));
    }
}

#[cfg(test)]
mod delta_generator_tests {
    use super::*;
    use talaria_sequoia::delta::DeltaGenerator as DeltaGeneratorTrait;
    use talaria_sequoia::delta_generator::DeltaGenerator;

    #[test]
    fn test_delta_generator_trait() {
        let config = talaria::sequoia::delta::DeltaGeneratorConfig::default();
        let mut generator = DeltaGenerator::new(config.clone());

        // Test through trait
        let generator_trait: &mut dyn DeltaGeneratorTrait = &mut generator;

        // Test get_config
        let trait_config = generator_trait.get_config();
        assert_eq!(trait_config.max_chunk_size, config.max_chunk_size);

        // Test generating deltas
        let sequences = vec![Sequence {
            id: "seq1".to_string(),
            description: Some("test".to_string()),
            sequence: b"ACGTACGT".to_vec(),
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        }];

        let references = vec![Sequence {
            id: "ref1".to_string(),
            description: Some("reference".to_string()),
            sequence: b"ACGTACGA".to_vec(), // Similar to seq1
            taxon_id: Some(562),
            taxonomy_sources: Default::default(),
        }];

        let ref_hash = SHA256Hash::compute(b"test_reference");
        let deltas = generator_trait
            .generate_deltas(&sequences, &references, ref_hash)
            .unwrap();
        assert!(!deltas.is_empty());
    }
}

#[cfg(test)]
mod selector_tests {
    use super::*;
    use talaria_core::selection::ReferenceSelector;

    /// Mock selector for testing
    struct MockSelector {
        _name: String,
        _ratio: f64,
        total_sequences: usize,
        selected_references: usize,
    }

    impl ReferenceSelector for MockSelector {
        fn select_references(
            &self,
            sequences: Vec<Sequence>,
            target_ratio: f64,
        ) -> anyhow::Result<talaria::core::selection::TraitSelectionResult> {
            let target_count = (sequences.len() as f64 * target_ratio) as usize;
            let total = sequences.len();
            let references = sequences.into_iter().take(target_count).collect();

            Ok(talaria::core::selection::TraitSelectionResult {
                references,
                stats: talaria::core::selection::SelectionStats {
                    total_sequences: total,
                    references_selected: target_count,
                    coverage: target_ratio,
                    avg_identity: 0.95,
                },
            })
        }

        fn get_stats(&self) -> talaria::core::selection::SelectionStats {
            talaria::core::selection::SelectionStats {
                total_sequences: self.total_sequences,
                references_selected: self.selected_references,
                coverage: 0.8,
                avg_identity: 0.95,
            }
        }

        fn recommend_params(
            &self,
            _num_sequences: usize,
        ) -> talaria::core::selection::RecommendedParams {
            talaria::core::selection::RecommendedParams {
                batch_size: 1000,
                min_length: 50,
                similarity_threshold: 0.9,
            }
        }
    }

    #[test]
    fn test_selector_trait() {
        let selector = MockSelector {
            _name: "MockSelector".to_string(),
            _ratio: 0.3,
            total_sequences: 0,
            selected_references: 0,
        };

        let sequences: Vec<Sequence> = (0..100)
            .map(|i| Sequence {
                id: format!("seq{}", i),
                description: None,
                sequence: vec![],
                taxon_id: None,
                taxonomy_sources: Default::default(),
            })
            .collect();

        let result = selector.select_references(sequences.clone(), 0.3).unwrap();
        assert_eq!(result.references.len(), 30);

        // Test stats from result
        assert_eq!(result.stats.total_sequences, 100);
        assert_eq!(result.stats.references_selected, 30);
        assert_eq!(result.stats.coverage, 0.3);

        // Test recommend_params
        let params = selector.recommend_params(1000);
        assert_eq!(params.batch_size, 1000);
        assert_eq!(params.min_length, 50);
    }
}
