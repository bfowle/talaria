use anyhow::Result;

/// Parse a database reference in the format "source/dataset"
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
pub fn validate_source(source: &str) -> Result<&str> {
    match source.to_lowercase().as_str() {
        "uniprot" => Ok("uniprot"),
        "ncbi" => Ok("ncbi"),
        "pdb" => Ok("pdb"),
        "pfam" => Ok("pfam"),
        "kegg" => Ok("kegg"),
        "custom" => Ok("custom"),
        _ => anyhow::bail!("Unknown database source: {}. Valid sources: uniprot, ncbi, pdb, pfam, kegg, custom", source)
    }
}

/// Validate a dataset name for a given source
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