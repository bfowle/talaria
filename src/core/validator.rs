use crate::bio::sequence::Sequence;
use crate::core::delta_encoder::DeltaRecord;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationMetrics {
    pub sequence_coverage: f64,
    pub taxonomic_coverage: f64,
    pub size_reduction: f64,
    pub avg_delta_size: f64,
    pub total_sequences: usize,
    pub reference_count: usize,
    pub child_count: usize,
    pub covered_sequences: usize,
    pub covered_taxa: usize,
    pub total_taxa: usize,
    pub original_file_size: u64,
    pub reduced_file_size: u64,
    pub file_size_reduction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentMetrics {
    pub similarity: f64,
    pub sensitivity: f64,
    pub specificity: f64,
}

pub struct ValidatorImpl;

impl ValidatorImpl {
    pub fn new() -> Self {
        Self
    }
    
    pub fn calculate_metrics(
        &self,
        original: &[Sequence],
        references: &[Sequence],
        deltas: &[DeltaRecord],
        original_file_size: u64,
        reduced_file_size: u64,
    ) -> Result<ValidationMetrics, crate::TalariaError> {
        let total_sequences = original.len();
        let reference_count = references.len();
        let child_count = deltas.len();
        
        // Calculate sequence coverage
        let original_ids: HashSet<String> = original.iter().map(|s| s.id.clone()).collect();
        let covered_ids: HashSet<String> = references
            .iter()
            .map(|s| s.id.clone())
            .chain(deltas.iter().map(|d| d.child_id.clone()))
            .collect();
        
        let covered_sequences = covered_ids.len();
        let sequence_coverage = covered_sequences as f64 / original_ids.len() as f64;
        
        // Calculate taxonomic coverage
        let original_taxa: HashSet<u32> = original
            .iter()
            .filter_map(|s| s.taxon_id)
            .collect();
        
        let covered_taxa: HashSet<u32> = references
            .iter()
            .filter_map(|s| s.taxon_id)
            .chain(deltas.iter().filter_map(|d| d.taxon_id))
            .collect();
        
        let covered_taxa_count = covered_taxa.len();
        let total_taxa = original_taxa.len();
        let taxonomic_coverage = if total_taxa == 0 {
            1.0
        } else {
            covered_taxa_count as f64 / total_taxa as f64
        };
        
        // Calculate size reduction
        let original_size: usize = original.iter().map(|s| s.sequence.len()).sum();
        let reduced_size: usize = references.iter().map(|s| s.sequence.len()).sum();
        let size_reduction = 1.0 - (reduced_size as f64 / original_size as f64);
        
        // Calculate average delta size
        let avg_delta_size = if deltas.is_empty() {
            0.0
        } else {
            let total_delta_size: usize = deltas
                .iter()
                .map(|d| d.deltas.iter().map(|r| r.substitution.len()).sum::<usize>())
                .sum();
            total_delta_size as f64 / deltas.len() as f64
        };
        
        // Calculate file size reduction
        let file_size_reduction = if original_file_size > 0 {
            1.0 - (reduced_file_size as f64 / original_file_size as f64)
        } else {
            0.0
        };
        
        Ok(ValidationMetrics {
            sequence_coverage,
            taxonomic_coverage,
            size_reduction,
            avg_delta_size,
            total_sequences,
            reference_count,
            child_count,
            covered_sequences,
            covered_taxa: covered_taxa_count,
            total_taxa,
            original_file_size,
            reduced_file_size,
            file_size_reduction,
        })
    }
    
    pub fn compare_alignments<P: AsRef<Path>>(
        &self,
        original_results: P,
        reduced_results: P,
    ) -> Result<AlignmentMetrics, crate::TalariaError> {
        // Parse alignment results (assuming BLAST m8 format)
        let original = self.parse_m8(original_results)?;
        let reduced = self.parse_m8(reduced_results)?;
        
        // Calculate metrics
        let mut true_positives = 0;
        let mut false_positives = 0;
        let mut false_negatives = 0;
        
        for (query, orig_hits) in &original {
            if let Some(red_hits) = reduced.get(query) {
                let orig_set: HashSet<_> = orig_hits.iter().collect();
                let red_set: HashSet<_> = red_hits.iter().collect();
                
                true_positives += orig_set.intersection(&red_set).count();
                false_positives += red_set.difference(&orig_set).count();
                false_negatives += orig_set.difference(&red_set).count();
            } else {
                false_negatives += orig_hits.len();
            }
        }
        
        let sensitivity = if true_positives + false_negatives > 0 {
            true_positives as f64 / (true_positives + false_negatives) as f64
        } else {
            0.0
        };
        
        let specificity = if true_positives + false_positives > 0 {
            true_positives as f64 / (true_positives + false_positives) as f64
        } else {
            0.0
        };
        
        let similarity = (sensitivity + specificity) / 2.0;
        
        Ok(AlignmentMetrics {
            similarity,
            sensitivity,
            specificity,
        })
    }
    
    fn parse_m8<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<std::collections::HashMap<String, Vec<String>>, crate::TalariaError> {
        use std::collections::HashMap;
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut results = HashMap::new();
        
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();
            
            if parts.len() >= 2 {
                let query = parts[0].to_string();
                let subject = parts[1].to_string();
                
                results.entry(query).or_insert_with(Vec::new).push(subject);
            }
        }
        
        Ok(results)
    }
}

