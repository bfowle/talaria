//! Test fixtures and data generators
//!
//! Common test data for use across the Talaria workspace.

use std::fmt::Write;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use talaria_core::types::TaxonId;

/// Test sequence with metadata
#[derive(Debug, Clone)]
pub struct TestSequence {
    pub id: String,
    pub description: String,
    pub sequence: String,
    pub taxon_id: Option<TaxonId>,
    pub database: Option<String>,
}

impl TestSequence {
    /// Create a simple test sequence
    pub fn new(id: impl Into<String>, sequence: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: String::new(),
            sequence: sequence.into(),
            taxon_id: None,
            database: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set taxonomy ID
    pub fn with_taxon(mut self, taxon_id: TaxonId) -> Self {
        self.taxon_id = Some(taxon_id);
        self
    }

    /// Set database
    pub fn with_database(mut self, db: impl Into<String>) -> Self {
        self.database = Some(db.into());
        self
    }

    /// Convert to FASTA format
    pub fn to_fasta(&self) -> String {
        let mut header = format!(">{}", self.id);
        if !self.description.is_empty() {
            write!(&mut header, " {}", self.description).unwrap();
        }
        if let Some(taxon) = self.taxon_id {
            write!(&mut header, " OX={}", taxon.0).unwrap();
        }
        format!("{}\n{}\n", header, self.sequence)
    }
}

/// Generate random DNA sequences
pub fn generate_sequences(count: usize, length: usize) -> Vec<TestSequence> {
    let mut rng = StdRng::seed_from_u64(42); // Deterministic for tests
    let bases = ['A', 'T', 'G', 'C'];

    (0..count)
        .map(|i| {
            let sequence: String = (0..length)
                .map(|_| bases[rng.gen_range(0..4)])
                .collect();

            TestSequence::new(format!("seq_{}", i), sequence)
        })
        .collect()
}

/// Generate sequences with controlled similarity
pub fn generate_similar_sequences(
    count: usize,
    length: usize,
    similarity: f64,
) -> Vec<TestSequence> {
    let mut rng = StdRng::seed_from_u64(42);
    let bases = ['A', 'T', 'G', 'C'];

    // Generate reference sequence
    let reference: String = (0..length)
        .map(|_| bases[rng.gen_range(0..4)])
        .collect();

    let mut sequences = vec![TestSequence::new("seq_0_ref", reference.clone())];

    // Generate similar sequences
    for i in 1..count {
        let mut seq = reference.chars().collect::<Vec<_>>();
        let mutations = ((1.0 - similarity) * length as f64) as usize;

        // Use a set to track mutated positions to ensure exact mutation count
        let mut mutated_positions = std::collections::HashSet::new();

        while mutated_positions.len() < mutations {
            let pos = rng.gen_range(0..length);
            if !mutated_positions.contains(&pos) {
                mutated_positions.insert(pos);
                // Pick a different base than the current one
                let current = seq[pos];
                let mut new_base = bases[rng.gen_range(0..4)];
                while new_base == current {
                    new_base = bases[rng.gen_range(0..4)];
                }
                seq[pos] = new_base;
            }
        }

        sequences.push(TestSequence::new(
            format!("seq_{}", i),
            seq.into_iter().collect::<String>(),
        ));
    }

    sequences
}

/// Create a test FASTA file content
pub fn create_test_fasta(sequences: &[TestSequence]) -> String {
    sequences
        .iter()
        .map(|s| s.to_fasta())
        .collect::<Vec<_>>()
        .join("")
}

/// Common E. coli test sequences
pub fn ecoli_sequences() -> Vec<TestSequence> {
    vec![
        TestSequence::new("ECOLI_GFP", "ATGATGATGATGATGATGATGATGATGATGATGATGATGATGATG")
            .with_description("Green fluorescent protein")
            .with_taxon(TaxonId(562))
            .with_database("uniprot"),
        TestSequence::new("ECOLI_LACZ", "GCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGCGC")
            .with_description("Beta-galactosidase")
            .with_taxon(TaxonId(562))
            .with_database("ncbi"),
        TestSequence::new("ECOLI_RECA", "TATATATATATATATATATATATATATATATATATATATATATAT")
            .with_description("RecA protein")
            .with_taxon(TaxonId(562))
            .with_database("refseq"),
    ]
}

/// Common test taxonomy IDs
pub mod taxonomy {
    use talaria_core::types::TaxonId;

