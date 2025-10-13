use crate::storage::HeraldStorage;
/// Discrepancy detection between taxonomy and sequence annotations
use crate::types::*;
use anyhow::Result;
use chrono::Utc;
use std::collections::{HashMap, HashSet};

pub struct DiscrepancyDetector {
    taxonomy_mappings: HashMap<String, TaxonId>,
}

impl Default for DiscrepancyDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl DiscrepancyDetector {
    pub fn new() -> Self {
        Self {
            taxonomy_mappings: HashMap::new(),
        }
    }

    /// Set taxonomy mappings (accession -> taxon ID)
    pub fn set_taxonomy_mappings(&mut self, mappings: HashMap<String, TaxonId>) {
        self.taxonomy_mappings = mappings;
    }

    /// Detect discrepancies from manifest and sequences (new approach)
    pub fn detect_from_manifest(
        &self,
        manifest: &ChunkManifest,
        sequences: Vec<(String, String)>, // (sequence_id, fasta_data)
    ) -> Vec<TaxonomicDiscrepancy> {
        let mut discrepancies = Vec::new();

        for (sequence_id, _fasta_data) in sequences {
            // Extract claimed taxon from sequence ID/header
            let header_taxon = self.extract_taxon_from_header(&sequence_id);

            // Look up in accession mapping
            let accession = self.extract_accession(&sequence_id);
            let mapped_taxon = self.taxonomy_mappings.get(&accession).cloned();

            // Infer from chunk context (using manifest's taxon_ids)
            let inferred_taxon = self.infer_taxon_from_chunk(&manifest.taxon_ids);

            // Check for discrepancies
            if let Some(discrepancy) = self.check_simple_discrepancy(
                &sequence_id,
                header_taxon,
                mapped_taxon,
                inferred_taxon,
            ) {
                discrepancies.push(discrepancy);
            }
        }

        discrepancies
    }

    /// Simplified discrepancy check without full taxonomy tree
    fn check_simple_discrepancy(
        &self,
        sequence_id: &str,
        header_taxon: Option<TaxonId>,
        mapped_taxon: Option<TaxonId>,
        inferred_taxon: Option<TaxonId>,
    ) -> Option<TaxonomicDiscrepancy> {
        // Determine discrepancy type
        let discrepancy_type = match (
            header_taxon.as_ref(),
            mapped_taxon.as_ref(),
            inferred_taxon.as_ref(),
        ) {
            (None, None, None) => Some(DiscrepancyType::Missing),
            (Some(h), Some(m), _) if h != m => Some(DiscrepancyType::Conflict),
            (Some(h), _, Some(i)) if h != i => Some(DiscrepancyType::Conflict),
            (_, Some(m), Some(i)) if m != i => Some(DiscrepancyType::Conflict),
            _ => None,
        };

        discrepancy_type.map(|dtype| {
            // Calculate confidence based on available sources
            let mut confidence = 0.0;
            let mut sources = 0;

            if header_taxon.is_some() {
                sources += 1;
                confidence += 0.33;
            }
            if mapped_taxon.is_some() {
                sources += 1;
                confidence += 0.33;
            }
            if inferred_taxon.is_some() {
                sources += 1;
                confidence += 0.34;
            }

            // Adjust confidence based on agreement
            if sources > 1 {
                let all_equal = header_taxon == mapped_taxon && mapped_taxon == inferred_taxon;
                if all_equal {
                    confidence = 1.0;
                } else {
                    confidence *= 0.5; // Lower confidence on disagreement
                }
            }

            TaxonomicDiscrepancy {
                sequence_id: sequence_id.to_string(),
                header_taxon,
                mapped_taxon,
                inferred_taxon,
                confidence,
                detection_date: Utc::now(),
                discrepancy_type: dtype,
            }
        })
    }

