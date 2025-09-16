use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sequence {
    pub id: String,
    pub description: Option<String>,
    pub sequence: Vec<u8>,
    pub taxon_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SequenceType {
    Protein,
    Nucleotide,
}

impl Sequence {
    pub fn new(id: String, sequence: Vec<u8>) -> Self {
        Self {
            id,
            description: None,
            sequence,
            taxon_id: None,
        }
    }
    
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    
    pub fn with_taxon(mut self, taxon_id: u32) -> Self {
        self.taxon_id = Some(taxon_id);
        self
    }
    
    pub fn len(&self) -> usize {
        self.sequence.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }
    
    pub fn detect_type(&self) -> SequenceType {
        let protein_chars = b"EFILPQXZ";
        let has_protein = self.sequence.iter()
            .any(|&c| protein_chars.contains(&c.to_ascii_uppercase()));
        
        if has_protein {
            SequenceType::Protein
        } else {
            SequenceType::Nucleotide
        }
    }
    
    pub fn to_string(&self) -> String {
        String::from_utf8_lossy(&self.sequence).to_string()
    }
    
    pub fn header(&self) -> String {
        let mut header = format!(">{}", self.id);

        if let Some(desc) = &self.description {
            header.push(' ');
            header.push_str(desc);
        }

        // Add TaxID to header if present and not already in description
        if let Some(taxon) = self.taxon_id {
            // Check if TaxID is already in the description to avoid duplication
            let has_taxid = self.description
                .as_ref()
                .map(|d| d.contains("TaxID=") || d.contains("OX=") || d.contains("taxon:"))
                .unwrap_or(false);

            if !has_taxid {
                header.push_str(&format!(" TaxID={}", taxon));
            }
        }

        header
    }

    /// Check if sequence contains ambiguous amino acids
    pub fn has_ambiguous_residues(&self) -> bool {
        // Check for ambiguous amino acids: B, J, O, U, Z, X
        // B = Aspartic acid or Asparagine
        // J = Leucine or Isoleucine
        // O = Pyrrolysine (rare)
        // U = Selenocysteine (rare)
        // Z = Glutamic acid or Glutamine
        // X = Any amino acid
        self.sequence.iter().any(|&aa| {
            matches!(aa, b'B' | b'J' | b'O' | b'U' | b'Z' | b'X' | b'b' | b'j' | b'o' | b'u' | b'z' | b'x')
        })
    }

    /// Remove ambiguous residues from sequence
    pub fn sanitize(&mut self) -> usize {
        let original_len = self.sequence.len();
        self.sequence.retain(|&aa| {
            !matches!(aa, b'B' | b'J' | b'O' | b'U' | b'Z' | b'X' | b'b' | b'j' | b'o' | b'u' | b'z' | b'x')
        });
        original_len - self.sequence.len()
    }
}

/// Sanitize a collection of sequences, removing those with ambiguous residues
/// Returns (sanitized sequences, number removed)
pub fn sanitize_sequences(sequences: Vec<Sequence>) -> (Vec<Sequence>, usize) {
    let total = sequences.len();
    let mut removed_count = 0;
    let mut removed_residues = 0;

    let sanitized: Vec<Sequence> = sequences
        .into_iter()
        .filter_map(|mut seq| {
            if seq.has_ambiguous_residues() {
                // Try to sanitize by removing ambiguous residues
                let removed = seq.sanitize();

                // If too many residues were removed (>10% of sequence), discard it
                if removed > 0 && seq.len() > 0 {
                    let removal_ratio = removed as f64 / (seq.len() + removed) as f64;
                    if removal_ratio > 0.1 {
                        removed_count += 1;
                        None
                    } else {
                        removed_residues += removed;
                        Some(seq)
                    }
                } else if seq.is_empty() {
                    removed_count += 1;
                    None
                } else {
                    Some(seq)
                }
            } else {
                Some(seq)
            }
        })
        .filter(|seq| seq.len() > 0) // Remove empty sequences
        .collect();

    if removed_count > 0 || removed_residues > 0 {
        println!("  Sanitized sequences: removed {} sequences with >10% ambiguous residues", removed_count);
        println!("  Removed {} ambiguous residues from remaining sequences", removed_residues);
        println!("  Sequences after sanitization: {} (from {})", sanitized.len(), total);
    }

    (sanitized, removed_count)
}