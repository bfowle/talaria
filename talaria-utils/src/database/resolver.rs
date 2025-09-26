#![allow(dead_code)]

/// Trait for flexible database path resolution and reference parsing
use anyhow::Result;
use std::path::PathBuf;

// DatabaseSource is now in talaria-core
use talaria_core::DatabaseSource;

// Re-export DatabaseReference from talaria-core
pub use talaria_core::types::DatabaseReference;

/// Paths for a database in the filesystem
#[derive(Debug, Clone)]
pub struct DatabasePaths {
    /// Root directory for this database version
    pub version_dir: PathBuf,
    /// Path to manifest file
    pub manifest_path: PathBuf,
    /// Directory containing chunks
    pub chunks_dir: PathBuf,
    /// Directory for metadata files
    pub metadata_dir: PathBuf,
    /// Directory for profiles/subsets
    pub profiles_dir: Option<PathBuf>,
    /// Directory for taxonomy data if applicable
    pub taxonomy_dir: Option<PathBuf>,
}

/// Trait for resolving database references to paths
pub trait DatabaseResolver: Send + Sync {
    /// Parse a database reference string
    fn parse_reference(&self, reference: &str) -> Result<DatabaseReference> {
        DatabaseReference::parse(reference)
    }

    /// Generate filesystem paths for a database reference
    fn resolve_paths(&self, reference: &DatabaseReference) -> Result<DatabasePaths>;

    /// Convert a DatabaseSource to DatabaseReference
    fn from_source(&self, source: &DatabaseSource) -> DatabaseReference;

    /// Validate that a reference is valid and exists
    fn validate(&self, reference: &DatabaseReference) -> Result<()>;

    /// Suggest corrections for invalid references
    fn suggest(&self, invalid: &str) -> Vec<String>;

    /// List available databases
    fn list_databases(&self) -> Result<Vec<DatabaseReference>>;

    /// Check if a database exists
    fn exists(&self, reference: &DatabaseReference) -> bool;

    /// Get the base path for all databases
    fn base_path(&self) -> &PathBuf;

    /// Normalize a reference (resolve aliases, etc.)
    fn normalize(&self, reference: &mut DatabaseReference) -> Result<()>;
}

/// Standard filesystem-based database resolver
pub struct StandardDatabaseResolver {
    base_path: PathBuf,
}

impl StandardDatabaseResolver {
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn get_version_dir(&self, reference: &DatabaseReference) -> PathBuf {
        let version = reference.version_or_default();

        self.base_path
            .join("versions")
            .join(&reference.source)
            .join(&reference.dataset)
            .join(version)
    }
}

impl DatabaseResolver for StandardDatabaseResolver {
    fn resolve_paths(&self, reference: &DatabaseReference) -> Result<DatabasePaths> {
        let version_dir = self.get_version_dir(reference);

        // Determine manifest extension based on what exists
        let manifest_path = if version_dir.join("manifest.tal").exists() {
            version_dir.join("manifest.tal")
        } else if version_dir.join("manifest.json").exists() {
            version_dir.join("manifest.json")
        } else {
            // Default to .tal for new manifests
            version_dir.join("manifest.tal")
        };

        let chunks_dir = self.base_path.join("chunks");
        let metadata_dir = version_dir.join("metadata");

        let profiles_dir = if reference.profile.is_some() {
            Some(
                version_dir
                    .join("profiles")
                    .join(reference.profile_or_default()),
            )
        } else {
            None
        };

        let taxonomy_dir = if reference.source == "ncbi" && reference.dataset == "taxonomy" {
            Some(self.base_path.join("taxonomy"))
        } else {
            None
        };

        Ok(DatabasePaths {
            version_dir,
            manifest_path,
            chunks_dir,
            metadata_dir,
            profiles_dir,
            taxonomy_dir,
        })
    }

    fn from_source(&self, source: &DatabaseSource) -> DatabaseReference {
        use talaria_core::{NCBIDatabase, UniProtDatabase};

        let (source_name, dataset) = match source {
            DatabaseSource::UniProt(UniProtDatabase::SwissProt) => ("uniprot", "swissprot"),
            DatabaseSource::UniProt(UniProtDatabase::TrEMBL) => ("uniprot", "trembl"),
            DatabaseSource::UniProt(UniProtDatabase::UniRef50) => ("uniprot", "uniref50"),
            DatabaseSource::UniProt(UniProtDatabase::UniRef90) => ("uniprot", "uniref90"),
            DatabaseSource::UniProt(UniProtDatabase::UniRef100) => ("uniprot", "uniref100"),
            DatabaseSource::UniProt(UniProtDatabase::IdMapping) => ("uniprot", "idmapping"),
            DatabaseSource::NCBI(NCBIDatabase::NR) => ("ncbi", "nr"),
            DatabaseSource::NCBI(NCBIDatabase::NT) => ("ncbi", "nt"),
            DatabaseSource::NCBI(NCBIDatabase::RefSeq) => ("ncbi", "refseq"),
            DatabaseSource::NCBI(NCBIDatabase::RefSeqProtein) => ("ncbi", "refseq-protein"),
            DatabaseSource::NCBI(NCBIDatabase::RefSeqGenomic) => ("ncbi", "refseq-genomic"),
            DatabaseSource::NCBI(NCBIDatabase::Taxonomy) => ("ncbi", "taxonomy"),
            DatabaseSource::NCBI(NCBIDatabase::ProtAccession2TaxId) => {
                ("ncbi", "prot-accession2taxid")
            }
            DatabaseSource::NCBI(NCBIDatabase::NuclAccession2TaxId) => {
                ("ncbi", "nucl-accession2taxid")
            }
            DatabaseSource::NCBI(NCBIDatabase::GenBank) => ("ncbi", "genbank"),
            DatabaseSource::Custom(name) => ("custom", name.as_str()),
            DatabaseSource::Test => ("test", "test"),
        };

        DatabaseReference {
            source: source_name.to_string(),
            dataset: dataset.to_string(),
            version: None,
            profile: None,
        }
    }

