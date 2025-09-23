use talaria_bio::sequence::Sequence;
/// Hierarchical taxonomic chunking as specified in SEQUOIA architecture
/// Implements: Kingdom → Phylum → Class → Order → Family → Genus → Species
/// With adaptive chunk sizes based on organism importance
use crate::types::*;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Taxonomic hierarchy levels for chunking
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaxonomicLevel {
    Kingdom = 0,
    Phylum = 1,
    Class = 2,
    Order = 3,
    Family = 4,
    Genus = 5,
    Species = 6,
}

impl TaxonomicLevel {
    pub fn name(&self) -> &str {
        match self {
            Self::Kingdom => "kingdom",
            Self::Phylum => "phylum",
            Self::Class => "class",
            Self::Order => "order",
            Self::Family => "family",
            Self::Genus => "genus",
            Self::Species => "species",
        }
    }

    pub fn from_rank(rank: &str) -> Option<Self> {
        match rank.to_lowercase().as_str() {
            "kingdom" | "superkingdom" => Some(Self::Kingdom),
            "phylum" => Some(Self::Phylum),
            "class" => Some(Self::Class),
            "order" => Some(Self::Order),
            "family" => Some(Self::Family),
            "genus" => Some(Self::Genus),
            "species" => Some(Self::Species),
            _ => None,
        }
    }
}

/// Organism importance category for adaptive chunk sizing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganismImportance {
    ModelOrganism, // 50-200MB chunks
    Pathogen,      // 100-500MB chunks
    Environmental, // 500MB-1GB chunks
}

impl OrganismImportance {
    pub fn chunk_size_range(&self) -> (usize, usize) {
        match self {
            Self::ModelOrganism => (50 * 1024 * 1024, 200 * 1024 * 1024),
            Self::Pathogen => (100 * 1024 * 1024, 500 * 1024 * 1024),
            Self::Environmental => (500 * 1024 * 1024, 1024 * 1024 * 1024),
        }
    }
}

/// Hierarchical taxonomic chunker implementing SEQUOIA 5-dimensional approach
pub struct HierarchicalTaxonomicChunker {
    /// Map of taxon IDs to their importance category
    importance_map: HashMap<TaxonId, OrganismImportance>,
    /// Map of taxon IDs to their taxonomic level
    level_map: HashMap<TaxonId, TaxonomicLevel>,
    /// Taxonomy tree for hierarchy navigation
    taxonomy_tree: Option<TaxonomyTree>,
}

/// Simplified taxonomy tree structure
pub struct TaxonomyTree {
    nodes: HashMap<TaxonId, TaxonomyNode>,
}

#[allow(dead_code)]
struct TaxonomyNode {
    taxon_id: TaxonId,
    name: String,
    rank: TaxonomicLevel,
    parent: Option<TaxonId>,
    children: Vec<TaxonId>,
}

impl Default for HierarchicalTaxonomicChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl HierarchicalTaxonomicChunker {
    pub fn new() -> Self {
        let mut importance_map = HashMap::new();

        // Model organisms
        importance_map.insert(TaxonId(9606), OrganismImportance::ModelOrganism); // Human
        importance_map.insert(TaxonId(10090), OrganismImportance::ModelOrganism); // Mouse
        importance_map.insert(TaxonId(7955), OrganismImportance::ModelOrganism); // Zebrafish
        importance_map.insert(TaxonId(7227), OrganismImportance::ModelOrganism); // Fruit fly
        importance_map.insert(TaxonId(6239), OrganismImportance::ModelOrganism); // C. elegans
        importance_map.insert(TaxonId(559292), OrganismImportance::ModelOrganism); // S. cerevisiae
        importance_map.insert(TaxonId(562), OrganismImportance::ModelOrganism); // E. coli

        // Pathogens
        importance_map.insert(TaxonId(1773), OrganismImportance::Pathogen); // Mycobacterium tuberculosis
        importance_map.insert(TaxonId(11103), OrganismImportance::Pathogen); // Hepatitis C virus
        importance_map.insert(TaxonId(11676), OrganismImportance::Pathogen); // HIV-1
        importance_map.insert(TaxonId(1280), OrganismImportance::Pathogen); // Staphylococcus aureus
        importance_map.insert(TaxonId(5476), OrganismImportance::Pathogen); // Candida albicans
        importance_map.insert(TaxonId(5661), OrganismImportance::Pathogen); // Leishmania donovani
        importance_map.insert(TaxonId(5833), OrganismImportance::Pathogen); // Plasmodium falciparum

        Self {
            importance_map,
            level_map: HashMap::new(),
            taxonomy_tree: None,
        }
    }

