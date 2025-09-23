use anyhow::Result;
use serde::{Deserialize, Serialize};

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
    #[allow(dead_code)]
    pub fn new(source: String, dataset: String) -> Self {
        Self {
            source,
            dataset,
            version: None,
            profile: None,
        }
    }

    /// Create with all fields
    #[allow(dead_code)]
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
}

impl std::fmt::Display for DatabaseReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut result = format!("{}/{}", self.source, self.dataset);

        if let Some(ref version) = self.version {
            result.push('@');
            result.push_str(version);
        }

        if let Some(ref profile) = self.profile {
            result.push(':');
            result.push_str(profile);
        }

        write!(f, "{}", result)
    }
}

/// Parse a database reference with full syntax support
///
/// Format: `source/dataset[@version][:profile]`
///
/// # Examples
/// - "uniprot/swissprot" -> defaults to current version, auto-detect profile
/// - "uniprot/swissprot@2024_04" -> specific version, auto-detect profile
/// - "uniprot/swissprot:50-percent" -> current version, 50% reduction
/// - "uniprot/swissprot@2024_04:50-percent" -> specific version and profile
/// - "ncbi/nr@stable" -> stable alias
/// - "custom/mydb@paper-2024:minimal" -> custom alias and profile
pub fn parse_database_reference(input: &str) -> Result<DatabaseReference> {
    // First, find the base reference (source/dataset)
    let version_split: Vec<&str> = input.splitn(2, '@').collect();
    let base_and_profile = version_split[0];
    let version = version_split.get(1);

    // Now split the base_and_profile by ':' to separate base from profile
    let profile_split: Vec<&str> = base_and_profile.splitn(2, ':').collect();
    let base = profile_split[0];
    let mut profile = profile_split.get(1).map(|s| s.to_string());

    // If we have a version part, it might also contain a profile after ':'
    let mut final_version = None;
    if let Some(version_part) = version {
        let version_profile_split: Vec<&str> = version_part.splitn(2, ':').collect();
        final_version = Some(version_profile_split[0].to_string());

        // Profile in version part takes precedence
        if let Some(profile_in_version) = version_profile_split.get(1) {
            profile = Some(profile_in_version.to_string());
        }
    }

    // Parse the base reference (source/dataset)
    let (source, dataset) = parse_database_ref(base)?;

    Ok(DatabaseReference {
        source,
        dataset,
        version: final_version,
        profile,
    })
}

/// Parse a database reference in the format "source/dataset"
/// This is the legacy function for backward compatibility
///
/// # Examples
/// - "uniprot/swissprot" -> ("uniprot", "swissprot")
/// - "ncbi/nr" -> ("ncbi", "nr")
/// - "custom/mydb" -> ("custom", "mydb")
pub fn parse_database_ref(input: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = input.split('/').collect();

    if parts.len() != 2 {
        anyhow::bail!(
            "Invalid database reference '{}'. Expected format: source/dataset (e.g., uniprot/swissprot)",
            input
        );
    }

    let source = parts[0].trim();
    let dataset = parts[1].trim();

    if source.is_empty() || dataset.is_empty() {
        anyhow::bail!(
            "Invalid database reference '{}'. Both source and dataset must be non-empty",
            input
        );
    }

    Ok((source.to_string(), dataset.to_string()))
}

/// Validate and normalize a database source name
#[allow(dead_code)]
pub fn validate_source(source: &str) -> Result<&str> {
    match source.to_lowercase().as_str() {
        "uniprot" => Ok("uniprot"),
        "ncbi" => Ok("ncbi"),
        "pdb" => Ok("pdb"),
        "pfam" => Ok("pfam"),
        "kegg" => Ok("kegg"),
        "custom" => Ok("custom"),
        _ => anyhow::bail!(
            "Unknown database source: {}. Valid sources: uniprot, ncbi, pdb, pfam, kegg, custom",
            source
        ),
    }
}

/// Validate a dataset name for a given source
#[allow(dead_code)]
pub fn validate_dataset(source: &str, dataset: &str) -> Result<()> {
    match source {
        "uniprot" => match dataset {
            "swissprot" | "trembl" | "uniref50" | "uniref90" | "uniref100" | "idmapping" => Ok(()),
            _ => anyhow::bail!("Invalid UniProt dataset: {}. Valid options: swissprot, trembl, uniref50, uniref90, uniref100, idmapping", dataset)
        },
        "ncbi" => match dataset {
            "nr" | "nt" | "refseq-protein" | "refseq-genomic" | "taxonomy" |
            "prot-accession2taxid" | "nucl-accession2taxid" => Ok(()),
            _ => anyhow::bail!("Invalid NCBI dataset: {}. Valid options: nr, nt, refseq-protein, refseq-genomic, taxonomy, prot-accession2taxid, nucl-accession2taxid", dataset)
        },
        "custom" => Ok(()), // Allow any dataset name for custom databases
        _ => Ok(()), // For other sources, accept any dataset for now
    }
}