    fn validate(&self, reference: &DatabaseReference) -> Result<()> {
        // Check source is valid
        let valid_sources = vec!["uniprot", "ncbi", "custom"];
        if !valid_sources.contains(&reference.source.as_str()) {
            anyhow::bail!(
                "Invalid source: {}. Valid sources: {:?}",
                reference.source,
                valid_sources
            );
        }

        // Check dataset is valid for source
        match reference.source.as_str() {
            "uniprot" => {
                let valid = vec![
                    "swissprot",
                    "trembl",
                    "uniref50",
                    "uniref90",
                    "uniref100",
                    "idmapping",
                ];
                if !valid.contains(&reference.dataset.as_str()) {
                    anyhow::bail!(
                        "Invalid UniProt dataset: {}. Valid: {:?}",
                        reference.dataset,
                        valid
                    );
                }
            }
            "ncbi" => {
                let valid = vec![
                    "nr",
                    "nt",
                    "refseq-protein",
                    "refseq-genomic",
                    "taxonomy",
                    "prot-accession2taxid",
                    "nucl-accession2taxid",
                ];
                if !valid.contains(&reference.dataset.as_str()) {
                    anyhow::bail!(
                        "Invalid NCBI dataset: {}. Valid: {:?}",
                        reference.dataset,
                        valid
                    );
                }
            }
            "custom" => {
                // Any dataset name is valid for custom
            }
            _ => {}
        }

        // Check if version format is valid
        if let Some(ref version) = reference.version {
            if version != "current" && version != "latest" && version != "stable" {
                // Check if it's a timestamp version
                if version.len() == 15 && version.chars().nth(8) == Some('_') {
                    // Validate timestamp format
                    if !version[0..8].chars().all(|c| c.is_ascii_digit())
                        || !version[9..15].chars().all(|c| c.is_ascii_digit())
                    {
                        anyhow::bail!("Invalid timestamp version format: {}", version);
                    }
                }
                // Could be upstream version like "2024_04"
            }
        }

        Ok(())
    }

    fn suggest(&self, invalid: &str) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Common typos and corrections
        let corrections = vec![
            ("swissprot", vec!["uniprot/swissprot"]),
            ("swiss", vec!["uniprot/swissprot"]),
            ("sprot", vec!["uniprot/swissprot"]),
            ("trembl", vec!["uniprot/trembl"]),
            ("nr", vec!["ncbi/nr"]),
            ("nt", vec!["ncbi/nt"]),
            ("refseq", vec!["ncbi/refseq-protein", "ncbi/refseq-genomic"]),
            ("taxonomy", vec!["ncbi/taxonomy"]),
            ("taxon", vec!["ncbi/taxonomy"]),
            (
                "uniref",
                vec!["uniprot/uniref50", "uniprot/uniref90", "uniprot/uniref100"],
            ),
        ];

        let lower = invalid.to_lowercase();
        for (pattern, suggs) in corrections {
            if lower.contains(pattern) {
                suggestions.extend(suggs.iter().map(|s| s.to_string()));
            }
        }

        // If no specific suggestions, provide common databases
        if suggestions.is_empty() {
            suggestions = vec![
                "uniprot/swissprot".to_string(),
                "uniprot/trembl".to_string(),
                "ncbi/nr".to_string(),
                "ncbi/nt".to_string(),
            ];
        }

        suggestions
    }

    fn list_databases(&self) -> Result<Vec<DatabaseReference>> {
        let mut databases = Vec::new();
        let versions_dir = self.base_path.join("versions");

        if !versions_dir.exists() {
            return Ok(databases);
        }

        // Iterate through source directories
        for source_entry in std::fs::read_dir(&versions_dir)? {
            let source_entry = source_entry?;
            let source_path = source_entry.path();

            if !source_path.is_dir() {
                continue;
            }

            let source = source_entry.file_name().to_str().unwrap_or("").to_string();

            // Iterate through dataset directories
            for dataset_entry in std::fs::read_dir(&source_path)? {
                let dataset_entry = dataset_entry?;
                let dataset_path = dataset_entry.path();

                if !dataset_path.is_dir() {
                    continue;
                }

                let dataset = dataset_entry.file_name().to_str().unwrap_or("").to_string();

                databases.push(DatabaseReference {
                    source: source.clone(),
                    dataset,
                    version: None,
                    profile: None,
                });
            }
        }

        Ok(databases)
    }

    fn exists(&self, reference: &DatabaseReference) -> bool {
        let paths = match self.resolve_paths(reference) {
            Ok(p) => p,
            Err(_) => return false,
        };

        paths.version_dir.exists()
            || (reference.version.is_none()
                && self
                    .base_path
                    .join("versions")
                    .join(&reference.source)
                    .join(&reference.dataset)
                    .exists())
    }

    fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    fn normalize(&self, reference: &mut DatabaseReference) -> Result<()> {
        // Resolve "latest" to "current"
        if reference.version.as_deref() == Some("latest") {
            reference.version = Some("current".to_string());
        }

        // Validate the normalized reference
        self.validate(reference)?;

        Ok(())
    }
}
