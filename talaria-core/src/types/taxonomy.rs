/// Taxonomy-related types used throughout Talaria
use serde::{Deserialize, Serialize};
use std::fmt;

/// Taxonomy ID type - newtype pattern for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[derive(Default)]
pub struct TaxonId(pub u32);

impl TaxonId {
    /// Create a new TaxonId
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the inner value
    pub fn value(&self) -> u32 {
        self.0
    }

    /// Check if this is the root taxon (1)
    pub fn is_root(&self) -> bool {
        self.0 == 1
    }

    /// Check if this is unclassified (0)
    pub fn is_unclassified(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for TaxonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u32> for TaxonId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<TaxonId> for u32 {
    fn from(taxon: TaxonId) -> Self {
        taxon.0
    }
}


// Common taxonomy constants
impl TaxonId {
    pub const UNCLASSIFIED: Self = Self(0);
    pub const ROOT: Self = Self(1);
    pub const BACTERIA: Self = Self(2);
    pub const ARCHAEA: Self = Self(2157);
    pub const EUKARYOTA: Self = Self(2759);
    pub const VIRUSES: Self = Self(10239);
    pub const HUMAN: Self = Self(9606);
    pub const MOUSE: Self = Self(10090);
    pub const ECOLI: Self = Self(562);
}

/// Source of taxonomy data (how it was obtained)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaxonomyDataSource {
    /// Provided by API (e.g., UniProt API when querying by taxid)
    Api,
    /// Specified by user via --taxids parameter
    User,
    /// Looked up from accession2taxid mappings
    Accession2Taxid,
    /// Extracted from FASTA header (OX= field)
    Header,
    /// Inherited from parent sequence during processing
    Inherited,
    /// Unknown source
    Unknown,
}

impl TaxonomyDataSource {
    /// Get priority for conflict resolution (higher is better)
    pub fn priority(&self) -> u8 {
        match self {
            Self::User => 100,          // User-specified has highest priority
            Self::Api => 90,            // API data is very reliable
            Self::Header => 80,         // Header data is reliable
            Self::Accession2Taxid => 70, // Mapping is good but may be outdated
            Self::Inherited => 50,      // Inherited is less certain
            Self::Unknown => 0,          // Unknown has lowest priority
        }
    }

    /// Check if this source is considered reliable
    pub fn is_reliable(&self) -> bool {
        self.priority() >= 70
    }
}

impl Default for TaxonomyDataSource {
    fn default() -> Self {
        Self::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taxon_id_creation() {
        let taxon = TaxonId::new(9606);
        assert_eq!(taxon.value(), 9606);
        assert_eq!(taxon, TaxonId::HUMAN);
    }

    #[test]
    fn test_taxon_id_conversion() {
        let id: u32 = 12345;
        let taxon = TaxonId::from(id);
        let back: u32 = taxon.into();
        assert_eq!(id, back);
    }
}