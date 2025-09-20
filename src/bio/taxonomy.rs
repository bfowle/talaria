/// Taxonomy resolution traits and types for biological sequences
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Standard taxonomic ranks from kingdom to species
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaxonomicRank {
    Superkingdom,
    Kingdom,
    Phylum,
    Class,
    Order,
    Family,
    Genus,
    Species,
    Subspecies,
    Strain,
    NoRank,
}

impl TaxonomicRank {
    /// Parse rank from NCBI taxonomy string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "superkingdom" => Self::Superkingdom,
            "kingdom" => Self::Kingdom,
            "phylum" => Self::Phylum,
            "class" => Self::Class,
            "order" => Self::Order,
            "family" => Self::Family,
            "genus" => Self::Genus,
            "species" => Self::Species,
            "subspecies" => Self::Subspecies,
            "strain" | "varietas" | "forma" => Self::Strain,
            _ => Self::NoRank,
        }
    }

    /// Get rank depth for distance calculations (lower = higher in hierarchy)
    pub fn depth(&self) -> u32 {
        match self {
            Self::Superkingdom => 0,
            Self::Kingdom => 1,
            Self::Phylum => 2,
            Self::Class => 3,
            Self::Order => 4,
            Self::Family => 5,
            Self::Genus => 6,
            Self::Species => 7,
            Self::Subspecies => 8,
            Self::Strain => 9,
            Self::NoRank => 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyInfo {
    pub taxon_id: u32,
    pub scientific_name: String,
    pub rank: String,
    pub parent_id: Option<u32>,
}

#[derive(Debug)]
pub struct TaxonomyDB {
    taxa: HashMap<u32, TaxonomyInfo>,
}

impl TaxonomyDB {
    pub fn new() -> Self {
        Self {
            taxa: HashMap::new(),
        }
    }

    pub fn add_taxon(&mut self, info: TaxonomyInfo) {
        self.taxa.insert(info.taxon_id, info);
    }

    pub fn get_taxon(&self, taxon_id: u32) -> Option<&TaxonomyInfo> {
        self.taxa.get(&taxon_id)
    }

    pub fn get_lineage(&self, taxon_id: u32) -> Vec<u32> {
        let mut lineage = Vec::new();
        let mut current_id = Some(taxon_id);

        while let Some(id) = current_id {
            lineage.push(id);
            current_id = self.taxa.get(&id).and_then(|t| t.parent_id);
        }

        lineage.reverse();
        lineage
    }

    pub fn common_ancestor(&self, taxon_a: u32, taxon_b: u32) -> Option<u32> {
        let lineage_a = self.get_lineage(taxon_a);
        let lineage_b = self.get_lineage(taxon_b);

        let mut common = None;
        for (a, b) in lineage_a.iter().zip(lineage_b.iter()) {
            if a == b {
                common = Some(*a);
            } else {
                break;
            }
        }

        common
    }

    pub fn distance(&self, taxon_a: u32, taxon_b: u32) -> Option<usize> {
        let lineage_a = self.get_lineage(taxon_a);
        let lineage_b = self.get_lineage(taxon_b);

        if let Some(common) = self.common_ancestor(taxon_a, taxon_b) {
            let dist_a = lineage_a.iter().position(|&x| x == common)?;
            let dist_b = lineage_b.iter().position(|&x| x == common)?;
            Some(lineage_a.len() - dist_a + lineage_b.len() - dist_b - 2)
        } else {
            None
        }
    }

    pub fn taxa_count(&self) -> usize {
        self.taxa.len()
    }

    /// Get the taxonomic rank of a taxon
    pub fn get_rank(&self, taxon_id: u32) -> Option<TaxonomicRank> {
        self.taxa
            .get(&taxon_id)
            .map(|info| TaxonomicRank::from_str(&info.rank))
    }

    /// Find the ancestor of a taxon at a specific rank
    pub fn find_ancestor_at_rank(&self, taxon_id: u32, target_rank: TaxonomicRank) -> Option<u32> {
        let lineage = self.get_lineage(taxon_id);

        for ancestor_id in lineage {
            if let Some(rank) = self.get_rank(ancestor_id) {
                if rank == target_rank {
                    return Some(ancestor_id);
                }
            }
        }

        None
    }

    /// Calculate phylogenetic distance between two taxa
    /// Returns a normalized distance score (0.0 = identical, 1.0 = maximally distant)
    pub fn phylogenetic_distance(&self, taxon_a: u32, taxon_b: u32) -> f64 {
        if taxon_a == taxon_b {
            return 0.0;
        }

        // Find common ancestor and calculate distances
        let lineage_a = self.get_lineage(taxon_a);
        let lineage_b = self.get_lineage(taxon_b);

        // Find divergence point
        let mut common_depth = 0;
        for (a, b) in lineage_a.iter().zip(lineage_b.iter()) {
            if a == b {
                common_depth += 1;
            } else {
                break;
            }
        }

        // Calculate distance based on divergence depth
        // Normalize by maximum possible depth
        let max_depth = lineage_a.len().max(lineage_b.len()) as f64;
        if max_depth == 0.0 {
            return 1.0;
        }

        let divergence_depth = (lineage_a.len() + lineage_b.len() - 2 * common_depth) as f64;
        (divergence_depth / (2.0 * max_depth)).min(1.0)
    }

    /// Get all taxa at a specific rank
    pub fn get_taxa_at_rank(&self, rank: TaxonomicRank) -> Vec<u32> {
        self.taxa
            .iter()
            .filter(|(_, info)| TaxonomicRank::from_str(&info.rank) == rank)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Group taxa by their common ancestor at a specific rank
    pub fn group_by_ancestor_at_rank(
        &self,
        taxon_ids: &[u32],
        rank: TaxonomicRank,
    ) -> HashMap<u32, Vec<u32>> {
        let mut groups: HashMap<u32, Vec<u32>> = HashMap::new();

        for &taxon_id in taxon_ids {
            if let Some(ancestor) = self.find_ancestor_at_rank(taxon_id, rank) {
                groups.entry(ancestor).or_default().push(taxon_id);
            } else {
                // No ancestor at this rank, use root (0) as placeholder
                groups.entry(0).or_default().push(taxon_id);
            }
        }

        groups
    }

    /// Check if two taxa are closely related (within same genus)
    pub fn are_closely_related(&self, taxon_a: u32, taxon_b: u32) -> bool {
        // Check if they share the same genus
        if let (Some(genus_a), Some(genus_b)) = (
            self.find_ancestor_at_rank(taxon_a, TaxonomicRank::Genus),
            self.find_ancestor_at_rank(taxon_b, TaxonomicRank::Genus),
        ) {
            genus_a == genus_b
        } else {
            false
        }
    }

    /// Get the lowest common rank between two taxa
    pub fn lowest_common_rank(&self, taxon_a: u32, taxon_b: u32) -> Option<TaxonomicRank> {
        if let Some(common_ancestor) = self.common_ancestor(taxon_a, taxon_b) {
            self.get_rank(common_ancestor)
        } else {
            None
        }
    }

    /// Find related taxa within a phylogenetic distance threshold
    pub fn find_related_taxa(&self, taxon_id: u32, distance_threshold: f64) -> Vec<u32> {
        self.taxa
            .keys()
            .filter(|&&other_id| {
                other_id != taxon_id
                    && self.phylogenetic_distance(taxon_id, other_id) <= distance_threshold
            })
            .copied()
            .collect()
    }
}

/// Parse NCBI taxonomy dump files
pub mod ncbi {
    use super::*;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::Path;

    pub fn load_names<P: AsRef<Path>>(path: P) -> Result<HashMap<u32, String>, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut names = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();

            if parts.len() >= 4 && parts[3].trim_end_matches("\t|") == "scientific name" {
                if let Ok(taxon_id) = parts[0].parse::<u32>() {
                    names.insert(taxon_id, parts[1].to_string());
                }
            }
        }

        Ok(names)
    }

    pub fn load_nodes<P: AsRef<Path>>(
        path: P,
    ) -> Result<HashMap<u32, (u32, String)>, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut nodes = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();

            if parts.len() >= 3 {
                if let (Ok(taxon_id), Ok(parent_id)) =
                    (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                {
                    let rank = parts[2].to_string();
                    nodes.insert(taxon_id, (parent_id, rank));
                }
            }
        }

        Ok(nodes)
    }

    pub fn build_taxonomy_db<P: AsRef<Path>>(
        names_path: P,
        nodes_path: P,
    ) -> Result<TaxonomyDB, std::io::Error> {
        let names = load_names(names_path)?;
        let nodes = load_nodes(nodes_path)?;

        let mut db = TaxonomyDB::new();

        for (taxon_id, name) in names {
            if let Some((parent_id, rank)) = nodes.get(&taxon_id) {
                let info = TaxonomyInfo {
                    taxon_id,
                    scientific_name: name,
                    rank: rank.clone(),
                    parent_id: if *parent_id == taxon_id {
                        None
                    } else {
                        Some(*parent_id)
                    },
                };
                db.add_taxon(info);
            }
        }

        Ok(db)
    }

    // Alias for convenience
    pub fn parse_ncbi_taxonomy<P: AsRef<Path>>(
        names_path: P,
        nodes_path: P,
    ) -> Result<TaxonomyDB, std::io::Error> {
        build_taxonomy_db(names_path, nodes_path)
    }
}

