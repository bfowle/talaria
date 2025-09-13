/// Delta encoding and decoding for sequence compression

use crate::bio::alignment::{Alignment, AlignmentResult};
use crate::bio::sequence::Sequence;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRecord {
    pub child_id: String,
    pub reference_id: String,
    pub taxon_id: Option<u32>,
    pub deltas: Vec<DeltaRange>,
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
        Self { start, end, substitution }
    }
    
    /// Check if this range represents a single position
    pub fn is_single(&self) -> bool {
        self.start == self.end
    }
}

pub struct DeltaEncoder;

impl DeltaEncoder {
    pub fn new() -> Self {
        Self
    }
    
    /// Encode a child sequence as deltas from a reference
    pub fn encode(&self, reference: &Sequence, child: &Sequence) -> DeltaRecord {
        let alignment = Alignment::global(reference, child);
        let deltas = self.compress_deltas(&alignment);
        
        DeltaRecord {
            child_id: child.id.clone(),
            reference_id: reference.id.clone(),
            taxon_id: child.taxon_id,
            deltas,
        }
    }
    
    /// Compress consecutive deltas into ranges (like in deltas-db-gen.cpp)
    fn compress_deltas(&self, alignment: &AlignmentResult) -> Vec<DeltaRange> {
        if alignment.deltas.is_empty() {
            return Vec::new();
        }
        
        let mut ranges = Vec::new();
        let mut current_start = alignment.deltas[0].position;
        let mut current_sub = vec![alignment.deltas[0].query];
        let mut last_pos = alignment.deltas[0].position;
        
        for delta in &alignment.deltas[1..] {
            // Check if this delta is consecutive and has the same substitution pattern
            if delta.position == last_pos + 1 && 
               current_sub.last() == Some(&delta.query) {
                // Extend current range
                last_pos = delta.position;
            } else {
                // Save current range and start new one
                ranges.push(DeltaRange::new(current_start, last_pos, current_sub.clone()));
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
                        all_sequences
                            .get(child_id)
                            .map(|child| {
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
        }
    }
    
    /// Reconstruct all sequences from references and deltas
    pub fn reconstruct_all(
        &self,
        references: Vec<Sequence>,
        deltas: Vec<DeltaRecord>,
        filter_ids: Vec<String>,
    ) -> Result<Vec<Sequence>, crate::TalariaError> {
        let ref_map: HashMap<String, Sequence> = references
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        
        let filter_set: Option<std::collections::HashSet<String>> = if filter_ids.is_empty() {
            None
        } else {
            Some(filter_ids.into_iter().collect())
        };
        
        let mut reconstructed = Vec::new();
        
        // Add references if they match filter
        for (id, seq) in &ref_map {
            if filter_set.as_ref().map_or(true, |f| f.contains(id)) {
                reconstructed.push(seq.clone());
            }
        }
        
        // Reconstruct children
        for delta in deltas {
            if filter_set.as_ref().map_or(true, |f| f.contains(&delta.child_id)) {
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
    let mut parts = vec![
        delta_record.child_id.clone(),
        delta_record.reference_id.clone(),  // Include reference_id in output
    ];

    for range in &delta_record.deltas {
        let delta_str = if range.is_single() {
            format!("{},{}", range.start, String::from_utf8_lossy(&range.substitution))
        } else {
            format!("{}>{},{}",
                range.start,
                range.end,
                String::from_utf8_lossy(&range.substitution))
        };
        parts.push(delta_str);
    }

    parts.join("\t")
}

/// Parse deltas from .dat format (supports both old and new formats)
pub fn parse_deltas_dat(line: &str) -> Result<DeltaRecord, crate::TalariaError> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.is_empty() {
        return Err(crate::TalariaError::Parse(
            "Empty delta line".to_string()
        ));
    }

    let child_id = parts[0].to_string();
    
    // Detect format: if second field contains ',' or '>', it's a delta (old format)
    // Otherwise it's a reference_id (new format)
    let (reference_id, delta_start_idx) = if parts.len() > 1 {
        let second_field = parts[1];
        if second_field.contains(',') || second_field.contains('>') {
            // Old format: child_id followed directly by deltas
            (String::new(), 1)
        } else {
            // New format: child_id, reference_id, then deltas
            (second_field.to_string(), 2)
        }
    } else {
        // Only child_id, no deltas
        (String::new(), 1)
    };
    
    let mut deltas = Vec::new();

    // Parse deltas starting from the determined index
    for part in &parts[delta_start_idx..] {
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

            if let (Ok(start), Ok(end)) = (pos_parts[0].parse::<usize>(), pos_parts[1].parse::<usize>()) {
                let substitution = range_parts[1].as_bytes().to_vec();
                deltas.push(DeltaRange::new(start, end, substitution));
            }
        } else {
            // Single position format: position,substitution
            let single_parts: Vec<&str> = part.split(',').collect();
            if single_parts.len() != 2 {
                continue;
            }

            if let Ok(pos) = single_parts[0].parse::<usize>() {
                let substitution = single_parts[1].as_bytes().to_vec();
                deltas.push(DeltaRange::new(pos, pos, substitution));
            }
        }
    }

    Ok(DeltaRecord {
        child_id,
        reference_id,  // Use the parsed reference_id
        taxon_id: None,
        deltas,
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
            deltas: vec![
                DeltaRange::new(1, 1, b"A".to_vec()),
                DeltaRange::new(5, 5, b"T".to_vec()),
            ],
        };

        // Format and parse back
        let formatted = format_deltas_dat(&delta);
        let parsed = parse_deltas_dat(&formatted).unwrap();

        // Check that reference_id is preserved
        assert_eq!(parsed.reference_id, "ref_seq_1",
                   "reference_id should be preserved through format/parse cycle");
        assert_eq!(parsed.child_id, "child_seq_1");
        assert_eq!(parsed.deltas.len(), 2);
    }

    #[test]
    fn test_reconstruction_with_reference_id() {
        use crate::bio::sequence::Sequence;

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
        assert_eq!(parsed.reference_id, "ref_seq_1",
                   "reference_id must be preserved for reconstruction to work");

        // Reconstruct and verify
        let reconstructor = DeltaReconstructor::new();
        let reconstructed = reconstructor.reconstruct(&reference, &parsed);
        assert_eq!(reconstructed.id, "child_seq_1");
        assert_eq!(reconstructed.sequence, child.sequence);
    }
}