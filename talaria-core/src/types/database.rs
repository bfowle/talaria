//! Database source types shared across all Talaria crates

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Database source identifier - represents which database a sequence comes from
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
}

/// UniProt database variants
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UniProtDatabase {
    SwissProt,
    TrEMBL,
    UniRef50,
    UniRef90,
    UniRef100,
    IdMapping,
}

/// NCBI database variants
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NCBIDatabase {
    Taxonomy,
    ProtAccession2TaxId,
    NuclAccession2TaxId,
    RefSeq,           // Generic RefSeq (for backwards compatibility)
    RefSeqProtein,    // RefSeq protein database
    RefSeqGenomic,    // RefSeq genomic database
    NR,
    NT,
}

impl fmt::Display for DatabaseSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseSource::UniProt(db) => write!(f, "UniProt: {}", db),
            DatabaseSource::NCBI(db) => write!(f, "NCBI: {}", db),
            DatabaseSource::Custom(name) => write!(f, "Custom: {}", name),
        }
    }
}

impl fmt::Display for UniProtDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UniProtDatabase::SwissProt => write!(f, "SwissProt"),
            UniProtDatabase::TrEMBL => write!(f, "TrEMBL"),
            UniProtDatabase::UniRef50 => write!(f, "UniRef50"),
            UniProtDatabase::UniRef90 => write!(f, "UniRef90"),
            UniProtDatabase::UniRef100 => write!(f, "UniRef100"),
            UniProtDatabase::IdMapping => write!(f, "IdMapping"),
        }
    }
}

impl fmt::Display for NCBIDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NCBIDatabase::Taxonomy => write!(f, "Taxonomy"),
            NCBIDatabase::ProtAccession2TaxId => write!(f, "ProtAccession2TaxId"),
            NCBIDatabase::NuclAccession2TaxId => write!(f, "NuclAccession2TaxId"),
            NCBIDatabase::RefSeq => write!(f, "RefSeq"),
            NCBIDatabase::RefSeqProtein => write!(f, "RefSeq Protein"),
            NCBIDatabase::RefSeqGenomic => write!(f, "RefSeq Genomic"),
            NCBIDatabase::NR => write!(f, "NR"),
            NCBIDatabase::NT => write!(f, "NT"),
        }
    }
}

/// Simple struct representation of database source (for internal use)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatabaseSourceInfo {
    pub source: String,  // e.g., "uniprot", "ncbi"
    pub dataset: String, // e.g., "swissprot", "nr"
}

impl DatabaseSourceInfo {
    pub fn new(source: impl Into<String>, dataset: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            dataset: dataset.into(),
        }
    }
}

impl fmt::Display for DatabaseSourceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.source, self.dataset)
    }
}

impl From<DatabaseSource> for DatabaseSourceInfo {
    fn from(source: DatabaseSource) -> Self {
        match source {
            DatabaseSource::UniProt(db) => DatabaseSourceInfo::new("uniprot", db.name()),
            DatabaseSource::NCBI(db) => DatabaseSourceInfo::new("ncbi", db.name()),
            DatabaseSource::Custom(name) => {
                if let Some((source, dataset)) = name.split_once('/') {
                    DatabaseSourceInfo::new(source, dataset)
                } else {
                    DatabaseSourceInfo::new("custom", name)
                }
            }
        }
    }
}

impl From<&DatabaseSource> for DatabaseSourceInfo {
    fn from(source: &DatabaseSource) -> Self {
        match source {
            DatabaseSource::UniProt(db) => DatabaseSourceInfo::new("uniprot", db.name()),
            DatabaseSource::NCBI(db) => DatabaseSourceInfo::new("ncbi", db.name()),
            DatabaseSource::Custom(name) => {
                if let Some((source, dataset)) = name.split_once('/') {
                    DatabaseSourceInfo::new(source, dataset)
                } else {
                    DatabaseSourceInfo::new("custom", name)
                }
            }
        }
    }
}

impl DatabaseSource {
    /// Parse a database name string into a DatabaseSource
    pub fn parse(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "uniprot/swissprot" | "swissprot" => {
                DatabaseSource::UniProt(UniProtDatabase::SwissProt)
            }
            "uniprot/trembl" | "trembl" => {
                DatabaseSource::UniProt(UniProtDatabase::TrEMBL)
            }
            "uniprot/uniref50" | "uniref50" => {
                DatabaseSource::UniProt(UniProtDatabase::UniRef50)
            }
            "uniprot/uniref90" | "uniref90" => {
                DatabaseSource::UniProt(UniProtDatabase::UniRef90)
            }
            "uniprot/uniref100" | "uniref100" => {
                DatabaseSource::UniProt(UniProtDatabase::UniRef100)
            }
            "uniprot/idmapping" => {
                DatabaseSource::UniProt(UniProtDatabase::IdMapping)
            }
            "ncbi/taxonomy" | "taxonomy" => {
                DatabaseSource::NCBI(NCBIDatabase::Taxonomy)
            }
            "ncbi/prot-accession2taxid" => {
                DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId)
            }
            "ncbi/nucl-accession2taxid" => {
                DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId)
            }
            "ncbi/refseq" | "refseq" => {
                DatabaseSource::NCBI(NCBIDatabase::RefSeq)
            }
            "ncbi/refseq-protein" | "refseq-protein" => {
                DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein)
            }
            "ncbi/refseq-genomic" | "refseq-genomic" => {
                DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic)
            }
            "ncbi/nr" | "nr" => {
                DatabaseSource::NCBI(NCBIDatabase::NR)
            }
            "ncbi/nt" | "nt" => {
                DatabaseSource::NCBI(NCBIDatabase::NT)
            }
            custom => DatabaseSource::Custom(custom.to_string()),
        }
    }

    /// Get the source name (e.g., "uniprot", "ncbi", "custom")
    pub fn source_name(&self) -> &str {
        match self {
            DatabaseSource::UniProt(_) => "uniprot",
            DatabaseSource::NCBI(_) => "ncbi",
            DatabaseSource::Custom(_) => "custom",
        }
    }

    /// Get the dataset name (e.g., "swissprot", "nr", etc.)
    pub fn dataset_name(&self) -> String {
        match self {
            DatabaseSource::UniProt(db) => db.to_string().to_lowercase(),
            DatabaseSource::NCBI(db) => db.to_string().to_lowercase(),
            DatabaseSource::Custom(name) => name.clone(),
        }
    }
}