/// Format a database reference from source and dataset
#[allow(dead_code)]
pub fn format_database_ref(source: &str, dataset: &str) -> String {
    format!("{}/{}", source, dataset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_database_ref() {
        assert_eq!(
            parse_database_ref("uniprot/swissprot").unwrap(),
            ("uniprot".to_string(), "swissprot".to_string())
        );

        assert_eq!(
            parse_database_ref("ncbi/nr").unwrap(),
            ("ncbi".to_string(), "nr".to_string())
        );

        assert!(parse_database_ref("invalid").is_err());
        assert!(parse_database_ref("too/many/slashes").is_err());
        assert!(parse_database_ref("/empty").is_err());
        assert!(parse_database_ref("empty/").is_err());
    }

    #[test]
    fn test_parse_database_reference_full() {
        // Test basic reference
        let ref1 = parse_database_reference("uniprot/swissprot").unwrap();
        assert_eq!(ref1.source, "uniprot");
        assert_eq!(ref1.dataset, "swissprot");
        assert_eq!(ref1.version, None);
        assert_eq!(ref1.profile, None);

        // Test with version
        let ref2 = parse_database_reference("uniprot/swissprot@2024_04").unwrap();
        assert_eq!(ref2.source, "uniprot");
        assert_eq!(ref2.dataset, "swissprot");
        assert_eq!(ref2.version, Some("2024_04".to_string()));
        assert_eq!(ref2.profile, None);

        // Test with profile
        let ref3 = parse_database_reference("uniprot/swissprot:50-percent").unwrap();
        assert_eq!(ref3.source, "uniprot");
        assert_eq!(ref3.dataset, "swissprot");
        assert_eq!(ref3.version, None);
        assert_eq!(ref3.profile, Some("50-percent".to_string()));

        // Test with both version and profile
        let ref4 = parse_database_reference("uniprot/swissprot@2024_04:50-percent").unwrap();
        assert_eq!(ref4.source, "uniprot");
        assert_eq!(ref4.dataset, "swissprot");
        assert_eq!(ref4.version, Some("2024_04".to_string()));
        assert_eq!(ref4.profile, Some("50-percent".to_string()));

        // Test with aliases
        let ref5 = parse_database_reference("ncbi/nr@stable:minimal").unwrap();
        assert_eq!(ref5.source, "ncbi");
        assert_eq!(ref5.dataset, "nr");
        assert_eq!(ref5.version, Some("stable".to_string()));
        assert_eq!(ref5.profile, Some("minimal".to_string()));
    }

    #[test]
    fn test_database_reference_display() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        assert_eq!(ref1.to_string(), "uniprot/swissprot");

        let ref2 = DatabaseReference::with_all(
            "uniprot".to_string(),
            "swissprot".to_string(),
            Some("2024_04".to_string()),
            Some("50-percent".to_string()),
        );
        assert_eq!(ref2.to_string(), "uniprot/swissprot@2024_04:50-percent");
    }

    #[test]
    fn test_database_reference_defaults() {
        let ref1 = DatabaseReference::new("uniprot".to_string(), "swissprot".to_string());
        assert_eq!(ref1.version_or_default(), "current");
        assert_eq!(ref1.profile_or_default(), "auto-detect");
    }

    #[test]
    fn test_validate_source() {
        assert_eq!(validate_source("uniprot").unwrap(), "uniprot");
        assert_eq!(validate_source("UNIPROT").unwrap(), "uniprot");
        assert_eq!(validate_source("UniProt").unwrap(), "uniprot");
        assert!(validate_source("invalid").is_err());
    }

    #[test]
    fn test_validate_dataset() {
        assert!(validate_dataset("uniprot", "swissprot").is_ok());
        assert!(validate_dataset("uniprot", "invalid").is_err());
        assert!(validate_dataset("ncbi", "nr").is_ok());
        assert!(validate_dataset("custom", "anything").is_ok());
    }
}