    /// Detect all discrepancies in the repository
    pub fn detect_all(&self, storage: &HeraldStorage) -> Result<Vec<TaxonomicDiscrepancy>> {
        let mut discrepancies = Vec::new();

        // Scan storage for taxonomy discrepancies
        let stats = storage.get_stats();
        tracing::info!(
            "Scanning {} chunks for taxonomy discrepancies...",
            stats.total_chunks
        );

        // Enumerate all chunks
        let chunk_infos = storage.enumerate_chunks();

        for chunk_info in chunk_infos {
            // Try to load as taxonomy-aware chunk
            match storage.get_chunk(&chunk_info.hash) {
                Ok(data) => {
                    // Try to deserialize as ChunkManifest
                    if let Ok(chunk) = serde_json::from_slice::<ChunkManifest>(&data) {
                        // Detect discrepancies in this chunk
                        match self.detect_in_chunk(&chunk, storage) {
                            Ok(mut chunk_discrepancies) => {
                                discrepancies.append(&mut chunk_discrepancies);
                            }
                            Err(e) => {
                                tracing::info!(
                                    "Error detecting discrepancies in chunk {}: {}",
                                    chunk_info.hash,
                                    e
                                );
                            }
                        }
                    }
                    // If not a ChunkManifest, skip it (could be delta chunk, etc.)
                }
                Err(e) => {
                    tracing::info!("Failed to read chunk {}: {}", chunk_info.hash, e);
                }
            }
        }

        tracing::info!("Found {} discrepancies", discrepancies.len());
        Ok(discrepancies)
    }

