use super::traits::{Chunker, ChunkingStats};
use crate::bio::sequence::Sequence;
use crate::bio::taxonomy::{TaxonomyDiscrepancy, TaxonomyEnrichable, TaxonomyResolver};
/// Smart chunking system for taxonomic groups
use crate::casg::types::*;
use crate::utils::progress::{create_progress_bar, create_spinner};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub struct TaxonomicChunker {
    strategy: ChunkingStrategy,
    taxonomy_map: HashMap<String, TaxonId>, // Accession -> TaxonId
}

impl TaxonomicChunker {
    pub fn new(strategy: ChunkingStrategy) -> Self {
        Self {
            strategy,
            taxonomy_map: HashMap::new(),
        }
    }

    /// Load taxonomy mapping from accession2taxid file
    pub fn load_taxonomy_mapping(&mut self, mapping: HashMap<String, TaxonId>) {
        self.taxonomy_map = mapping;
    }

    /// Chunk sequences with validation using trait-based taxonomy resolution
    pub fn chunk_with_validation(
        &mut self,
        mut sequences: Vec<Sequence>,
    ) -> Result<Vec<TaxonomyAwareChunk>> {
        // Enrich all sequences with taxonomy from mappings
        let mappings: HashMap<String, u32> = self
            .taxonomy_map
            .iter()
            .map(|(k, v)| (k.clone(), v.0))
            .collect();

        let enrichment_progress =
            create_progress_bar(sequences.len() as u64, "Enriching sequences with taxonomy");
        for seq in &mut sequences {
            // Enrich from various sources
            seq.enrich_from_mappings(&mappings);
            seq.enrich_from_header();
            enrichment_progress.inc(1);
        }
        enrichment_progress.finish_and_clear();

        // Detect and report discrepancies
        let mut all_discrepancies = Vec::new();
        for seq in &sequences {
            let discrepancies = seq.detect_discrepancies();
            all_discrepancies.extend(discrepancies);
        }

        if !all_discrepancies.is_empty() {
            self.report_discrepancies(&all_discrepancies);
        }

        // Group sequences by resolved taxonomy
        let grouping_progress = create_progress_bar(
            sequences.len() as u64,
            "Grouping sequences by resolved taxonomy",
        );
        let mut taxon_groups: HashMap<TaxonId, Vec<Sequence>> = HashMap::new();

        for seq in sequences {
            let resolution = seq.resolve_taxonomy();
            let taxon_id = TaxonId(resolution.get_primary_taxon());

            // Log if there was a conflict
            if resolution.has_conflicts() {
                tracing::debug!(
                    "Taxonomy conflict for {}: resolved to {}",
                    seq.id,
                    taxon_id.0
                );
            }

            taxon_groups.entry(taxon_id).or_default().push(seq);
            grouping_progress.inc(1);
        }
        grouping_progress.finish_and_clear();
        println!("Grouped into {} taxa", taxon_groups.len());

        // Apply chunking strategy to each group
        let chunking_progress = create_progress_bar(
            taxon_groups.len() as u64,
            "Creating chunks from taxa groups",
        );
        let mut chunks = Vec::new();

        for (taxon_id, sequences) in taxon_groups {
            let group_chunks = self.chunk_taxon_group(taxon_id, sequences)?;
            chunks.extend(group_chunks);
            chunking_progress.inc(1);
        }
        chunking_progress.finish_and_clear();
        println!("Created {} chunks", chunks.len());

        // Apply special handling for important taxa
        let special_progress = create_spinner("Applying special taxa rules");
        chunks = self.apply_special_taxa_rules(chunks)?;
        special_progress.finish_and_clear();
        println!("Special taxa rules applied");

        Ok(chunks)
    }