/// Represents different sources of taxonomy information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaxonomySource {
    /// Provided by API (e.g., UniProt API when querying by taxid)
    Api,
    /// Specified by user via --taxids parameter
    User,
    /// Looked up from accession2taxid mappings
    Mapping,
    /// Parsed from sequence header/description
    Header,
    /// Inferred from chunk context in CASG
    ChunkContext,
}

/// Confidence level in taxonomy assignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaxonomyConfidence {
    /// No taxonomy information available
    None,
    /// Low confidence (parsed from text, might be wrong)
    Low,
    /// Medium confidence (from mappings, might be outdated)
    Medium,
    /// High confidence (from authoritative API)
    High,
    /// Verified (multiple sources agree)
    Verified,
}

/// Stores taxonomy IDs from different sources
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaxonomySources {
    /// Taxonomy ID provided by API
    pub api_provided: Option<u32>,
    /// Taxonomy ID specified by user
    pub user_specified: Option<u32>,
    /// Taxonomy ID from accession2taxid mapping
    pub mapping_lookup: Option<u32>,
    /// Taxonomy ID parsed from header
    pub header_parsed: Option<u32>,
    /// Taxonomy ID from chunk context
    pub chunk_context: Option<u32>,
}

impl TaxonomySources {
    /// Create new empty taxonomy sources
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all available sources as a vector
    pub fn all_sources(&self) -> Vec<(TaxonomySource, u32)> {
        let mut sources = Vec::new();

        if let Some(id) = self.api_provided {
            sources.push((TaxonomySource::Api, id));
        }
        if let Some(id) = self.user_specified {
            sources.push((TaxonomySource::User, id));
        }
        if let Some(id) = self.mapping_lookup {
            sources.push((TaxonomySource::Mapping, id));
        }
        if let Some(id) = self.header_parsed {
            sources.push((TaxonomySource::Header, id));
        }
        if let Some(id) = self.chunk_context {
            sources.push((TaxonomySource::ChunkContext, id));
        }

        sources
    }