    /// Detect discrepancies in a specific chunk
    pub fn detect_in_chunk(
        &self,
        chunk: &ChunkManifest,
        storage: &HeraldStorage,
    ) -> Result<Vec<TaxonomicDiscrepancy>> {
        let mut discrepancies = Vec::new();

        // For ChunkManifest, we only have sequence hashes, not full metadata
        // We need to load representations to get accession info
        for seq_hash in &chunk.sequence_refs {
            // Try to load representations to get accession
            if let Ok(representations) = storage.sequence_storage.load_representations(seq_hash) {
                for repr in &representations.representations {
                    // Get first accession if available
                    if let Some(accession) = repr.accessions.first() {
                        // Look up in accession mapping
                        let mapped_taxon = self.taxonomy_mappings.get(accession).cloned();

                        // Check for discrepancies between repr taxon and chunk taxon
                        if let Some(repr_taxon) = repr.taxon_id {
                            // Check if the taxon in representation matches chunk taxons
                            if !chunk.taxon_ids.contains(&repr_taxon) && !chunk.taxon_ids.is_empty()
                            {
                                discrepancies.push(TaxonomicDiscrepancy {
                                    sequence_id: accession.clone(),
                                    header_taxon: Some(repr_taxon),
                                    mapped_taxon: mapped_taxon,
                                    inferred_taxon: chunk.taxon_ids.first().cloned(),
                                    confidence: 0.8,
                                    detection_date: chrono::Utc::now(),
                                    discrepancy_type: DiscrepancyType::Conflict,
                                });
                            }
                        }

                        // Check mapping vs representation taxon
                        if let Some(mapped) = mapped_taxon {
                            if let Some(repr_taxon) = repr.taxon_id {
                                if mapped != repr_taxon {
                                    discrepancies.push(TaxonomicDiscrepancy {
                                        sequence_id: accession.clone(),
                                        header_taxon: Some(repr_taxon),
                                        mapped_taxon: Some(mapped),
                                        inferred_taxon: None,
                                        confidence: 0.9,
                                        detection_date: chrono::Utc::now(),
                                        discrepancy_type: DiscrepancyType::Conflict,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(discrepancies)
    }

    /// Check for discrepancy between different taxon sources
    fn check_discrepancy(
        &self,
        sequence_id: &str,
        header_taxon: Option<TaxonId>,
        mapped_taxon: Option<TaxonId>,
        chunk_taxa: &[TaxonId],
    ) -> Option<TaxonomicDiscrepancy> {
        let discrepancy_type = match (header_taxon.as_ref(), mapped_taxon.as_ref()) {
            (None, None) => {
                // No taxonomy information at all
                Some(DiscrepancyType::Missing)
            }
            (Some(h), Some(m)) if h != m => {
                // Header and mapping disagree
                Some(DiscrepancyType::Conflict)
            }
            (Some(taxon), None) | (None, Some(taxon)) => {
                // Check if taxon is outdated (simple check without full taxonomy tree)
                if self.is_outdated_classification(taxon) {
                    Some(DiscrepancyType::Outdated)
                } else {
                    None
                }
            }
            _ => None,
        };

        discrepancy_type.map(|dtype| TaxonomicDiscrepancy {
            sequence_id: sequence_id.to_string(),
            header_taxon,
            mapped_taxon,
            inferred_taxon: self.infer_taxon_from_chunk(chunk_taxa),
            confidence: self.calculate_confidence(&header_taxon, &mapped_taxon),
            detection_date: Utc::now(),
            discrepancy_type: dtype,
        })
    }

    /// Parse FASTA header from chunk data
    fn parse_fasta_header(&self, data: &[u8], offset: usize) -> Result<String> {
        let start = offset;
        let mut end = offset;

        // Find end of header line
        while end < data.len() && data[end] != b'\n' {
            end += 1;
        }

        Ok(String::from_utf8_lossy(&data[start..end]).to_string())
    }

    /// Extract taxon ID from header
    fn extract_taxon_from_header(&self, header: &str) -> Option<TaxonId> {
        // Look for common patterns
        // OX=12345 (UniProt)
        if let Some(ox_pos) = header.find("OX=") {
            let start = ox_pos + 3;
            let end = header[start..]
                .find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(header.len());

            if let Ok(taxon_id) = header[start..end].parse::<u32>() {
                return Some(TaxonId(taxon_id));
            }
        }

        // TaxID=12345
        if let Some(tax_pos) = header.find("TaxID=") {
            let start = tax_pos + 6;
            let end = header[start..]
                .find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(header.len());

            if let Ok(taxon_id) = header[start..end].parse::<u32>() {
                return Some(TaxonId(taxon_id));
            }
        }

        None
    }

    /// Extract accession from sequence ID
    fn extract_accession(&self, id: &str) -> String {
        // Handle common formats
        if id.contains('|') {
            let parts: Vec<&str> = id.split('|').collect();
            if parts.len() >= 2 {
                // UniProt format: sp|P12345|NAME
                if parts[0] == "sp" || parts[0] == "tr" {
                    return parts[1].to_string();
                }
                // NCBI format: gi|123|ref|NP_12345.1|
                if parts.len() >= 4 && parts[2] == "ref" {
                    return parts[3].split('.').next().unwrap_or(parts[3]).to_string();
                }
            }
        }

        // Simple accession
        id.split('.').next().unwrap_or(id).to_string()
    }

    /// Check if a classification is outdated
    fn is_outdated_classification(&self, taxon_id: &TaxonId) -> bool {
        // Check against known reclassifications
        // This would be populated from taxonomy version history

        // Common outdated classifications
        match taxon_id.0 {
            // Example: Old Lactobacillus species that have been reclassified
            1578..=1680 => true, // Many old Lactobacillus IDs
            _ => false,
        }
    }

    /// Infer taxon from chunk context
    fn infer_taxon_from_chunk(&self, chunk_taxa: &[TaxonId]) -> Option<TaxonId> {
        // If chunk has a single taxon, that's likely correct
        if chunk_taxa.len() == 1 {
            Some(chunk_taxa[0])
        } else {
            None
        }
    }

    /// Calculate confidence in the discrepancy detection
    fn calculate_confidence(&self, header: &Option<TaxonId>, mapped: &Option<TaxonId>) -> f32 {
        match (header, mapped) {
            (Some(_), Some(_)) => 0.9,                // Both sources present
            (Some(_), None) | (None, Some(_)) => 0.6, // One source
            (None, None) => 0.3,                      // No sources
        }
    }

    /// Analyze discrepancies to find patterns
    pub fn analyze_discrepancies(
        &self,
        discrepancies: &[TaxonomicDiscrepancy],
    ) -> DiscrepancyAnalysis {
        let total = discrepancies.len();

        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut affected_taxa: HashSet<TaxonId> = HashSet::new();
        let mut common_conflicts: HashMap<(TaxonId, TaxonId), usize> = HashMap::new();

        for disc in discrepancies {
            // Count by type
            let type_name = format!("{:?}", disc.discrepancy_type);
            *by_type.entry(type_name).or_default() += 1;

            // Collect affected taxa
            if let Some(ref taxon) = disc.header_taxon {
                affected_taxa.insert(*taxon);
            }
            if let Some(ref taxon) = disc.mapped_taxon {
                affected_taxa.insert(*taxon);
            }

            // Track common conflicts
            if let (Some(ref h), Some(ref m)) = (&disc.header_taxon, &disc.mapped_taxon) {
                if h != m {
                    let key = if h.0 < m.0 { (*h, *m) } else { (*m, *h) };
                    *common_conflicts.entry(key).or_default() += 1;
                }
            }
        }

        // Find most common conflicts
        let mut conflicts_vec: Vec<_> = common_conflicts.into_iter().collect();
        conflicts_vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        DiscrepancyAnalysis {
            total_discrepancies: total,
            by_type,
            affected_taxa_count: affected_taxa.len(),
            most_common_conflicts: conflicts_vec.into_iter().take(10).collect(),
            missing_taxonomy_count: discrepancies
                .iter()
                .filter(|d| matches!(d.discrepancy_type, DiscrepancyType::Missing))
                .count(),
            outdated_count: discrepancies
                .iter()
                .filter(|d| matches!(d.discrepancy_type, DiscrepancyType::Outdated))
                .count(),
        }
    }

    /// Generate report of discrepancies
    pub fn generate_report(&self, discrepancies: &[TaxonomicDiscrepancy]) -> String {
        let analysis = self.analyze_discrepancies(discrepancies);

        let mut report = String::new();
        report.push_str("# Taxonomy Discrepancy Report\n\n");
        report.push_str(&format!(
            "Total discrepancies found: {}\n\n",
            analysis.total_discrepancies
        ));

        report.push_str("## Summary by Type\n");
        for (dtype, count) in &analysis.by_type {
            report.push_str(&format!("- {}: {}\n", dtype, count));
        }
        report.push('\n');

        report.push_str("## Statistics\n");
        report.push_str(&format!(
            "- Affected taxa: {}\n",
            analysis.affected_taxa_count
        ));
        report.push_str(&format!(
            "- Missing taxonomy: {}\n",
            analysis.missing_taxonomy_count
        ));
        report.push_str(&format!(
            "- Outdated classifications: {}\n",
            analysis.outdated_count
        ));
        report.push('\n');

        if !analysis.most_common_conflicts.is_empty() {
            report.push_str("## Most Common Conflicts\n");
            for ((taxon1, taxon2), count) in &analysis.most_common_conflicts {
                report.push_str(&format!(
                    "- {} vs {} ({} occurrences)\n",
                    taxon1, taxon2, count
                ));
            }
        }

        report
    }
}

#[derive(Debug)]
pub struct DiscrepancyAnalysis {
    pub total_discrepancies: usize,
    pub by_type: HashMap<String, usize>,
    pub affected_taxa_count: usize,
    pub most_common_conflicts: Vec<((TaxonId, TaxonId), usize)>,
    pub missing_taxonomy_count: usize,
    pub outdated_count: usize,
}
