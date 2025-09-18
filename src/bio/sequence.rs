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

        // If we have a taxon_id, it's authoritative (from bi-temporal chunk context)
        // We need to replace any existing TaxID in the description
        if let Some(taxon) = self.taxon_id {
            if let Some(desc) = &self.description {
                // Remove existing TaxID from description if present
                let cleaned_desc = Self::remove_taxid_from_description(desc);
                if !cleaned_desc.is_empty() {
                    header.push(' ');
                    header.push_str(&cleaned_desc);
                }
            }
            // Always add the authoritative TaxID from chunk context
            header.push_str(&format!(" TaxID={}", taxon));
        } else {
            // No taxon_id from chunk, use original description as-is
            if let Some(desc) = &self.description {
                header.push(' ');
                header.push_str(desc);
            }
        }

        header
    }

    /// Remove existing TaxID from description to avoid conflicts
    fn remove_taxid_from_description(desc: &str) -> String {
        // Remove TaxID=N pattern
        let desc = regex::Regex::new(r"\s*TaxID=\d+")
            .unwrap()
            .replace_all(desc, "")
            .to_string();

        // Also remove taxon:N pattern for completeness
        let desc = regex::Regex::new(r"\s*taxon:\d+")
            .unwrap()
            .replace_all(&desc, "")
            .to_string();

        desc.trim().to_string()
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
        use crate::cli::output::*;
        let sanitization_items = vec![
            ("Removed sequences", format!("{} (>10% ambiguous)", format_number(removed_count))),
            ("Removed residues", format_number(removed_residues)),
            ("Sequences remaining", format!("{} (from {})", format_number(sanitized.len()), format_number(total))),
        ];
        tree_section("Sanitization Results", sanitization_items, false);
    }

    (sanitized, removed_count)
}