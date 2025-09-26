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
    /// Test database source for unit testing
    Test,
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
    GenBank,          // GenBank database
}

impl fmt::Display for DatabaseSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DatabaseSource::UniProt(db) => write!(f, "UniProt: {}", db),
            DatabaseSource::NCBI(db) => write!(f, "NCBI: {}", db),
            DatabaseSource::Custom(name) => write!(f, "Custom: {}", name),
            DatabaseSource::Test => write!(f, "Test"),
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
            NCBIDatabase::GenBank => write!(f, "GenBank"),
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
            DatabaseSource::Test => DatabaseSourceInfo::new("test", "test"),
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
            DatabaseSource::Test => DatabaseSourceInfo::new("test", "test"),
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
            "ncbi/genbank" | "genbank" => {
                DatabaseSource::NCBI(NCBIDatabase::GenBank)
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
            DatabaseSource::Test => "test",
        }
    }

    /// Get the dataset name (e.g., "swissprot", "nr", etc.)
    pub fn dataset_name(&self) -> String {
        match self {
            DatabaseSource::UniProt(db) => db.to_string().to_lowercase(),
            DatabaseSource::NCBI(db) => db.to_string().to_lowercase(),
            DatabaseSource::Custom(name) => name.clone(),
            DatabaseSource::Test => "test".to_string(),
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
            NCBIDatabase::GenBank => "genbank",
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

    /// Parse from a string like "uniprot/swissprot@2024_04:blast-30"
    pub fn parse(input: &str) -> Result<Self> {
        // Split on ':' for profile
        let (base_with_version, profile) = if let Some(idx) = input.rfind(':') {
            (&input[..idx], Some(input[idx + 1..].to_string()))
        } else {
            (input, None)
        };

        // Split on '@' for version
        let (base, version) = if let Some(idx) = base_with_version.rfind('@') {
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
            write!(f, "@{}", v)?;
        }
        if let Some(p) = &self.profile {
            write!(f, ":{}", p)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_source_display() {
        assert_eq!(
            format!("{}", DatabaseSource::UniProt(UniProtDatabase::SwissProt)),
            "UniProt: SwissProt"
        );
        assert_eq!(
            format!("{}", DatabaseSource::NCBI(NCBIDatabase::NR)),
            "NCBI: NR"
        );
        assert_eq!(
            format!("{}", DatabaseSource::Custom("mydb".to_string())),
            "Custom: mydb"
        );
        assert_eq!(format!("{}", DatabaseSource::Test), "Test");
    }

    #[test]
    fn test_uniprot_database_display() {
        assert_eq!(format!("{}", UniProtDatabase::SwissProt), "SwissProt");
        assert_eq!(format!("{}", UniProtDatabase::TrEMBL), "TrEMBL");
        assert_eq!(format!("{}", UniProtDatabase::UniRef50), "UniRef50");
        assert_eq!(format!("{}", UniProtDatabase::UniRef90), "UniRef90");
        assert_eq!(format!("{}", UniProtDatabase::UniRef100), "UniRef100");
        assert_eq!(format!("{}", UniProtDatabase::IdMapping), "IdMapping");
    }

    #[test]
    fn test_ncbi_database_display() {
        assert_eq!(format!("{}", NCBIDatabase::Taxonomy), "Taxonomy");
        assert_eq!(format!("{}", NCBIDatabase::ProtAccession2TaxId), "ProtAccession2TaxId");
        assert_eq!(format!("{}", NCBIDatabase::RefSeqProtein), "RefSeq Protein");
        assert_eq!(format!("{}", NCBIDatabase::NR), "NR");
        assert_eq!(format!("{}", NCBIDatabase::NT), "NT");
        assert_eq!(format!("{}", NCBIDatabase::GenBank), "GenBank");
    }

    #[test]
    fn test_database_source_parse() {
        // UniProt parsing
        assert!(matches!(
            DatabaseSource::parse("swissprot"),
            DatabaseSource::UniProt(UniProtDatabase::SwissProt)
        ));
        assert!(matches!(
            DatabaseSource::parse("uniprot/swissprot"),
            DatabaseSource::UniProt(UniProtDatabase::SwissProt)
        ));
        assert!(matches!(
            DatabaseSource::parse("trembl"),
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL)
        ));

        // NCBI parsing
        assert!(matches!(
            DatabaseSource::parse("ncbi/nr"),
            DatabaseSource::NCBI(NCBIDatabase::NR)
        ));
        assert!(matches!(
            DatabaseSource::parse("nr"),
            DatabaseSource::NCBI(NCBIDatabase::NR)
        ));
        assert!(matches!(
            DatabaseSource::parse("taxonomy"),
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy)
        ));

        // Custom parsing
        match DatabaseSource::parse("my_custom_db") {
            DatabaseSource::Custom(name) => assert_eq!(name, "my_custom_db"),
            _ => panic!("Expected Custom variant"),
        }
    }

    #[test]
    fn test_database_source_methods() {
        let uniprot = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        assert_eq!(uniprot.source_name(), "uniprot");
        assert_eq!(uniprot.dataset_name(), "swissprot");

        let ncbi = DatabaseSource::NCBI(NCBIDatabase::NR);
        assert_eq!(ncbi.source_name(), "ncbi");
        assert_eq!(ncbi.dataset_name(), "nr");

        let custom = DatabaseSource::Custom("mydb".to_string());
        assert_eq!(custom.source_name(), "custom");
        assert_eq!(custom.dataset_name(), "mydb");

        let test = DatabaseSource::Test;
        assert_eq!(test.source_name(), "test");
        assert_eq!(test.dataset_name(), "test");
    }

    #[test]
    fn test_uniprot_database_name() {
        assert_eq!(UniProtDatabase::SwissProt.name(), "swissprot");
        assert_eq!(UniProtDatabase::TrEMBL.name(), "trembl");
        assert_eq!(UniProtDatabase::UniRef50.name(), "uniref50");
        assert_eq!(UniProtDatabase::UniRef90.name(), "uniref90");
        assert_eq!(UniProtDatabase::UniRef100.name(), "uniref100");
        assert_eq!(UniProtDatabase::IdMapping.name(), "idmapping");
    }

    #[test]
    fn test_ncbi_database_name() {
        assert_eq!(NCBIDatabase::Taxonomy.name(), "taxonomy");
        assert_eq!(NCBIDatabase::ProtAccession2TaxId.name(), "prot-accession2taxid");
        assert_eq!(NCBIDatabase::NuclAccession2TaxId.name(), "nucl-accession2taxid");
        assert_eq!(NCBIDatabase::RefSeq.name(), "refseq");
        assert_eq!(NCBIDatabase::RefSeqProtein.name(), "refseq-protein");
        assert_eq!(NCBIDatabase::RefSeqGenomic.name(), "refseq-genomic");
        assert_eq!(NCBIDatabase::NR.name(), "nr");
        assert_eq!(NCBIDatabase::NT.name(), "nt");
        assert_eq!(NCBIDatabase::GenBank.name(), "genbank");
    }

    #[test]
    fn test_database_source_info_new() {
        let info = DatabaseSourceInfo::new("uniprot", "swissprot");
        assert_eq!(info.source, "uniprot");
        assert_eq!(info.dataset, "swissprot");
        assert_eq!(format!("{}", info), "uniprot/swissprot");
    }

    #[test]
    fn test_database_source_to_info_conversion() {
        // UniProt conversion
        let uniprot = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let info: DatabaseSourceInfo = uniprot.into();
        assert_eq!(info.source, "uniprot");
        assert_eq!(info.dataset, "swissprot");

        // NCBI conversion
        let ncbi = DatabaseSource::NCBI(NCBIDatabase::NR);
        let info: DatabaseSourceInfo = ncbi.into();
        assert_eq!(info.source, "ncbi");
        assert_eq!(info.dataset, "nr");

        // Custom conversion with slash
        let custom = DatabaseSource::Custom("org/database".to_string());
        let info: DatabaseSourceInfo = custom.into();
        assert_eq!(info.source, "org");
        assert_eq!(info.dataset, "database");

        // Custom conversion without slash
        let custom = DatabaseSource::Custom("mydb".to_string());
        let info: DatabaseSourceInfo = custom.into();
        assert_eq!(info.source, "custom");
        assert_eq!(info.dataset, "mydb");

        // Test conversion
        let test = DatabaseSource::Test;
        let info: DatabaseSourceInfo = test.into();
        assert_eq!(info.source, "test");
        assert_eq!(info.dataset, "test");
    }

    #[test]
    fn test_database_source_ref_to_info_conversion() {
        let uniprot = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let info: DatabaseSourceInfo = (&uniprot).into();
        assert_eq!(info.source, "uniprot");
        assert_eq!(info.dataset, "swissprot");
    }

    #[test]
    fn test_database_reference_new() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        assert_eq!(ref1.source, "uniprot");
        assert_eq!(ref1.dataset, "swissprot");
        assert_eq!(ref1.version, None);
        assert_eq!(ref1.profile, None);
    }

    #[test]
    fn test_database_reference_with_all() {
        let ref1 = DatabaseReference::with_all(
            "ncbi".to_string(),
            "nr".to_string(),
            Some("2024_04".to_string()),
            Some("blast-30".to_string()),
        );
        assert_eq!(ref1.source, "ncbi");
        assert_eq!(ref1.dataset, "nr");
        assert_eq!(ref1.version, Some("2024_04".to_string()));
        assert_eq!(ref1.profile, Some("blast-30".to_string()));
    }

    #[test]
    fn test_database_reference_base_ref() {
        let ref1 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            Some("2024_04".to_string()),
            Some("blast".to_string()),
        );
        assert_eq!(ref1.base_ref(), "uniprot/swissprot");
    }

    #[test]
    fn test_database_reference_defaults() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        assert_eq!(ref1.version_or_default(), "current");
        assert_eq!(ref1.profile_or_default(), "auto-detect");

        let ref2 = DatabaseReference::with_all(
            "ncbi".to_string(),
            "nr".to_string(),
            Some("2024_04".to_string()),
            Some("custom".to_string()),
        );
        assert_eq!(ref2.version_or_default(), "2024_04");
        assert_eq!(ref2.profile_or_default(), "custom");
    }

    #[test]
    fn test_database_reference_parse() {
        // Simple format
        let ref1 = DatabaseReference::parse("uniprot/swissprot").unwrap();
        assert_eq!(ref1.source, "uniprot");
        assert_eq!(ref1.dataset, "swissprot");
        assert_eq!(ref1.version, None);
        assert_eq!(ref1.profile, None);

        // With version
        let ref2 = DatabaseReference::parse("ncbi/nr@2024_04").unwrap();
        assert_eq!(ref2.source, "ncbi");
        assert_eq!(ref2.dataset, "nr");
        assert_eq!(ref2.version, Some("2024_04".to_string()));
        assert_eq!(ref2.profile, None);

        // With profile
        let ref3 = DatabaseReference::parse("uniprot/trembl:blast-30").unwrap();
        assert_eq!(ref3.source, "uniprot");
        assert_eq!(ref3.dataset, "trembl");
        assert_eq!(ref3.version, None);
        assert_eq!(ref3.profile, Some("blast-30".to_string()));

        // With both version and profile
        let ref4 = DatabaseReference::parse("ncbi/nt@2024_04:blast-50").unwrap();
        assert_eq!(ref4.source, "ncbi");
        assert_eq!(ref4.dataset, "nt");
        assert_eq!(ref4.version, Some("2024_04".to_string()));
        assert_eq!(ref4.profile, Some("blast-50".to_string()));

        // Invalid format
        assert!(DatabaseReference::parse("invalid").is_err());
        assert!(DatabaseReference::parse("").is_err());
        assert!(DatabaseReference::parse("one/two/three").is_err());
    }

    #[test]
    fn test_database_reference_display() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        assert_eq!(format!("{}", ref1), "uniprot/swissprot");

        let ref2 = DatabaseReference::with_all(
            "ncbi".to_string(),
            "nr".to_string(),
            Some("2024_04".to_string()),
            None,
        );
        assert_eq!(format!("{}", ref2), "ncbi/nr@2024_04");

        let ref3 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "trembl".to_string(),
            None,
            Some("blast-30".to_string()),
        );
        assert_eq!(format!("{}", ref3), "uniprot/trembl:blast-30");

        let ref4 = DatabaseReference::with_all(
            "ncbi".to_string(),
            "nt".to_string(),
            Some("2024_04".to_string()),
            Some("blast-50".to_string()),
        );
        assert_eq!(format!("{}", ref4), "ncbi/nt@2024_04:blast-50");
    }

    #[test]
    fn test_database_reference_matches() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        let ref2 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        let ref3 = DatabaseReference::new("ncbi".to_string(), "nr".to_string());

        // Same reference
        assert!(ref1.matches(&ref2));

        // Different source/dataset
        assert!(!ref1.matches(&ref3));

        // Version matching
        let ref_v1 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            Some("2024_04".to_string()),
            None,
        );
        let ref_v2 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            Some("2024_05".to_string()),
            None,
        );

        assert!(!ref_v1.matches(&ref_v2)); // Different versions
        assert!(ref1.matches(&ref_v1)); // None matches any version
        assert!(ref_v1.matches(&ref1)); // Any version matches None

        // Profile matching
        let ref_p1 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            None,
            Some("blast-30".to_string()),
        );
        let ref_p2 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            None,
            Some("blast-50".to_string()),
        );

        assert!(!ref_p1.matches(&ref_p2)); // Different profiles
        assert!(ref1.matches(&ref_p1)); // None matches any profile
    }

    #[test]
    fn test_database_reference_round_trip() {
        let original = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            Some("2024_04".to_string()),
            Some("blast-30".to_string()),
        );

        let formatted = format!("{}", original);
        let parsed = DatabaseReference::parse(&formatted).unwrap();

        assert_eq!(original.source, parsed.source);
        assert_eq!(original.dataset, parsed.dataset);
        assert_eq!(original.version, parsed.version);
        assert_eq!(original.profile, parsed.profile);
    }

    #[test]
    fn test_serialization() {
        // Test DatabaseSource serialization
        let source = DatabaseSource::UniProt(UniProtDatabase::SwissProt);
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: DatabaseSource = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", source), format!("{:?}", deserialized));

        // Test DatabaseSourceInfo serialization
        let info = DatabaseSourceInfo::new("uniprot", "swissprot");
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: DatabaseSourceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.source, deserialized.source);
        assert_eq!(info.dataset, deserialized.dataset);

        // Test DatabaseReference serialization
        let reference = DatabaseReference::with_all(
            "ncbi".to_string(),
            "nr".to_string(),
            Some("2024_04".to_string()),
            Some("blast".to_string()),
        );
        let json = serde_json::to_string(&reference).unwrap();
        let deserialized: DatabaseReference = serde_json::from_str(&json).unwrap();
        assert_eq!(reference.source, deserialized.source);
        assert_eq!(reference.dataset, deserialized.dataset);
        assert_eq!(reference.version, deserialized.version);
        assert_eq!(reference.profile, deserialized.profile);
    }
}