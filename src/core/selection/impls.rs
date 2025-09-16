/// Trait implementations for reference selection strategies

use super::traits::{
    ReferenceSelector as ReferenceSelectorTrait,
    SelectionResult as TraitSelectionResult,
    SelectionStats, RecommendedParams,
    AlignmentBasedSelector, AlignmentScore,
};
use crate::core::reference_selector::{ReferenceSelector, SelectionAlgorithm};
use crate::bio::sequence::Sequence;
use anyhow::Result;
use std::collections::HashSet;

/// Implement the trait for our concrete ReferenceSelector
impl ReferenceSelectorTrait for ReferenceSelector {
    fn select_references(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> Result<TraitSelectionResult> {
        // Use the appropriate method based on configuration
        let result = if self.use_alignment {
            self.select_references_with_alignment(sequences.clone(), target_ratio)
        } else if self.use_similarity {
            self.select_references_with_similarity(sequences.clone(), target_ratio)
        } else {
            self.simple_select_references(sequences.clone(), target_ratio)
        };

        // Convert to trait result type
        let total = sequences.len();
        let refs = result.references.len();
        let children_count: usize = result.children.values().map(|v| v.len()).sum();

        Ok(TraitSelectionResult {
            references: result.references,
            children: result.children,
            discarded: result.discarded.clone(),
            stats: SelectionStats {
                total_sequences: total,
                selected_references: refs,
                assigned_children: children_count,
                discarded_sequences: result.discarded.len(),
                coverage_ratio: result.discarded.len() as f64 / total as f64,
                avg_children_per_reference: if refs > 0 {
                    children_count as f64 / refs as f64
                } else {
                    0.0
                },
                selection_time_ms: 0, // Would need timing
            },
        })
    }

    fn calculate_coverage(
        &self,
        references: &[Sequence],
        all_sequences: &[Sequence],
    ) -> f64 {
        // Calculate how many sequences are covered by the references
        let ref_ids: HashSet<_> = references.iter().map(|r| &r.id).collect();
        let covered = all_sequences.iter()
            .filter(|s| ref_ids.contains(&s.id))
            .count();

        covered as f64 / all_sequences.len() as f64
    }

    fn strategy_name(&self) -> &str {
        match self.selection_algorithm {
            SelectionAlgorithm::SinglePass => "single-pass",
            SelectionAlgorithm::SimilarityMatrix => "similarity-matrix",
            SelectionAlgorithm::Hybrid => "hybrid",
        }
    }

    fn estimate_memory_usage(&self, num_sequences: usize) -> usize {
        match self.selection_algorithm {
            SelectionAlgorithm::SinglePass => {
                // O(n) memory for sequences and references
                num_sequences * 200 // Approximate bytes per sequence
            }
            SelectionAlgorithm::SimilarityMatrix => {
                // O(n²) memory for similarity matrix
                let matrix_size = num_sequences * num_sequences * 8; // 8 bytes per f64
                let seq_size = num_sequences * 200;
                matrix_size + seq_size
            }
            SelectionAlgorithm::Hybrid => {
                // Between single-pass and matrix
                num_sequences * num_sequences * 4 + num_sequences * 200
            }
        }
    }

    fn supports_incremental(&self) -> bool {
        // SinglePass could support incremental, matrix cannot easily
        matches!(self.selection_algorithm, SelectionAlgorithm::SinglePass)
    }

    fn recommend_parameters(
        &self,
        num_sequences: usize,
        avg_sequence_length: usize,
    ) -> RecommendedParams {
        let (ratio, batch_size) = if num_sequences > 100000 {
            // Large dataset - be more aggressive
            (0.1, Some(10000))
        } else if num_sequences > 10000 {
            // Medium dataset
            (0.2, Some(5000))
        } else {
            // Small dataset - can be less aggressive
            (0.3, None)
        };

        RecommendedParams {
            target_ratio: ratio,
            min_length: 50.max(avg_sequence_length / 10),
            similarity_threshold: if self.use_alignment { 0.7 } else { 0.9 },
            use_taxonomy: num_sequences > 50000, // Use taxonomy for large datasets
            batch_size,
        }
    }
}

/// Implement AlignmentBasedSelector for our ReferenceSelector
impl AlignmentBasedSelector for ReferenceSelector {
    fn select_with_alignments(
        &self,
        sequences: Vec<Sequence>,
        _alignments: &[AlignmentScore],
        target_ratio: f64,
    ) -> Result<TraitSelectionResult> {
        // Convert AlignmentScore to our internal format and use existing logic
        // This would need the actual alignment implementation
        // For now, delegate to the trait method which handles the conversion
        <Self as ReferenceSelectorTrait>::select_references(self, sequences, target_ratio)
    }

