/// Delta encoding and decoding for sequence compression
use crate::alignment::{Alignment, DetailedAlignment};
use crate::sequence::Sequence;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRecord {
    pub child_id: String,
    pub reference_id: String,
    pub taxon_id: Option<u32>,
    pub deltas: Vec<DeltaRange>,
    /// Track header changes separately from sequence changes
    pub header_change: Option<HeaderChange>,
}

/// Tracks changes to FASTA headers (ID and description)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderChange {
    /// The old description (from reference)
    pub old_description: Option<String>,
    /// The new description (in child)
    pub new_description: Option<String>,
    /// Whether the ID itself changed
    pub id_changed: bool,
    /// The old ID if it changed
    pub old_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRange {
    pub start: usize,
    pub end: usize,
    pub substitution: Vec<u8>,
}

impl DeltaRange {
    /// Create a new delta range
    pub fn new(start: usize, end: usize, substitution: Vec<u8>) -> Self {
        Self {
            start,
            end,
            substitution,
        }
    }

    /// Check if this range represents a single position
    pub fn is_single(&self) -> bool {
        self.start == self.end
    }
}

pub struct DeltaEncoder;

impl Default for DeltaEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaEncoder {
    pub fn new() -> Self {
        Self
    }

    /// Encode a child sequence as deltas from a reference
    pub fn encode(&self, reference: &Sequence, child: &Sequence) -> DeltaRecord {
        let alignment = Alignment::global(reference, child);
        let deltas = self.compress_deltas(&alignment);

        // Track header changes
        let header_change = if reference.description != child.description {
            Some(HeaderChange {
                old_description: reference.description.clone(),
                new_description: child.description.clone(),
                id_changed: false, // IDs should match in our current model
                old_id: None,
            })
        } else {
            None
        };

        DeltaRecord {
            child_id: child.id.clone(),
            reference_id: reference.id.clone(),
            taxon_id: child.taxon_id,
            deltas,
            header_change,
        }
    }

    /// Compress consecutive deltas into ranges (like in deltas-db-gen.cpp)
    fn compress_deltas(&self, alignment: &DetailedAlignment) -> Vec<DeltaRange> {
        if alignment.deltas.is_empty() {
            return Vec::new();
        }

        let mut ranges = Vec::new();
        let mut current_start = alignment.deltas[0].position;
        let mut current_sub = vec![alignment.deltas[0].query];
        let mut last_pos = alignment.deltas[0].position;

        for delta in &alignment.deltas[1..] {
            // Check if this delta is consecutive and has the same substitution pattern
            if delta.position == last_pos + 1 && current_sub.last() == Some(&delta.query) {
                // Extend current range
                last_pos = delta.position;
            } else {
                // Save current range and start new one
                ranges.push(DeltaRange::new(
                    current_start,
                    last_pos,
                    current_sub.clone(),
                ));
                current_start = delta.position;
                current_sub = vec![delta.query];
                last_pos = delta.position;
            }
        }

        // Don't forget the last range
        ranges.push(DeltaRange::new(current_start, last_pos, current_sub));

        ranges
    }

    /// Encode multiple children against their references
    pub fn encode_batch(
        &self,
        references: &HashMap<String, Sequence>,
        children: &HashMap<String, Vec<String>>,
        all_sequences: &HashMap<String, Sequence>,
    ) -> Vec<DeltaRecord> {
        self.encode_batch_with_progress(references, children, all_sequences, |_| {})
    }