    /// Load taxonomy tree from database
    pub fn load_taxonomy_tree(&mut self, tree: TaxonomyTree) {
        // Build level map from tree
        for (taxon_id, node) in &tree.nodes {
            self.level_map.insert(*taxon_id, node.rank);
        }
        self.taxonomy_tree = Some(tree);
    }

    /// Perform hierarchical chunking of sequences
    pub fn chunk_hierarchically(&self, sequences: Vec<Sequence>) -> Result<Vec<HierarchicalChunk>> {
        println!("Performing hierarchical taxonomic chunking (SEQUOIA 5-dimensional)");

        // Step 1: Group sequences by taxonomic hierarchy
        let hierarchy = self.build_hierarchy(&sequences)?;

        // Step 2: Apply adaptive chunking based on importance
        let chunks = self.apply_adaptive_chunking(hierarchy)?;

        println!("Created {} hierarchical chunks", chunks.len());

        Ok(chunks)
    }

    /// Build hierarchical grouping of sequences
    fn build_hierarchy(&self, sequences: &[Sequence]) -> Result<TaxonomicHierarchy> {
        let mut hierarchy = TaxonomicHierarchy::new();

        for seq in sequences {
            let taxon_id = self.get_taxon_id(seq)?;

            // Get the full lineage for this taxon
            let lineage = self.get_lineage(taxon_id)?;

            // Add sequence to appropriate levels in hierarchy
            hierarchy.add_sequence(seq.clone(), lineage);
        }

        Ok(hierarchy)
    }

    /// Get taxonomic lineage for a taxon ID
    fn get_lineage(&self, taxon_id: TaxonId) -> Result<Vec<(TaxonomicLevel, TaxonId)>> {
        let mut lineage = Vec::new();

        if let Some(tree) = &self.taxonomy_tree {
            let mut current = Some(taxon_id);

            while let Some(tid) = current {
                if let Some(node) = tree.nodes.get(&tid) {
                    lineage.push((node.rank, tid));
                    current = node.parent;
                } else {
                    break;
                }
            }

            lineage.reverse(); // Start from kingdom down to species
        } else {
            // Fallback: use simplified hierarchy
            lineage.push((TaxonomicLevel::Species, taxon_id));
        }

        Ok(lineage)
    }

    /// Apply adaptive chunk sizing based on organism importance
    fn apply_adaptive_chunking(
        &self,
        hierarchy: TaxonomicHierarchy,
    ) -> Result<Vec<HierarchicalChunk>> {
        let mut chunks = Vec::new();

        // Process each taxonomic level
        for level in [
            TaxonomicLevel::Kingdom,
            TaxonomicLevel::Phylum,
            TaxonomicLevel::Class,
            TaxonomicLevel::Order,
            TaxonomicLevel::Family,
            TaxonomicLevel::Genus,
            TaxonomicLevel::Species,
        ] {
            let level_chunks = self.chunk_at_level(&hierarchy, level)?;
            chunks.extend(level_chunks);
        }

        Ok(chunks)
    }

    /// Create chunks at a specific taxonomic level
    fn chunk_at_level(
        &self,
        hierarchy: &TaxonomicHierarchy,
        level: TaxonomicLevel,
    ) -> Result<Vec<HierarchicalChunk>> {
        let mut chunks = Vec::new();

        if let Some(taxa_at_level) = hierarchy.get_taxa_at_level(level) {
            for taxon_id in &taxa_at_level {
                let sequences = hierarchy.get_sequences_for_taxon(*taxon_id, level);

                if !sequences.is_empty() {
                    // Determine chunk size based on importance
                    let importance = self.get_importance(*taxon_id);
                    let (min_size, max_size) = importance.chunk_size_range();

                    // Create chunks respecting size limits
                    let taxon_chunks =
                        self.create_sized_chunks(sequences, *taxon_id, level, min_size, max_size)?;

                    chunks.extend(taxon_chunks);
                }
            }
        }

        Ok(chunks)
    }

    /// Get importance category for a taxon
    fn get_importance(&self, taxon_id: TaxonId) -> OrganismImportance {
        // Check direct mapping
        if let Some(&importance) = self.importance_map.get(&taxon_id) {
            return importance;
        }

        // Check ancestors for importance
        if let Some(tree) = &self.taxonomy_tree {
            let mut current = Some(taxon_id);

            while let Some(tid) = current {
                if let Some(&importance) = self.importance_map.get(&tid) {
                    return importance;
                }

                if let Some(node) = tree.nodes.get(&tid) {
                    current = node.parent;
                } else {
                    break;
                }
            }
        }

        // Default to environmental
        OrganismImportance::Environmental
    }