impl UniProtDatabase {
    pub fn name(&self) -> &str {
        match self {
            UniProtDatabase::SwissProt => "swissprot",
            UniProtDatabase::TrEMBL => "trembl",
            UniProtDatabase::UniRef50 => "uniref50",
            UniProtDatabase::UniRef90 => "uniref90",
            UniProtDatabase::UniRef100 => "uniref100",
            UniProtDatabase::IdMapping => "idmapping",
        }
    }
}

impl NCBIDatabase {
    pub fn name(&self) -> &str {
        match self {
            NCBIDatabase::Taxonomy => "taxonomy",
            NCBIDatabase::ProtAccession2TaxId => "prot-accession2taxid",
            NCBIDatabase::NuclAccession2TaxId => "nucl-accession2taxid",
            NCBIDatabase::RefSeq => "refseq",
            NCBIDatabase::RefSeqProtein => "refseq-protein",
            NCBIDatabase::RefSeqGenomic => "refseq-genomic",
            NCBIDatabase::NR => "nr",
            NCBIDatabase::NT => "nt",
        }
    }
}

/// Represents a complete database reference with version and profile
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseReference {
    /// Source system (e.g., "uniprot", "ncbi", "custom")
    pub source: String,

    /// Dataset name (e.g., "swissprot", "nr", "trembl")
    pub dataset: String,

    /// Version specification (e.g., "2024_04", "current", "stable")
    /// None means use the current/latest version
    pub version: Option<String>,

    /// Reduction profile (e.g., "50-percent", "auto-detect")
    /// None means use the auto-detect profile
    pub profile: Option<String>,
}

impl DatabaseReference {
    /// Create a new database reference
    pub fn new(source: String, dataset: String) -> Self {
        Self {
            source,
            dataset,
            version: None,
            profile: None,
        }
    }

    /// Create with all fields
    pub fn with_all(
        source: String,
        dataset: String,
        version: Option<String>,
        profile: Option<String>,
    ) -> Self {
        Self {
            source,
            dataset,
            version,
            profile,
        }
    }

    /// Get the base reference without version or profile
    pub fn base_ref(&self) -> String {
        format!("{}/{}", self.source, self.dataset)
    }

    /// Get version or default
    pub fn version_or_default(&self) -> &str {
        self.version.as_deref().unwrap_or("current")
    }

    /// Get profile or default
    pub fn profile_or_default(&self) -> &str {
        self.profile.as_deref().unwrap_or("auto-detect")
    }

    /// Parse from a string like "uniprot/swissprot:2024_04#blast-30"
    pub fn parse(input: &str) -> Result<Self> {
        // Split on '#' for profile
        let (base_with_version, profile) = if let Some(idx) = input.rfind('#') {
            (&input[..idx], Some(input[idx + 1..].to_string()))
        } else {
            (input, None)
        };

        // Split on ':' for version
        let (base, version) = if let Some(idx) = base_with_version.rfind(':') {
            (
                &base_with_version[..idx],
                Some(base_with_version[idx + 1..].to_string()),
            )
        } else {
            (base_with_version, None)
        };

        // Split on '/' for source/dataset
        let parts: Vec<&str> = base.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid database reference format: '{}' (expected 'source/dataset')",
                input
            );
        }

        Ok(Self {
            source: parts[0].to_string(),
            dataset: parts[1].to_string(),
            version,
            profile,
        })
    }

    /// Check if this reference matches another (with wildcards)
    pub fn matches(&self, other: &DatabaseReference) -> bool {
        // Source and dataset must always match
        if self.source != other.source || self.dataset != other.dataset {
            return false;
        }

        // Version matching (None matches any)
        if let (Some(v1), Some(v2)) = (&self.version, &other.version) {
            if v1 != v2 {
                return false;
            }
        }

        // Profile matching (None matches any)
        if let (Some(p1), Some(p2)) = (&self.profile, &other.profile) {
            if p1 != p2 {
                return false;
            }
        }

        true
    }
}

impl std::fmt::Display for DatabaseReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.source, self.dataset)?;
        if let Some(v) = &self.version {
            write!(f, ":{}", v)?;
        }
        if let Some(p) = &self.profile {
            write!(f, "#{}", p)?;
        }
        Ok(())
    }
}