    pub fn encode_batch_with_progress<F>(
        &self,
        references: &HashMap<String, Sequence>,
        children: &HashMap<String, Vec<String>>,
        all_sequences: &HashMap<String, Sequence>,
        progress_callback: F,
    ) -> Vec<DeltaRecord>
    where
        F: Fn(&str) + Send + Sync,
    {
        use rayon::prelude::*;
        use std::sync::Arc;

        let callback = Arc::new(progress_callback);

        let delta_records: Vec<DeltaRecord> = children
            .par_iter()
            .flat_map(|(ref_id, child_ids)| {
                let reference = match references.get(ref_id) {
                    Some(r) => r,
                    None => return Vec::new(),
                };

                let callback_clone = callback.clone();
                child_ids
                    .par_iter()
                    .filter_map(move |child_id| {
                        all_sequences.get(child_id).map(|child| {
                            callback_clone(child_id);
                            self.encode(reference, child)
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        delta_records
    }
}

pub struct DeltaReconstructor;

impl Default for DeltaReconstructor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaReconstructor {
    pub fn new() -> Self {
        Self
    }

    /// Reconstruct a child sequence from a reference and deltas
    pub fn reconstruct(&self, reference: &Sequence, delta_record: &DeltaRecord) -> Sequence {
        let mut reconstructed = reference.sequence.clone();

        // Apply deltas in order
        for range in &delta_record.deltas {
            if range.is_single() {
                // Single position substitution
                if range.start < reconstructed.len() {
                    reconstructed[range.start] = range.substitution[0];
                }
            } else {
                // Range substitution
                for (i, pos) in (range.start..=range.end).enumerate() {
                    if pos < reconstructed.len() && i < range.substitution.len() {
                        reconstructed[pos] = range.substitution[i % range.substitution.len()];
                    }
                }
            }
        }

        Sequence {
            id: delta_record.child_id.clone(),
            description: reference.description.clone(),
            sequence: reconstructed,
            taxon_id: delta_record.taxon_id,
            taxonomy_sources: Default::default(),
        }
    }

    /// Reconstruct all sequences from references and deltas
    pub fn reconstruct_all(
        &self,
        references: Vec<Sequence>,
        deltas: Vec<DeltaRecord>,
        filter_ids: Vec<String>,
    ) -> Result<Vec<Sequence>, talaria_core::TalariaError> {
        let ref_map: HashMap<String, Sequence> =
            references.into_iter().map(|s| (s.id.clone(), s)).collect();

        let filter_set: Option<std::collections::HashSet<String>> = if filter_ids.is_empty() {
            None
        } else {
            Some(filter_ids.into_iter().collect())
        };

        let mut reconstructed = Vec::new();

        // Add references if they match filter
        for (id, seq) in &ref_map {
            if filter_set.as_ref().is_none_or(|f| f.contains(id)) {
                reconstructed.push(seq.clone());
            }
        }

        // Reconstruct children
        for delta in deltas {
            if filter_set
                .as_ref()
                .is_none_or(|f| f.contains(&delta.child_id))
            {
                if let Some(reference) = ref_map.get(&delta.reference_id) {
                    reconstructed.push(self.reconstruct(reference, &delta));
                }
            }
        }

        Ok(reconstructed)
    }
}

/// Format deltas for output (similar to original .dat format)
pub fn format_deltas_dat(delta_record: &DeltaRecord) -> String {
    // Escape special characters in IDs to avoid parsing issues
    let escape_id = |id: &str| -> String {
        id.chars().map(|c| match c {
            '\t' => "\\t".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\\' => "\\\\".to_string(),
            ',' => "\\x2c".to_string(), // Escape comma to avoid format detection issues
            '>' => "\\x3e".to_string(), // Escape > to avoid format detection issues
            _ => c.to_string(),
        }).collect()
    };

    let mut parts = vec![
        escape_id(&delta_record.child_id),
        escape_id(&delta_record.reference_id),
    ];

    // Include taxon_id if present
    if let Some(taxon) = delta_record.taxon_id {
        parts.push(format!("taxon:{}", taxon));
    }

    for range in &delta_record.deltas {
        // Escape special characters in substitution for safe serialization
        let substitution_str: String = range.substitution.iter()
            .map(|&b| {
                match b {
                    b'\n' => "\\n".to_string(),
                    b'\t' => "\\t".to_string(),
                    b'\r' => "\\r".to_string(),
                    b'\\' => "\\\\".to_string(),
                    b',' => "\\x2c".to_string(),  // Escape comma since it's our delimiter
                    b if b.is_ascii_graphic() || b == b' ' => (b as char).to_string(),
                    _ => format!("\\x{:02x}", b)
                }
            })
            .collect();

        let delta_str = if range.is_single() {
            format!("{},{}", range.start, substitution_str)
        } else {
            format!("{}>{},{}", range.start, range.end, substitution_str)
        };
        parts.push(delta_str);
    }

    parts.join("\t")
}

/// Helper function to unescape substitution strings
fn unescape_substitution(s: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => result.push(b'\n'),
                    't' => result.push(b'\t'),
                    'r' => result.push(b'\r'),
                    '\\' => result.push(b'\\'),
                    'x' => {
                        // Read two hex digits
                        let hex: String = chars.by_ref().take(2).collect();
                        if let Ok(b) = u8::from_str_radix(&hex, 16) {
                            result.push(b);
                        }
                    }
                    _ => {
                        result.push(c as u8);
                        result.push(next as u8);
                    }
                }
            } else {
                result.push(c as u8);
            }
        } else {
            result.push(c as u8);
        }
    }

    result
}

/// Parse deltas from .dat format (supports both old and new formats)
pub fn parse_deltas_dat(line: &str) -> Result<DeltaRecord, talaria_core::TalariaError> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.is_empty() {
        return Err(talaria_core::TalariaError::Parse("Empty delta line".to_string()));
    }

    // Unescape special characters in IDs
    let unescape_id = |id: &str| -> String {
        let mut result = String::new();
        let mut chars = id.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    match next {
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        'r' => result.push('\r'),
                        '\\' => result.push('\\'),
                        'x' => {
                            // Read two hex digits
                            let hex: String = chars.by_ref().take(2).collect();
                            if let Ok(b) = u8::from_str_radix(&hex, 16) {
                                result.push(b as char);
                            } else {
                                // If not valid hex, include the original chars
                                result.push(c);
                                result.push(next);
                                result.push_str(&hex);
                            }
                        }
                        _ => {
                            result.push(c);
                            result.push(next);
                        }
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }
        result
    };

    let child_id = unescape_id(parts[0]);

    // Improved format detection:
    // A delta field must have the pattern: number[>number],data
    // Anything else is treated as a reference_id
    let is_delta_field = |field: &str| -> bool {
        if field.starts_with("taxon:") {
            return false;
        }
        // A delta field must have numeric start position followed by comma
        if let Some(comma_pos) = field.find(',') {
            let prefix = &field[..comma_pos];
            // Check for range format: start>end
            if let Some(gt_pos) = prefix.find('>') {
                prefix[..gt_pos].parse::<usize>().is_ok() &&
                prefix[gt_pos+1..].parse::<usize>().is_ok()
            } else {
                // Single position format
                prefix.parse::<usize>().is_ok()
            }
        } else {
            false
        }
    };

    let (reference_id, delta_start_idx) = if parts.len() > 1 {
        if is_delta_field(parts[1]) {
            // Old format: child_id followed directly by deltas
            (String::new(), 1)
        } else {
            // New format: child_id, reference_id, then deltas
            (unescape_id(parts[1]), 2)
        }
    } else {
        // Only child_id, no deltas
        (String::new(), 1)
    };

    let mut deltas = Vec::new();
    let mut taxon_id = None;

    // Parse deltas and taxon_id starting from the determined index
    for part in &parts[delta_start_idx..] {
        // Check for taxon ID
        if part.starts_with("taxon:") {
            if let Ok(taxon) = part[6..].parse::<u32>() {
                taxon_id = Some(taxon);
            }
            continue;
        }
        if part.contains('>') {
            // Range format: start>end,substitution
            let range_parts: Vec<&str> = part.split(',').collect();
            if range_parts.len() != 2 {
                continue;
            }

            let pos_parts: Vec<&str> = range_parts[0].split('>').collect();
            if pos_parts.len() != 2 {
                continue;
            }

            if let (Ok(start), Ok(end)) =
                (pos_parts[0].parse::<usize>(), pos_parts[1].parse::<usize>())
            {
                // Unescape the substitution string
                let substitution = unescape_substitution(range_parts[1]);
                deltas.push(DeltaRange::new(start, end, substitution));
            }
        } else {
            // Single position format: position,substitution
            let single_parts: Vec<&str> = part.split(',').collect();
            if single_parts.len() != 2 {
                continue;
            }

            if let Ok(pos) = single_parts[0].parse::<usize>() {
                // Unescape the substitution string
                let substitution = unescape_substitution(single_parts[1]);
                deltas.push(DeltaRange::new(pos, pos, substitution));
            }
        }
    }

    Ok(DeltaRecord {
        child_id,
        reference_id,
        taxon_id,  // Use the parsed taxon_id
        deltas,
        header_change: None, // No header change tracking in the old format
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_format_includes_reference_id() {
        // Create a delta record with reference_id
        let delta = DeltaRecord {
            child_id: "child_seq_1".to_string(),
            reference_id: "ref_seq_1".to_string(),
            taxon_id: Some(9606),
            header_change: None,
            deltas: vec![
                DeltaRange::new(1, 1, b"A".to_vec()),
                DeltaRange::new(5, 5, b"T".to_vec()),
            ],
        };

        // Format and parse back
        let formatted = format_deltas_dat(&delta);
        let parsed = parse_deltas_dat(&formatted).unwrap();

        // Check that reference_id is preserved
        assert_eq!(
            parsed.reference_id, "ref_seq_1",
            "reference_id should be preserved through format/parse cycle"
        );
        assert_eq!(parsed.child_id, "child_seq_1");
        assert_eq!(parsed.deltas.len(), 2);
    }

    #[test]
    fn test_reconstruction_with_reference_id() {
        use crate::sequence::Sequence;

        // Create reference and child sequences
        let reference = Sequence::new("ref_seq_1".to_string(), b"ACGTACGTACGT".to_vec());
        let child = Sequence::new("child_seq_1".to_string(), b"ACATACGTACGT".to_vec());

        // Encode the child as deltas
        let encoder = DeltaEncoder::new();
        let delta_record = encoder.encode(&reference, &child);

        // Verify reference_id is set
        assert_eq!(delta_record.reference_id, "ref_seq_1");
        assert_eq!(delta_record.child_id, "child_seq_1");

        // Format, parse, and verify reference_id is preserved
        let formatted = format_deltas_dat(&delta_record);
        let parsed = parse_deltas_dat(&formatted).unwrap();
        assert_eq!(
            parsed.reference_id, "ref_seq_1",
            "reference_id must be preserved for reconstruction to work"
        );

        // Reconstruct and verify
        let reconstructor = DeltaReconstructor::new();
        let reconstructed = reconstructor.reconstruct(&reference, &parsed);
        assert_eq!(reconstructed.id, "child_seq_1");
        assert_eq!(reconstructed.sequence, child.sequence);
    }
}