    /// Create chunks with size constraints
    fn create_sized_chunks(
        &self,
        sequences: Vec<Sequence>,
        taxon_id: TaxonId,
        level: TaxonomicLevel,
        min_size: usize,
        max_size: usize,
    ) -> Result<Vec<HierarchicalChunk>> {
        let mut chunks = Vec::new();
        let mut current_sequences = Vec::new();
        let mut current_size = 0;

        for seq in sequences {
            let seq_size = seq.sequence.len();

            if current_size + seq_size > max_size && !current_sequences.is_empty() {
                // Create chunk
                chunks.push(HierarchicalChunk {
                    taxon_id,
                    level,
                    sequences: current_sequences.clone(),
                    size: current_size,
                });

                // Start new chunk
                current_sequences = vec![seq];
                current_size = seq_size;
            } else {
                current_sequences.push(seq);
                current_size += seq_size;
            }
        }

        // Create final chunk if it meets minimum size or is the only chunk
        if current_size >= min_size || (chunks.is_empty() && !current_sequences.is_empty()) {
            chunks.push(HierarchicalChunk {
                taxon_id,
                level,
                sequences: current_sequences,
                size: current_size,
            });
        } else if !current_sequences.is_empty() && !chunks.is_empty() {
            // Merge with last chunk if below minimum
            if let Some(last_chunk) = chunks.last_mut() {
                last_chunk.sequences.extend(current_sequences);
                last_chunk.size += current_size;
            }
        }

        Ok(chunks)
    }

    /// Extract taxon ID from sequence
    fn get_taxon_id(&self, seq: &Sequence) -> Result<TaxonId> {
        if let Some(taxon_id) = seq.taxon_id {
            Ok(TaxonId(taxon_id))
        } else {
            // Try to parse from description
            if let Some(desc) = &seq.description {
                if let Some(ox_pos) = desc.find("OX=") {
                    let start = ox_pos + 3;
                    let end = desc[start..]
                        .find(|c: char| !c.is_numeric())
                        .map(|i| start + i)
                        .unwrap_or(desc.len());

                    if let Ok(taxon_id) = desc[start..end].parse::<u32>() {
                        return Ok(TaxonId(taxon_id));
                    }
                }
            }

            // Default to unclassified
            Ok(TaxonId(0))
        }
    }
}

/// Hierarchical organization of sequences
struct TaxonomicHierarchy {
    levels: HashMap<TaxonomicLevel, HashMap<TaxonId, Vec<Sequence>>>,
}

impl TaxonomicHierarchy {
    fn new() -> Self {
        Self {
            levels: HashMap::new(),
        }
    }

    fn add_sequence(&mut self, sequence: Sequence, lineage: Vec<(TaxonomicLevel, TaxonId)>) {
        for (level, taxon_id) in lineage {
            self.levels
                .entry(level)
                .or_default()
                .entry(taxon_id)
                .or_default()
                .push(sequence.clone());
        }
    }

    fn get_taxa_at_level(&self, level: TaxonomicLevel) -> Option<HashSet<TaxonId>> {
        self.levels
            .get(&level)
            .map(|taxa| taxa.keys().cloned().collect())
    }

    fn get_sequences_for_taxon(&self, taxon_id: TaxonId, level: TaxonomicLevel) -> Vec<Sequence> {
        self.levels
            .get(&level)
            .and_then(|taxa| taxa.get(&taxon_id))
            .cloned()
            .unwrap_or_default()
    }
}

/// Result of hierarchical chunking
#[derive(Debug, Clone)]
pub struct HierarchicalChunk {
    pub taxon_id: TaxonId,
    pub level: TaxonomicLevel,
    pub sequences: Vec<Sequence>,
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_organism_importance_sizing() {
        assert_eq!(
            OrganismImportance::ModelOrganism.chunk_size_range(),
            (50 * 1024 * 1024, 200 * 1024 * 1024)
        );
        assert_eq!(
            OrganismImportance::Pathogen.chunk_size_range(),
            (100 * 1024 * 1024, 500 * 1024 * 1024)
        );
        assert_eq!(
            OrganismImportance::Environmental.chunk_size_range(),
            (500 * 1024 * 1024, 1024 * 1024 * 1024)
        );
    }

    #[test]
    fn test_taxonomic_level_ordering() {
        assert!(TaxonomicLevel::Kingdom < TaxonomicLevel::Phylum);
        assert!(TaxonomicLevel::Phylum < TaxonomicLevel::Class);
        assert!(TaxonomicLevel::Class < TaxonomicLevel::Order);
        assert!(TaxonomicLevel::Order < TaxonomicLevel::Family);
        assert!(TaxonomicLevel::Family < TaxonomicLevel::Genus);
        assert!(TaxonomicLevel::Genus < TaxonomicLevel::Species);
    }
}