    pub const BACTERIA: TaxonId = TaxonId(2);
    pub const ARCHAEA: TaxonId = TaxonId(2157);
    pub const EUKARYOTA: TaxonId = TaxonId(2759);
    pub const VIRUSES: TaxonId = TaxonId(10239);

    pub const E_COLI: TaxonId = TaxonId(562);
    pub const HUMAN: TaxonId = TaxonId(9606);
    pub const MOUSE: TaxonId = TaxonId(10090);
    pub const YEAST: TaxonId = TaxonId(559292);

    pub const SARS_COV_2: TaxonId = TaxonId(2697049);
    pub const HIV_1: TaxonId = TaxonId(11676);
}

/// Create redundant sequences for testing deduplication
pub fn create_redundant_sequences(unique_count: usize, copies_per_sequence: usize) -> Vec<TestSequence> {
    let base_sequences = generate_sequences(unique_count, 50);
    let mut all_sequences = Vec::new();

    for (_i, seq) in base_sequences.iter().enumerate() {
        for j in 0..copies_per_sequence {
            let mut copy = seq.clone();
            copy.id = format!("{}_{}", seq.id, j);
            if j > 0 {
                copy.description = format!("Copy {} of {}", j, seq.id);
            }
            all_sequences.push(copy);
        }
    }

    all_sequences
}

/// Create sequences with known taxonomy distribution
pub fn create_taxonomic_distribution() -> Vec<TestSequence> {
    use taxonomy::*;

    vec![
        // Bacteria (40%)
        TestSequence::new("bact_1", "ATGATGATGATG").with_taxon(BACTERIA),
        TestSequence::new("bact_2", "GCGCGCGCGCGC").with_taxon(BACTERIA),
        TestSequence::new("ecoli_1", "TATATATATATAT").with_taxon(E_COLI),
        TestSequence::new("ecoli_2", "CACACACACACA").with_taxon(E_COLI),

        // Eukaryota (30%)
        TestSequence::new("human_1", "AAAAAAAAAA").with_taxon(HUMAN),
        TestSequence::new("mouse_1", "TTTTTTTTTT").with_taxon(MOUSE),
        TestSequence::new("yeast_1", "GGGGGGGGGG").with_taxon(YEAST),

        // Viruses (20%)
        TestSequence::new("covid_1", "CCCCCCCCCC").with_taxon(SARS_COV_2),
        TestSequence::new("hiv_1", "AGTCAGTCAG").with_taxon(HIV_1),

        // Archaea (10%)
        TestSequence::new("arch_1", "TGCATGCATG").with_taxon(ARCHAEA),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_generation() {
        let sequences = generate_sequences(10, 100);
        assert_eq!(sequences.len(), 10);
        assert_eq!(sequences[0].sequence.len(), 100);

        // Should be deterministic
        let sequences2 = generate_sequences(10, 100);
        assert_eq!(sequences[0].sequence, sequences2[0].sequence);
    }

    #[test]
    fn test_similar_sequences() {
        let sequences = generate_similar_sequences(5, 100, 0.9);
        assert_eq!(sequences.len(), 5);

        // Reference should be first
        let reference = &sequences[0].sequence;

        // Others should be ~90% similar
        for seq in &sequences[1..] {
            let matches = reference
                .chars()
                .zip(seq.sequence.chars())
                .filter(|(a, b)| a == b)
                .count();

            let similarity = matches as f64 / reference.len() as f64;
            assert!(similarity >= 0.85 && similarity <= 0.95);
        }
    }

    #[test]
    fn test_fasta_generation() {
        let sequences = ecoli_sequences();
        let fasta = create_test_fasta(&sequences);

        assert!(fasta.contains(">ECOLI_GFP"));
        assert!(fasta.contains("OX=562"));
        assert!(fasta.contains("Green fluorescent protein"));
    }
}