    /// Report taxonomy discrepancies
    fn report_discrepancies(&self, discrepancies: &[TaxonomyDiscrepancy]) {
        use crate::cli::output::*;

        if discrepancies.is_empty() {
            return;
        }

        warning(&format!(
            "Found {} taxonomy discrepancies",
            discrepancies.len()
        ));

        // Group by conflict pattern
        let mut by_pattern: HashMap<String, Vec<&TaxonomyDiscrepancy>> = HashMap::new();
        for disc in discrepancies {
            let pattern = format!(
                "{:?}",
                disc.conflicts.iter().map(|(s, _)| s).collect::<Vec<_>>()
            );
            by_pattern.entry(pattern).or_default().push(disc);
        }

        for (pattern, discs) in by_pattern.iter().take(5) {
            println!("  Conflict pattern {}: {} sequences", pattern, discs.len());
            for disc in discs.iter().take(3) {
                println!(
                    "    - {} (resolved by {})",
                    disc.sequence_id, disc.resolution_strategy
                );
            }
            if discs.len() > 3 {
                println!("    ... and {} more", discs.len() - 3);
            }
        }

        if by_pattern.len() > 5 {
            println!("  ... and {} more conflict patterns", by_pattern.len() - 5);
        }
    }

    /// Chunk sequences by taxonomic groups
    pub fn chunk_sequences_into_taxonomy_aware(
        &self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<TaxonomyAwareChunk>> {
        // Group sequences by taxon ID
        let grouping_progress =
            create_progress_bar(sequences.len() as u64, "Grouping sequences by taxonomy");
        let mut taxon_groups: HashMap<TaxonId, Vec<Sequence>> = HashMap::new();

        for seq in sequences {
            let taxon_id = self.get_taxon_id(&seq)?;
            taxon_groups.entry(taxon_id).or_default().push(seq);
            grouping_progress.inc(1);
        }
        grouping_progress.finish_and_clear();
        println!("Grouped into {} taxa", taxon_groups.len());

        // Apply chunking strategy to each group
        let chunking_progress = create_progress_bar(
            taxon_groups.len() as u64,
            "Creating chunks from taxa groups",
        );
        let mut chunks = Vec::new();

        for (taxon_id, sequences) in taxon_groups {
            let group_chunks = self.chunk_taxon_group(taxon_id, sequences)?;
            chunks.extend(group_chunks);
            chunking_progress.inc(1);
        }
        chunking_progress.finish_and_clear();
        println!("Created {} chunks", chunks.len());

        // Apply special handling for important taxa
        let special_progress = create_spinner("Applying special taxa rules");
        chunks = self.apply_special_taxa_rules(chunks)?;
        special_progress.finish_and_clear();
        println!("Special taxa rules applied");

        Ok(chunks)
    }

    /// Get taxon ID for a sequence
    fn get_taxon_id(&self, sequence: &Sequence) -> Result<TaxonId> {
        // First, try to get from the sequence's taxon_id field
        if let Some(taxon_id) = sequence.taxon_id {
            return Ok(TaxonId(taxon_id));
        }

        // Try to extract accession from sequence ID
        let accession = self.extract_accession(&sequence.id);

        // Look up in taxonomy map
        if let Some(taxon_id) = self.taxonomy_map.get(&accession) {
            return Ok(*taxon_id);
        }

        // Try to parse from description (common formats)
        if let Some(taxon_id) = self.parse_taxon_from_description(&sequence.description) {
            return Ok(taxon_id);
        }

        // Default to unclassified
        Ok(TaxonId(0))
    }

    /// Extract accession from sequence ID
    fn extract_accession(&self, id: &str) -> String {
        // Handle common formats:
        // >sp|P12345|NAME_ORGANISM
        // >gi|123456|ref|NP_123456.1|
        // >NP_123456.1

        if id.contains('|') {
            let parts: Vec<&str> = id.split('|').collect();
            if parts.len() >= 2 {
                // UniProt format
                if parts[0] == "sp" || parts[0] == "tr" {
                    return parts[1].to_string();
                }
                // NCBI format
                if parts.len() >= 4 && parts[2] == "ref" {
                    return parts[3].split('.').next().unwrap_or(parts[3]).to_string();
                }
            }
        }

        // Simple accession
        id.split('.').next().unwrap_or(id).to_string()
    }

    /// Parse taxon ID from description
    fn parse_taxon_from_description(&self, description: &Option<String>) -> Option<TaxonId> {
        let desc = description.as_ref()?;
        // Look for patterns like:
        // "OX=9606" (UniProt)
        // "TaxID=9606"
        // "[Homo sapiens]"

        // UniProt OX= pattern
        if let Some(ox_pos) = desc.find("OX=") {
            let start = ox_pos + 3;
            let end = desc[start..]
                .find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(desc.len());

            if let Ok(taxon_id) = desc[start..end].parse::<u32>() {
                return Some(TaxonId(taxon_id));
            }
        }

        // TaxID= pattern
        if let Some(tax_pos) = desc.find("TaxID=") {
            let start = tax_pos + 6;
            let end = desc[start..]
                .find(|c: char| !c.is_numeric())
                .map(|i| start + i)
                .unwrap_or(desc.len());

            if let Ok(taxon_id) = desc[start..end].parse::<u32>() {
                return Some(TaxonId(taxon_id));
            }
        }

        None
    }

    /// Chunk a group of sequences with the same taxon
    fn chunk_taxon_group(
        &self,
        taxon_id: TaxonId,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<TaxonomyAwareChunk>> {
        let mut chunks = Vec::new();
        let mut current_chunk_sequences = Vec::new();
        let mut current_size = 0;

        for seq in sequences {
            let seq_size = seq.sequence.len();

            // Check if adding this sequence would exceed limits
            if current_size + seq_size > self.strategy.max_chunk_size
                || (current_size > self.strategy.target_chunk_size
                    && current_chunk_sequences.len() >= self.strategy.min_sequences_per_chunk)
            {
                // Create chunk
                if !current_chunk_sequences.is_empty() {
                    chunks
                        .push(self.create_chunk(vec![taxon_id], current_chunk_sequences)?);
                }

                // Start new chunk
                current_chunk_sequences = vec![seq];
                current_size = seq_size;
            } else {
                current_chunk_sequences.push(seq);
                current_size += seq_size;
            }
        }

        // Create final chunk
        if !current_chunk_sequences.is_empty() {
            chunks.push(self.create_chunk(vec![taxon_id], current_chunk_sequences)?);
        }

        Ok(chunks)
    }

    /// Create a taxonomy-aware chunk
    fn create_chunk(
        &self,
        taxon_ids: Vec<TaxonId>,
        sequences: Vec<Sequence>,
    ) -> Result<TaxonomyAwareChunk> {
        // Serialize sequences
        let mut chunk_data = Vec::new();
        let mut sequence_refs = Vec::new();

        for seq in sequences {
            let seq_start = chunk_data.len();
            let seq_bytes = self.serialize_sequence(&seq)?;
            chunk_data.extend(&seq_bytes);

            sequence_refs.push(SequenceRef {
                chunk_hash: SHA256Hash::compute(&seq_bytes),
                offset: seq_start,
                length: seq_bytes.len(),
                sequence_id: seq.id.clone(),
            });
        }

        let content_hash = SHA256Hash::compute(&chunk_data);
        let now = chrono::Utc::now();
        let chunk_size = chunk_data.len();

        // Get actual version hashes
        use crate::core::paths;

        let taxonomy_version = if let Ok(tax_mgr) =
            crate::casg::taxonomy::TaxonomyManager::new(&paths::talaria_databases_dir())
        {
            if tax_mgr.has_taxonomy() {
                tax_mgr
                    .get_taxonomy_root()
                    .unwrap_or_else(|_| SHA256Hash::compute(b"v1"))
            } else {
                SHA256Hash::compute(b"no_taxonomy")
            }
        } else {
            SHA256Hash::compute(b"no_taxonomy")
        };

        // For sequence version, use content hash of the chunk data as a proxy
        // In a full implementation, this would come from the manifest's sequence root
        let sequence_version = SHA256Hash::compute(format!("seq_v_{}", now.timestamp()).as_bytes());

        Ok(TaxonomyAwareChunk {
            content_hash,
            taxonomy_version,
            sequence_version,
            taxon_ids,
            sequences: sequence_refs,
            sequence_data: chunk_data, // Store the actual sequence data
            created_at: now,
            valid_from: now,
            valid_until: None,
            size: chunk_size,
            compressed_size: None, // Will be set when storing
        })
    }

    /// Serialize a sequence to bytes
    fn serialize_sequence(&self, seq: &Sequence) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        // Write FASTA format
        result.extend(b">");
        result.extend(seq.id.as_bytes());

        if let Some(ref desc) = seq.description {
            if !desc.is_empty() {
                result.extend(b" ");
                result.extend(desc.as_bytes());
            }
        }

        // Add taxon ID if available
        if let Some(taxon_id) = seq.taxon_id {
            result.extend(format!(" TaxID={}", taxon_id).as_bytes());
        }

        result.extend(b"\n");

        // Write sequence in lines of 80 characters
        for chunk in seq.sequence.chunks(80) {
            result.extend(chunk);
            result.extend(b"\n");
        }

        Ok(result)
    }

    /// Apply special handling rules for important taxa
    fn apply_special_taxa_rules(
        &self,
        chunks: Vec<TaxonomyAwareChunk>,
    ) -> Result<Vec<TaxonomyAwareChunk>> {
        let mut final_chunks = Vec::new();
        let mut special_taxa_chunks: HashMap<TaxonId, Vec<TaxonomyAwareChunk>> = HashMap::new();

        // Separate special taxa
        for chunk in chunks {
            let mut is_special = false;

            for special_taxon in &self.strategy.special_taxa {
                if chunk.taxon_ids.contains(&special_taxon.taxon_id) {
                    special_taxa_chunks
                        .entry(special_taxon.taxon_id)
                        .or_default()
                        .push(chunk.clone());
                    is_special = true;
                    break;
                }
            }

            if !is_special {
                final_chunks.push(chunk);
            }
        }

        // Process special taxa according to their strategies
        for (taxon_id, chunks) in special_taxa_chunks {
            let special_taxon = self
                .strategy
                .special_taxa
                .iter()
                .find(|st| st.taxon_id == taxon_id)
                .unwrap();

            match special_taxon.strategy {
                ChunkStrategy::OwnChunks => {
                    // Keep as separate chunks
                    final_chunks.extend(chunks);
                }
                ChunkStrategy::GroupWithSiblings => {
                    // Group with sibling taxa (same parent)
                    use crate::core::paths;
                    let taxonomy = crate::casg::taxonomy::TaxonomyManager::new(
                        &paths::talaria_databases_dir(),
                    )
                    .ok();

                    if let Some(_tax) = taxonomy {
                        // Find parent taxon
                        // Simplified parent lookup - would need actual implementation
                        let _parent_id = TaxonId(1);
                        if true {
                            // Find all siblings (taxa with same parent)
                            let siblings = chunks
                                .iter()
                                .filter(|c| {
                                    c.taxon_ids.iter().any(|&tid| {
                                        // Simplified check - would need actual parent lookup
                                        tid.0 > 0
                                    })
                                })
                                .collect::<Vec<_>>();

                            if siblings.len() > 1 {
                                // Merge sibling chunks
                                let merged = self.merge_chunks(&siblings)?;
                                final_chunks.push(merged);
                            } else {
                                final_chunks.extend(chunks);
                            }
                        } else {
                            final_chunks.extend(chunks);
                        }
                    } else {
                        // No taxonomy available, keep as is
                        final_chunks.extend(chunks);
                    }
                }
                ChunkStrategy::GroupAtLevel(level) => {
                    // Group sequences at taxonomic level (e.g., genus, family)
                    eprintln!("Grouping at taxonomic level: {}", level);

                    use crate::core::paths;
                    let taxonomy = crate::casg::taxonomy::TaxonomyManager::new(
                        &paths::talaria_databases_dir(),
                    )
                    .ok();

                    if let Some(_tax) = taxonomy {
                        // Group chunks by ancestor at specified level
                        let mut level_groups: HashMap<TaxonId, Vec<TaxonomyAwareChunk>> =
                            HashMap::new();

                        for chunk in chunks {
                            // Find the ancestor at the specified level for this chunk's primary taxon
                            if let Some(&primary_taxon) = chunk.taxon_ids.first() {
                                // Simplified ancestor lookup - would need actual implementation
                                let ancestor = TaxonId(primary_taxon.0 / 100); // Placeholder logic
                                if true {
                                    level_groups
                                        .entry(ancestor)
                                        .or_default()
                                        .push(chunk);
                                } else {
                                    // No ancestor at this level, keep as separate chunk
                                    final_chunks.push(chunk);
                                }
                            } else {
                                final_chunks.push(chunk);
                            }
                        }

                        // Merge chunks in each group
                        for (ancestor_id, group_chunks) in level_groups {
                            if group_chunks.len() > 1 {
                                eprintln!(
                                    "  Merging {} chunks for ancestor taxon {}",
                                    group_chunks.len(),
                                    ancestor_id
                                );
                                let merged =
                                    self.merge_chunks(&group_chunks.iter().collect::<Vec<_>>())?;
                                final_chunks.push(merged);
                            } else {
                                final_chunks.extend(group_chunks);
                            }
                        }
                    } else {
                        // No taxonomy available, keep as is
                        final_chunks.extend(chunks);
                    }
                }
            }
        }

        Ok(final_chunks)
    }

    /// Merge multiple chunks into a single chunk
    fn merge_chunks(&self, chunks: &[&TaxonomyAwareChunk]) -> Result<TaxonomyAwareChunk> {
        let mut all_sequences = Vec::new();
        let mut all_taxon_ids = HashSet::new();
        let mut combined_data = Vec::new();
        let mut all_sequence_data = Vec::new();

        for chunk in chunks {
            all_sequences.extend(chunk.sequences.clone());
            all_taxon_ids.extend(chunk.taxon_ids.iter().cloned());

            // Combine sequence data
            all_sequence_data.extend_from_slice(&chunk.sequence_data);

            // Combine raw data for hash computation
            combined_data.extend_from_slice(&chunk.content_hash.0);
        }

        // Compute new content hash from the combined sequence data
        let content_hash = SHA256Hash::compute(&all_sequence_data);

        // Get version hashes from first chunk (all should have same versions in this context)
        let taxonomy_version = chunks
            .first()
            .map(|c| c.taxonomy_version.clone())
            .unwrap_or_else(|| SHA256Hash::compute(b"v1"));
        let sequence_version = chunks
            .first()
            .map(|c| c.sequence_version.clone())
            .unwrap_or_else(|| SHA256Hash::compute(b"v1"));

        Ok(TaxonomyAwareChunk {
            content_hash,
            taxonomy_version,
            sequence_version,
            taxon_ids: all_taxon_ids.into_iter().collect(),
            sequences: all_sequences,
            sequence_data: all_sequence_data,
            created_at: chrono::Utc::now(),
            valid_from: chrono::Utc::now(),
            valid_until: None,
            size: chunks.iter().map(|c| c.size).sum(),
            compressed_size: chunks.first().and_then(|c| c.compressed_size),
        })
    }

    /// Analyze sequences to detect optimal chunking strategy
    pub fn analyze_for_strategy(&self, sequences: &[Sequence]) -> ChunkingAnalysis {
        let mut taxon_counts: HashMap<TaxonId, usize> = HashMap::new();
        let mut total_size = 0;

        for seq in sequences {
            if let Ok(taxon_id) = self.get_taxon_id(seq) {
                *taxon_counts.entry(taxon_id).or_default() += 1;
            }
            total_size += seq.sequence.len();
        }

        // Find taxa with high sequence counts
        let mut high_volume_taxa: Vec<(TaxonId, usize)> = taxon_counts
            .iter()
            .filter(|(_, count)| **count > 100)
            .map(|(id, count)| (id.clone(), *count))
            .collect();

        high_volume_taxa.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        ChunkingAnalysis {
            total_sequences: sequences.len(),
            total_size,
            unique_taxa: taxon_counts.len(),
            high_volume_taxa: high_volume_taxa.into_iter().take(10).collect(),
            suggested_special_taxa: self.suggest_special_taxa(&taxon_counts),
        }
    }

    fn suggest_special_taxa(&self, taxon_counts: &HashMap<TaxonId, usize>) -> Vec<SpecialTaxon> {
        let mut suggestions = Vec::new();

        // E. coli (562)
        if taxon_counts.get(&TaxonId(562)).copied().unwrap_or(0) > 50 {
            suggestions.push(SpecialTaxon {
                taxon_id: TaxonId(562),
                name: "Escherichia coli".to_string(),
                strategy: ChunkStrategy::OwnChunks,
            });
        }

        // Human (9606)
        if taxon_counts.get(&TaxonId(9606)).copied().unwrap_or(0) > 50 {
            suggestions.push(SpecialTaxon {
                taxon_id: TaxonId(9606),
                name: "Homo sapiens".to_string(),
                strategy: ChunkStrategy::OwnChunks,
            });
        }

        // Mouse (10090)
        if taxon_counts.get(&TaxonId(10090)).copied().unwrap_or(0) > 50 {
            suggestions.push(SpecialTaxon {
                taxon_id: TaxonId(10090),
                name: "Mus musculus".to_string(),
                strategy: ChunkStrategy::OwnChunks,
            });
        }

        suggestions
    }
}

#[derive(Debug)]
pub struct ChunkingAnalysis {
    pub total_sequences: usize,
    pub total_size: usize,
    pub unique_taxa: usize,
    pub high_volume_taxa: Vec<(TaxonId, usize)>,
    pub suggested_special_taxa: Vec<SpecialTaxon>,
}

impl Default for ChunkingStrategy {
    fn default() -> Self {
        Self {
            target_chunk_size: 50 * 1024 * 1024, // 50 MB
            max_chunk_size: 100 * 1024 * 1024,   // 100 MB
            min_sequences_per_chunk: 10,
            taxonomic_coherence: 0.9,
            special_taxa: vec![
                SpecialTaxon {
                    taxon_id: TaxonId(562), // E. coli
                    name: "Escherichia coli".to_string(),
                    strategy: ChunkStrategy::OwnChunks,
                },
                SpecialTaxon {
                    taxon_id: TaxonId(9606), // Human
                    name: "Homo sapiens".to_string(),
                    strategy: ChunkStrategy::OwnChunks,
                },
            ],
        }
    }
}

// Implement the Chunker trait for TaxonomicChunker
impl Chunker for TaxonomicChunker {
    fn chunk_sequences(&mut self, sequences: &[Sequence]) -> Result<Vec<ChunkMetadata>> {
        // Call the existing chunk_sequences method and convert the result
        let chunks = self.chunk_sequences_into_taxonomy_aware(sequences.to_vec())?;

        // Convert TaxonomyAwareChunk to ChunkMetadata
        Ok(chunks
            .into_iter()
            .map(|chunk| ChunkMetadata {
                hash: chunk.content_hash,
                taxon_ids: chunk.taxon_ids,
                sequence_count: chunk.sequences.len(),
                size: chunk.size,
                compressed_size: chunk.compressed_size,
            })
            .collect())
    }

    fn get_stats(&self) -> ChunkingStats {
        ChunkingStats {
            total_chunks: 0,
            total_sequences: 0,
            avg_chunk_size: 0,
            compression_ratio: 1.0,
        }
    }

    fn set_chunk_size(&mut self, _min_size: usize, _max_size: usize) {
        // Would update internal configuration
    }
}

// Additional helper methods for TaxonomicChunker
// (These are already defined in the main impl block above)