    fn calculate_alignments(
        &self,
        sequences: &[Sequence],
    ) -> Result<Vec<AlignmentScore>> {
        // This would use LAMBDA or other alignment tools
        let mut scores = Vec::new();

        // For now, return empty - would need actual alignment implementation
        for i in 0..sequences.len() {
            for j in i+1..sequences.len() {
                scores.push(AlignmentScore {
                    seq1_id: sequences[i].id.clone(),
                    seq2_id: sequences[j].id.clone(),
                    score: 0.0,
                    identity: 0.0,
                    coverage: 0.0,
                });
            }
        }

        Ok(scores)
    }

    fn min_alignment_score(&self) -> f64 {
        self.similarity_threshold
    }

    fn set_min_alignment_score(&mut self, score: f64) {
        self.similarity_threshold = score;
    }
}

/// Factory function to create selector based on algorithm choice
pub fn create_selector(algorithm: SelectionAlgorithm) -> Box<dyn ReferenceSelectorTrait> {
    Box::new(
        ReferenceSelector::new()
            .with_selection_algorithm(algorithm)
    )
}

/// Create a selector with full configuration
pub fn create_configured_selector(
    algorithm: SelectionAlgorithm,
    min_length: usize,
    similarity_threshold: f64,
    taxonomy_aware: bool,
) -> Box<dyn ReferenceSelectorTrait> {
    Box::new(
        ReferenceSelector::new()
            .with_selection_algorithm(algorithm)
            .with_min_length(min_length)
            .with_similarity_threshold(similarity_threshold)
            .with_taxonomy_aware(taxonomy_aware)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_implementation() {
        let selector: Box<dyn ReferenceSelectorTrait> = create_selector(SelectionAlgorithm::SinglePass);

        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 100]),
            Sequence::new("seq2".to_string(), vec![65; 80]),
        ];

        let result = selector.select_references(sequences.clone(), 0.5);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert!(!result.references.is_empty());
        assert_eq!(result.stats.total_sequences, 2);
    }

    #[test]
    fn test_strategy_names() {
        let sp_selector = create_selector(SelectionAlgorithm::SinglePass);
        assert_eq!(sp_selector.strategy_name(), "single-pass");

        let sm_selector = create_selector(SelectionAlgorithm::SimilarityMatrix);
        assert_eq!(sm_selector.strategy_name(), "similarity-matrix");
    }

    #[test]
    fn test_memory_estimation() {
        let selector = create_selector(SelectionAlgorithm::SimilarityMatrix);
        let mem_small = selector.estimate_memory_usage(100);
        let mem_large = selector.estimate_memory_usage(1000);

        assert!(mem_large > mem_small);
        // Matrix algorithm should use quadratic memory
        assert!(mem_large > mem_small * 10);
    }

    #[test]
    fn test_polymorphic_usage() {
        // Test that we can use different selectors through the trait
        let selectors: Vec<Box<dyn ReferenceSelectorTrait>> = vec![
            create_selector(SelectionAlgorithm::SinglePass),
            create_selector(SelectionAlgorithm::SimilarityMatrix),
        ];

        let sequences = vec![
            Sequence::new("test".to_string(), vec![65; 100]),
        ];

        for selector in selectors {
            let result = selector.select_references(sequences.clone(), 0.5);
            assert!(result.is_ok());
        }
    }
}