    /// Get unique taxonomy IDs from all sources
    pub fn unique_ids(&self) -> HashSet<u32> {
        self.all_sources().into_iter().map(|(_, id)| id).collect()
    }

    /// Check if sources have conflicts
    pub fn has_conflicts(&self) -> bool {
        self.unique_ids().len() > 1
    }

    /// Get the highest priority taxonomy ID
    pub fn resolve_with_priority(&self) -> Option<u32> {
        // Priority order: API > User > Mapping > Header > ChunkContext
        self.api_provided
            .or(self.user_specified)
            .or(self.mapping_lookup)
            .or(self.header_parsed)
            .or(self.chunk_context)
    }
}

/// Result of taxonomy resolution
#[derive(Debug, Clone)]
pub enum TaxonomyResolution {
    /// No taxonomy information available
    None,
    /// All sources agree on the same taxonomy
    Unanimous {
        taxon_id: u32,
        sources: Vec<(TaxonomySource, u32)>,
    },
    /// Sources conflict, but resolved to a value
    Conflicted {
        candidates: Vec<(TaxonomySource, u32)>,
        resolved_to: u32,
    },
}

impl TaxonomyResolution {
    /// Get the primary taxon ID from the resolution
    pub fn get_primary_taxon(&self) -> u32 {
        match self {
            TaxonomyResolution::None => 0,
            TaxonomyResolution::Unanimous { taxon_id, .. } => *taxon_id,
            TaxonomyResolution::Conflicted { resolved_to, .. } => *resolved_to,
        }
    }

    /// Check if this resolution has conflicts
    pub fn has_conflicts(&self) -> bool {
        matches!(self, TaxonomyResolution::Conflicted { .. })
    }

