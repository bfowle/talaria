use crate::taxonomy::{
    TaxonomyDiscrepancy, TaxonomyEnrichable, TaxonomyResolution, TaxonomyResolver, TaxonomySource,
    TaxonomySources,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sequence {
    pub id: String,
    pub description: Option<String>,
    pub sequence: Vec<u8>,
    pub taxon_id: Option<u32>, // Legacy field, kept for backward compatibility
    #[serde(default)]
    pub taxonomy_sources: TaxonomySources, // New: track all taxonomy sources
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
            taxonomy_sources: TaxonomySources::new(),
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
        let has_protein = self
            .sequence
            .iter()
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
            matches!(
                aa,
                b'B' | b'J' | b'O' | b'U' | b'Z' | b'X' | b'b' | b'j' | b'o' | b'u' | b'z' | b'x'
            )
        })
    }

    /// Remove ambiguous residues from sequence
    pub fn sanitize(&mut self) -> usize {
        let original_len = self.sequence.len();
        self.sequence.retain(|&aa| {
            !matches!(
                aa,
                b'B' | b'J' | b'O' | b'U' | b'Z' | b'X' | b'b' | b'j' | b'o' | b'u' | b'z' | b'x'
            )
        });
        original_len - self.sequence.len()
    }
}

/// Sanitize a collection of sequences, removing those with ambiguous residues
/// Returns (sanitized sequences, number removed)
pub fn sanitize_sequences(sequences: Vec<Sequence>) -> (Vec<Sequence>, usize) {
    use indicatif::{ProgressBar, ProgressStyle};
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let total = sequences.len();
    let removed_count = AtomicUsize::new(0);
    let removed_residues = AtomicUsize::new(0);

    // Create progress bar for sanitization
    let show_progress = total > 1000 && std::env::var("TALARIA_SILENT").is_err();
    let pb = if show_progress {
        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} sequences ({per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message("Sanitizing sequences...");
        Some(std::sync::Arc::new(pb))
    } else {
        None
    };

    // Process sequences in parallel
    let processed = std::sync::Arc::new(AtomicUsize::new(0));
    let processed_clone = processed.clone();

    let sanitized: Vec<Sequence> = sequences
        .into_par_iter()
        .filter_map(|mut seq| {
            // Update progress counter
            let count = processed_clone.fetch_add(1, Ordering::Relaxed);
            if let Some(ref pb) = pb {
                if count % 100 == 0 {
                    pb.set_position(count as u64);
                }
            }

            if seq.has_ambiguous_residues() {
                // Try to sanitize by removing ambiguous residues
                let removed = seq.sanitize();

                // If too many residues were removed (>10% of sequence), discard it
                if removed > 0 && !seq.is_empty() {
                    let removal_ratio = removed as f64 / (seq.len() + removed) as f64;
                    if removal_ratio > 0.1 {
                        removed_count.fetch_add(1, Ordering::Relaxed);
                        None
                    } else {
                        removed_residues.fetch_add(removed, Ordering::Relaxed);
                        Some(seq)
                    }
                } else if seq.is_empty() {
                    removed_count.fetch_add(1, Ordering::Relaxed);
                    None
                } else {
                    Some(seq)
                }
            } else {
                Some(seq)
            }
        })
        .filter(|seq| !seq.is_empty()) // Remove empty sequences
        .collect();

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    let final_removed_count = removed_count.load(Ordering::Relaxed);
    let _final_removed_residues = removed_residues.load(Ordering::Relaxed);

    // Return the sanitization results without display logic
    // Display should be handled by the CLI layer
    (sanitized, final_removed_count)
}

// Implement TaxonomyResolver trait for Sequence
impl TaxonomyResolver for Sequence {
    fn resolve_taxonomy(&self) -> TaxonomyResolution {
        let sources = self.taxonomy_sources();
        let all_sources = sources.all_sources();

        if all_sources.is_empty() {
            // Check legacy taxon_id field
            if let Some(id) = self.taxon_id {
                return TaxonomyResolution::Unanimous {
                    taxon_id: id,
                    sources: vec![(TaxonomySource::Header, id)],
                };
            }
            return TaxonomyResolution::None;
        }

        let unique = sources.unique_ids();

        match unique.len() {
            0 => TaxonomyResolution::None,
            1 => TaxonomyResolution::Unanimous {
                taxon_id: *unique.iter().next().unwrap(),
                sources: all_sources,
            },
            _ => {
                let resolved = sources
                    .resolve_with_priority()
                    .or(self.taxon_id)
                    .unwrap_or(0);

                TaxonomyResolution::Conflicted {
                    candidates: all_sources,
                    resolved_to: resolved,
                }
            }
        }
    }

    fn taxonomy_sources(&self) -> &TaxonomySources {
        &self.taxonomy_sources
    }

    fn taxonomy_sources_mut(&mut self) -> &mut TaxonomySources {
        &mut self.taxonomy_sources
    }

    fn detect_discrepancies(&self) -> Vec<TaxonomyDiscrepancy> {
        match self.resolve_taxonomy() {
            TaxonomyResolution::Conflicted { candidates, .. } => {
                vec![TaxonomyDiscrepancy {
                    sequence_id: self.id.clone(),
                    conflicts: candidates,
                    resolution_strategy: "priority-based",
                }]
            }
            _ => vec![],
        }
    }
}

// Implement TaxonomyEnrichable trait for Sequence
impl TaxonomyEnrichable for Sequence {
    fn enrich_from_mappings(&mut self, mappings: &HashMap<String, u32>) {
        let accession = self.extract_accession();
        if let Some(&taxid) = mappings.get(&accession) {
            self.taxonomy_sources.mapping_lookup = Some(taxid);
        }
    }

    fn enrich_from_user(&mut self, taxid: u32) {
        self.taxonomy_sources.user_specified = Some(taxid);
    }

    fn enrich_from_header(&mut self) {
        if let Some(taxid) =
            crate::taxonomy::parse_taxonomy_from_description(&self.description)
        {
            self.taxonomy_sources.header_parsed = Some(taxid);
        }
    }

    fn enrich_from_chunk(&mut self, taxid: u32) {
        self.taxonomy_sources.chunk_context = Some(taxid);
    }

    fn extract_accession(&self) -> String {
        crate::taxonomy::extract_accession_from_id(&self.id)
    }

    fn get_description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}
