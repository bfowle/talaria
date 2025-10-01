//! Sequence-related types shared across Talaria

use serde::{Deserialize, Serialize};

/// Type of biological sequence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SequenceType {
    /// Protein/amino acid sequence
    Protein,
    /// DNA sequence
    DNA,
    /// RNA sequence
    RNA,
    /// Generic nucleotide (DNA or RNA)
    Nucleotide,
    /// Unknown sequence type
    Unknown,
}

impl SequenceType {
    /// Check if this is a nucleotide sequence (DNA or RNA)
    pub fn is_nucleotide(&self) -> bool {
        matches!(self, Self::DNA | Self::RNA | Self::Nucleotide)
    }

    /// Check if this is a protein sequence
    pub fn is_protein(&self) -> bool {
        matches!(self, Self::Protein)
    }

    /// Detect sequence type from sequence content
    pub fn detect(sequence: &str) -> Self {
        let upper = sequence.to_uppercase();
        let nucleotide_chars = ['A', 'T', 'G', 'C', 'U', 'N'];

        let total_chars = upper.len();
        if total_chars == 0 {
            return Self::Unknown;
        }

        let nucleotide_count = upper
            .chars()
            .filter(|c| nucleotide_chars.contains(c))
            .count();

        // If > 90% are nucleotide characters, it's likely DNA/RNA
        if nucleotide_count as f32 / total_chars as f32 > 0.9 {
            if upper.contains('U') {
                Self::RNA
            } else {
                Self::DNA
            }
        } else {
            Self::Protein
        }
    }
}

impl Default for SequenceType {
    fn default() -> Self {
        Self::Unknown
    }
}

impl std::fmt::Display for SequenceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Protein => write!(f, "Protein"),
            Self::DNA => write!(f, "DNA"),
            Self::RNA => write!(f, "RNA"),
            Self::Nucleotide => write!(f, "Nucleotide"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}
