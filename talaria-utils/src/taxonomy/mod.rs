//! Unified taxonomy access utilities
//!
//! Provides a consistent interface for loading and accessing taxonomy data
//! across all Talaria components.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use talaria_core::system::paths;

/// Trait for types that can provide taxonomy data
///
/// This trait ensures consistent taxonomy loading across all commands
/// and prevents the proliferation of different taxonomy access patterns.
pub trait TaxonomyProvider {
    /// Check if taxonomy data is available
    fn has_taxonomy(&self) -> bool;

    /// Ensure taxonomy is available, returning error with download instructions if not
    fn require_taxonomy(&self) -> Result<()>;

    /// Get path to taxonomy tree directory (containing nodes.dmp, names.dmp)
    fn get_taxonomy_tree_path(&self) -> Result<PathBuf>;

    /// Get path to taxonomy mappings directory
    fn get_taxonomy_mappings_dir(&self) -> Result<PathBuf>;
}

/// Get the standard path to taxonomy tree files (nodes.dmp, names.dmp)
pub fn get_taxonomy_tree_path() -> PathBuf {
    paths::talaria_taxonomy_current_dir().join("tree")
}

/// Get the standard path to taxonomy mappings directory
pub fn get_taxonomy_mappings_dir() -> PathBuf {
    paths::talaria_taxonomy_current_dir().join("mappings")
}

/// Check if taxonomy data is available at the standard location
pub fn has_taxonomy() -> bool {
    let tree_path = get_taxonomy_tree_path();
    let nodes_file = tree_path.join("nodes.dmp");
    let names_file = tree_path.join("names.dmp");

    nodes_file.exists() && names_file.exists()
}

/// Ensure taxonomy is available, returning a helpful error if not
pub fn require_taxonomy() -> Result<()> {
    if !has_taxonomy() {
        let tree_path = get_taxonomy_tree_path();
        anyhow::bail!(
            "Taxonomy database not found.\n\
             \n\
             Expected location: {}\n\
             \n\
             Download with:\n\
             \x1b[1m  talaria database download ncbi/taxonomy\x1b[0m",
            tree_path.display()
        );
    }
    Ok(())
}

/// Get path to taxonomy mapping file for a specific database source
pub fn get_taxonomy_mapping_path(source_type: TaxonomyMappingSource) -> Result<PathBuf> {
    let mappings_dir = get_taxonomy_mappings_dir();

    let filename = match source_type {
        TaxonomyMappingSource::UniProt => "uniprot_idmapping.dat.gz",
        TaxonomyMappingSource::NCBI => "prot.accession2taxid.gz",
    };

    let path = mappings_dir.join(filename);

    if !path.exists() {
        anyhow::bail!(
            "Taxonomy mapping file not found: {}\n\
             \n\
             Download with:\n\
             \x1b[1m  talaria database download ncbi/prot-accession2taxid\x1b[0m",
            path.display()
        );
    }

    Ok(path)
}

/// Type of taxonomy mapping source
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaxonomyMappingSource {
    /// UniProt idmapping
    UniProt,
    /// NCBI accession2taxid
    NCBI,
}

/// Load taxonomy mappings from file
///
/// This is a unified implementation that handles both UniProt and NCBI formats
/// consistently across the codebase.
pub fn load_taxonomy_mappings<TaxonId>(
    source: TaxonomyMappingSource,
) -> Result<HashMap<String, TaxonId>>
where
    TaxonId: From<u32>,
{
    use flate2::read::GzDecoder;
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let mapping_file = get_taxonomy_mapping_path(source)?;
    let mut mappings = HashMap::new();

    let file = File::open(&mapping_file)
        .with_context(|| format!("Failed to open mapping file: {}", mapping_file.display()))?;

    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    match source {
        TaxonomyMappingSource::UniProt => {
            // UniProt idmapping format: accession<tab>type<tab>value
            // We're looking for: P12345<tab>NCBI_TaxID<tab>9606
            for line_result in reader.lines() {
                let line = line_result?;
                let parts: Vec<&str> = line.split('\t').collect();

                if parts.len() >= 3 && parts[1] == "NCBI_TaxID" {
                    if let Ok(taxid) = parts[2].parse::<u32>() {
                        mappings.insert(parts[0].to_string(), TaxonId::from(taxid));
                    }
                }
            }
        }
        TaxonomyMappingSource::NCBI => {
            // NCBI prot.accession2taxid format:
            // accession.version<tab>taxid<tab>gi
            let mut lines = reader.lines();
            lines.next(); // Skip header

            for line_result in lines {
                let line = line_result?;
                let parts: Vec<&str> = line.split('\t').collect();

                if parts.len() >= 2 {
                    if let Ok(taxid) = parts[1].parse::<u32>() {
                        let accession = parts[0].to_string();
                        mappings.insert(accession.clone(), TaxonId::from(taxid));

                        // Also store without version suffix
                        if let Some(dot_pos) = accession.rfind('.') {
                            mappings.insert(accession[..dot_pos].to_string(), TaxonId::from(taxid));
                        }
                    }
                }
            }
        }
    }

    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taxonomy_paths() {
        let tree_path = get_taxonomy_tree_path();
        assert!(tree_path.to_string_lossy().contains("taxonomy"));
        assert!(tree_path.to_string_lossy().contains("tree"));

        let mappings_dir = get_taxonomy_mappings_dir();
        assert!(mappings_dir.to_string_lossy().contains("taxonomy"));
        assert!(mappings_dir.to_string_lossy().contains("mappings"));
    }
}