    /// Get confidence level based on resolution
    pub fn confidence(&self) -> TaxonomyConfidence {
        match self {
            TaxonomyResolution::None => TaxonomyConfidence::None,
            TaxonomyResolution::Unanimous { sources, .. } => {
                // If API or User provided and others agree, very high confidence
                if sources
                    .iter()
                    .any(|(s, _)| matches!(s, TaxonomySource::Api | TaxonomySource::User))
                    && sources.len() > 1
                {
                    TaxonomyConfidence::Verified
                } else if sources
                    .iter()
                    .any(|(s, _)| matches!(s, TaxonomySource::Api))
                {
                    TaxonomyConfidence::High
                } else if sources
                    .iter()
                    .any(|(s, _)| matches!(s, TaxonomySource::Mapping))
                {
                    TaxonomyConfidence::Medium
                } else {
                    TaxonomyConfidence::Low
                }
            }
            TaxonomyResolution::Conflicted { .. } => TaxonomyConfidence::Low,
        }
    }
}

/// Represents a taxonomy discrepancy
#[derive(Debug, Clone)]
pub struct TaxonomyDiscrepancy {
    /// ID of the sequence with discrepancy
    pub sequence_id: String,
    /// Conflicting taxonomy assignments
    pub conflicts: Vec<(TaxonomySource, u32)>,
    /// How the conflict was resolved
    pub resolution_strategy: &'static str,
}

/// Trait for resolving taxonomy from multiple sources
pub trait TaxonomyResolver {
    /// Get taxonomy ID with source tracking
    fn resolve_taxonomy(&self) -> TaxonomyResolution;

    /// Get all available taxonomy sources
    fn taxonomy_sources(&self) -> &TaxonomySources;

    /// Get mutable access to taxonomy sources
    fn taxonomy_sources_mut(&mut self) -> &mut TaxonomySources;

    /// Report discrepancies if multiple sources conflict
    fn detect_discrepancies(&self) -> Vec<TaxonomyDiscrepancy>;
}

/// Trait for enriching sequences with taxonomy information
pub trait TaxonomyEnrichable: TaxonomyResolver {
    /// Enrich with taxonomy from accession2taxid mappings
    fn enrich_from_mappings(&mut self, mappings: &HashMap<String, u32>);

    /// Enrich with user-specified taxonomy
    fn enrich_from_user(&mut self, taxid: u32);

    /// Enrich from header parsing
    fn enrich_from_header(&mut self);

    /// Enrich from chunk context
    fn enrich_from_chunk(&mut self, taxid: u32);

    /// Extract accession for mapping lookup
    fn extract_accession(&self) -> String;

    /// Get description for header parsing
    fn get_description(&self) -> Option<&str>;
}

/// Trait for providers that can fetch sequences with taxonomy
pub trait SequenceProvider {
    /// Download sequences with full taxonomy preservation
    fn fetch_sequences(&self) -> Result<Vec<crate::bio::sequence::Sequence>>;

    /// Get provider's confidence in taxonomy data
    fn taxonomy_confidence(&self) -> TaxonomyConfidence;

    /// Get the source type for this provider
    fn source_type(&self) -> TaxonomySource;
}

/// Parse taxonomy ID from a sequence description
pub fn parse_taxonomy_from_description(description: &Option<String>) -> Option<u32> {
    let desc = description.as_ref()?;

    // Look for UniProt OX= pattern
    if let Some(ox_pos) = desc.find("OX=") {
        let start = ox_pos + 3;
        let end = desc[start..]
            .find(|c: char| !c.is_numeric())
            .map(|i| start + i)
            .unwrap_or(desc.len());

        if let Ok(taxon_id) = desc[start..end].parse::<u32>() {
            return Some(taxon_id);
        }
    }

    // Look for TaxID= pattern
    if let Some(tax_pos) = desc.find("TaxID=") {
        let start = tax_pos + 6;
        let end = desc[start..]
            .find(|c: char| !c.is_numeric())
            .map(|i| start + i)
            .unwrap_or(desc.len());

        if let Ok(taxon_id) = desc[start..end].parse::<u32>() {
            return Some(taxon_id);
        }
    }

    // Look for [organism] pattern at the end
    if let Some(bracket_start) = desc.rfind('[') {
        if let Some(bracket_end) = desc[bracket_start..].find(']') {
            let _organism = &desc[bracket_start + 1..bracket_start + bracket_end];
            // Would need organism to taxid mapping here
            // For now, return None
        }
    }

    None
}

/// Extract accession from sequence ID
pub fn extract_accession_from_id(id: &str) -> String {
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

    // Simple accession - remove version
    id.split('.').next().unwrap_or(id).to_string()
}
