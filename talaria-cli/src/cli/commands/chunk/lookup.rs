use crate::cli::formatting::output::*;
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use talaria_sequoia::SHA256Hash;

/// Trait for different lookup strategies
pub trait ChunkLookupStrategy {
    /// Perform the lookup and return matching chunk hashes
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>>;

    /// Get a description of this lookup strategy for display
    fn description(&self) -> String;
}

/// Trait for chunk information display
pub trait ChunkDisplay {
    /// Display chunk information in human-readable format
    fn display(&self, detailed: bool);

    /// Export as JSON
    fn to_json(&self) -> serde_json::Value;

    /// Export as CSV row
    fn to_csv_row(&self) -> Vec<String>;
}

#[derive(Args)]
pub struct LookupArgs {
    /// Chunk hash (SHA256) to look up
    #[arg(long, value_name = "HASH")]
    pub hash: Option<String>,

    /// Taxonomy ID to search for
    #[arg(long, value_name = "TAXID")]
    pub taxid: Option<u32>,

    /// Accession number to search for
    #[arg(long, value_name = "ACCESSION")]
    pub accession: Option<String>,

    /// Organism name to search for
    #[arg(long, value_name = "NAME")]
    pub organism: Option<String>,

    /// Database to search in (e.g., "uniprot/swissprot")
    #[arg(long, value_name = "DATABASE")]
    pub database: Option<String>,

    /// Minimum number of sequences in chunk
    #[arg(long)]
    pub min_sequences: Option<usize>,

    /// Maximum chunk size in MB
    #[arg(long)]
    pub max_size: Option<u64>,

    /// Output format (text, json, csv)
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,

    /// Show detailed information
    #[arg(long)]
    pub detailed: bool,

    /// Show statistics about all chunks
    #[arg(long)]
    pub stats: bool,

    /// Export matching chunks as manifest
    #[arg(long)]
    pub export_manifest: Option<PathBuf>,
}

// Use OutputFormat from talaria-core
use talaria_core::OutputFormat;

/// Information about a chunk for display
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChunkDisplayInfo {
    pub hash: SHA256Hash,
    pub database: String,
    pub version: String,
    pub taxonomy: Vec<TaxonomyInfo>,
    pub sequence_count: usize,
    pub size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f32,
    pub reference_sequences: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaxonomyInfo {
    pub taxid: u32,
    pub name: String,
    pub count: usize,
    pub percentage: f32,
}

#[derive(Debug)]
pub struct ChunkMatch {
    pub chunk: ChunkDisplayInfo,
    pub match_reason: String,
    pub relevance_score: f32,
}

/// Index for fast chunk lookups
pub struct ChunkIndex {
    by_hash: std::collections::HashMap<SHA256Hash, ChunkDisplayInfo>,
    by_taxid: std::collections::HashMap<u32, Vec<SHA256Hash>>,
    by_accession: std::collections::HashMap<String, SHA256Hash>,
    by_database: std::collections::HashMap<String, std::collections::HashSet<SHA256Hash>>,
}

// Implement lookup strategies
struct HashLookup(SHA256Hash);
struct TaxidLookup(u32);
struct AccessionLookup(String);
struct OrganismLookup(String);
struct DatabaseLookup(String);

impl ChunkLookupStrategy for HashLookup {
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>> {
        if let Some(chunk) = index.by_hash.get(&self.0) {
            Ok(vec![ChunkMatch {
                chunk: chunk.clone(),
                match_reason: "Exact hash match".to_string(),
                relevance_score: 1.0,
            }])
        } else {
            Ok(vec![])
        }
    }

    fn description(&self) -> String {
        format!("Looking up chunk by hash: {}", self.0)
    }
}

impl ChunkLookupStrategy for TaxidLookup {
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>> {
        let mut matches = Vec::new();

        if let Some(hashes) = index.by_taxid.get(&self.0) {
            for hash in hashes {
                if let Some(chunk) = index.by_hash.get(hash) {
                    // Calculate relevance based on how much of the chunk is this taxid
                    let taxid_percentage = chunk
                        .taxonomy
                        .iter()
                        .find(|t| t.taxid == self.0)
                        .map(|t| t.percentage)
                        .unwrap_or(0.0);

                    matches.push(ChunkMatch {
                        chunk: chunk.clone(),
                        match_reason: format!("Contains TaxID {}", self.0),
                        relevance_score: taxid_percentage / 100.0,
                    });
                }
            }
        }

        // Sort by relevance
        matches.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        Ok(matches)
    }

    fn description(&self) -> String {
        format!("Looking up chunks containing TaxID {}", self.0)
    }
}

impl ChunkLookupStrategy for AccessionLookup {
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>> {
        if let Some(hash) = index.by_accession.get(&self.0) {
            if let Some(chunk) = index.by_hash.get(hash) {
                return Ok(vec![ChunkMatch {
                    chunk: chunk.clone(),
                    match_reason: format!("Contains accession {}", self.0),
                    relevance_score: 1.0,
                }]);
            }
        }
        Ok(vec![])
    }

    fn description(&self) -> String {
        format!("Looking up chunk containing accession {}", self.0)
    }
}

impl ChunkLookupStrategy for OrganismLookup {
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>> {
        let mut matches = Vec::new();
        let search_term = self.0.to_lowercase();

        // Search through all chunks for matching organism names
        for chunk in index.by_hash.values() {
            for tax_info in &chunk.taxonomy {
                if tax_info.name.to_lowercase().contains(&search_term) {
                    matches.push(ChunkMatch {
                        chunk: chunk.clone(),
                        match_reason: format!("Contains organism: {}", tax_info.name),
                        relevance_score: tax_info.percentage / 100.0,
                    });
                    break; // Only add each chunk once
                }
            }
        }

        // Sort by relevance
        matches.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        Ok(matches)
    }

    fn description(&self) -> String {
        format!("Looking up chunks containing organism name: {}", self.0)
    }
}

impl ChunkLookupStrategy for DatabaseLookup {
    fn lookup(&self, index: &ChunkIndex) -> Result<Vec<ChunkMatch>> {
        let mut matches = Vec::new();

        if let Some(hashes) = index.by_database.get(&self.0) {
            for hash in hashes {
                if let Some(chunk) = index.by_hash.get(hash) {
                    matches.push(ChunkMatch {
                        chunk: chunk.clone(),
                        match_reason: format!("Part of database: {}", self.0),
                        relevance_score: 1.0,
                    });
                }
            }
        }

        Ok(matches)
    }

    fn description(&self) -> String {
        format!("Looking up all chunks in database: {}", self.0)
    }
}

impl ChunkDisplay for ChunkDisplayInfo {
    fn display(&self, detailed: bool) {
        section_header("Chunk Information");

        tree_item(false, "Hash", Some(&self.hash.to_string()));
        tree_item(false, "Database", Some(&self.database));
        tree_item(false, "Version", Some(&self.version));
        tree_item(
            false,
            "Sequences",
            Some(&format_number(self.sequence_count)),
        );
        tree_item(
            false,
            "Size",
            Some(&format!("{:.1} MB", self.size as f64 / 1_048_576.0)),
        );
        tree_item(
            false,
            "Compressed",
            Some(&format!(
                "{:.1} MB",
                self.compressed_size as f64 / 1_048_576.0
            )),
        );
        tree_item(
            false,
            "Compression Ratio",
            Some(&format!("{:.2}x", self.compression_ratio)),
        );

        if detailed {
            subsection_header("Taxonomic Distribution");
            for tax in &self.taxonomy {
                tree_item(
                    false,
                    &format!("{} (TaxID: {})", tax.name, tax.taxid),
                    Some(&format!("{} sequences ({:.1}%)", tax.count, tax.percentage)),
                );
            }

            if !self.reference_sequences.is_empty() {
                subsection_header("Representative Sequences");
                for (i, seq) in self.reference_sequences.iter().take(5).enumerate() {
                    tree_item(i == self.reference_sequences.len() - 1, seq, None);
                }
                if self.reference_sequences.len() > 5 {
                    info(&format!(
                        "... and {} more",
                        self.reference_sequences.len() - 5
                    ));
                }
            }
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.hash.to_string(),
            self.database.clone(),
            self.version.clone(),
            self.sequence_count.to_string(),
            self.size.to_string(),
            self.compressed_size.to_string(),
            format!("{:.2}", self.compression_ratio),
        ]
    }
}

impl ChunkDisplay for ChunkMatch {
    fn display(&self, detailed: bool) {
        self.chunk.display(detailed);
        tree_item(false, "Match Reason", Some(&self.match_reason));
        tree_item(
            false,
            "Relevance",
            Some(&format!("{:.0}%", self.relevance_score * 100.0)),
        );
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "chunk": self.chunk.to_json(),
            "match_reason": self.match_reason,
            "relevance_score": self.relevance_score,
        })
    }

    fn to_csv_row(&self) -> Vec<String> {
        let mut row = self.chunk.to_csv_row();
        row.push(self.match_reason.clone());
        row.push(format!("{:.2}", self.relevance_score));
        row
    }
}

pub fn run(args: LookupArgs) -> Result<()> {
    // Parse database@version if provided
    let (database_filter, version_filter) = if let Some(database) = &args.database {
        if let Some(at_pos) = database.find('@') {
            let (db, ver) = database.split_at(at_pos);
            (Some(db.to_string()), Some(ver[1..].to_string()))
        } else {
            (Some(database.clone()), None)
        }
    } else {
        (None, None)
    };

    // Build chunk index (with optional version filter)
    let index = if version_filter.is_some() {
        build_chunk_index_for_version(database_filter.as_deref(), version_filter.as_deref())?
    } else {
        build_chunk_index()?
    };

    // Show statistics if requested
    if args.stats {
        show_chunk_statistics(&index);
        return Ok(());
    }

    // Determine lookup strategy based on arguments
    let strategy: Box<dyn ChunkLookupStrategy> = if let Some(hash_str) = &args.hash {
        let hash = SHA256Hash::from_hex(hash_str).context("Invalid SHA256 hash")?;
        Box::new(HashLookup(hash))
    } else if let Some(taxid) = args.taxid {
        Box::new(TaxidLookup(taxid))
    } else if let Some(accession) = &args.accession {
        Box::new(AccessionLookup(accession.clone()))
    } else if let Some(organism) = &args.organism {
        Box::new(OrganismLookup(organism.clone()))
    } else if let Some(db_filter) = database_filter {
        Box::new(DatabaseLookup(db_filter))
    } else {
        anyhow::bail!("Please specify a lookup criterion (--hash, --taxid, --accession, --organism, or --database)");
    };

    // Perform lookup
    info(&strategy.description());
    let mut matches = strategy.lookup(&index)?;

    // Apply filters
    if let Some(min_seq) = args.min_sequences {
        matches.retain(|m| m.chunk.sequence_count >= min_seq);
    }

    if let Some(max_size) = args.max_size {
        let max_bytes = max_size * 1_048_576;
        matches.retain(|m| m.chunk.size <= max_bytes);
    }

    // Display results
    if matches.is_empty() {
        warning("No matching chunks found");
        return Ok(());
    }

    success(&format!("Found {} matching chunk(s)", matches.len()));

    match args.format {
        OutputFormat::Text => {
            for (i, m) in matches.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                m.display(args.detailed);
            }
        }
        OutputFormat::Json => {
            let json = serde_json::json!({
                "matches": matches.iter().map(|m| m.to_json()).collect::<Vec<_>>(),
                "count": matches.len(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Csv => {
            // Print header
            println!("hash,database,version,sequences,size,compressed_size,compression_ratio,match_reason,relevance");
            for m in &matches {
                println!("{}", m.to_csv_row().join(","));
            }
        }
        OutputFormat::HashOnly => {
            for m in &matches {
                println!("{}", m.chunk.hash);
            }
        }
        OutputFormat::Yaml
        | OutputFormat::Tsv
        | OutputFormat::Fasta
        | OutputFormat::Summary
        | OutputFormat::Detailed => {
            // Default to text output for unsupported formats
            for (i, m) in matches.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                m.display(args.detailed);
            }
        }
    }

    // Export manifest if requested
    if let Some(manifest_path) = args.export_manifest {
        export_chunk_manifest(&matches, &manifest_path)?;
        success(&format!("Exported manifest to {}", manifest_path.display()));
    }

    Ok(())
}

fn build_chunk_index_for_version(
    database: Option<&str>,
    version: Option<&str>,
) -> Result<ChunkIndex> {
    use glob::glob;
    use talaria_core::system::paths;
    use talaria_sequoia::Manifest;

    let databases_dir = paths::talaria_databases_dir();
    action(&format!(
        "Building chunk index for {}@{}...",
        database.unwrap_or("*"),
        version.unwrap_or("*")
    ));

    let mut index = ChunkIndex {
        by_hash: std::collections::HashMap::new(),
        by_taxid: std::collections::HashMap::new(),
        by_accession: std::collections::HashMap::new(),
        by_database: std::collections::HashMap::new(),
    };

    // Build specific pattern for version lookup
    let patterns = if let (Some(db), Some(ver)) = (database, version) {
        // Parse database into provider/name
        let parts: Vec<&str> = db.split('/').collect();
        if parts.len() == 2 {
            vec![
                format!(
                    "{}/versions/{}/{}/{}/manifest.tal",
                    databases_dir.display(),
                    parts[0],
                    parts[1],
                    ver
                ),
                format!(
                    "{}/versions/{}/{}/{}/manifest.json",
                    databases_dir.display(),
                    parts[0],
                    parts[1],
                    ver
                ),
            ]
        } else {
            // Try to find it in any provider
            vec![
                format!(
                    "{}/versions/*/{}/{}/manifest.tal",
                    databases_dir.display(),
                    db,
                    ver
                ),
                format!(
                    "{}/versions/*/{}/{}/manifest.json",
                    databases_dir.display(),
                    db,
                    ver
                ),
            ]
        }
    } else {
        // Should not happen, but handle gracefully
        return build_chunk_index();
    };

    for pattern in &patterns {
        if let Ok(paths) = glob(pattern) {
            for path in paths.flatten() {
                // Try to load the manifest
                match Manifest::load_file(&path) {
                    Ok(manifest) => {
                        // Get database name from path
                        let database_name = extract_database_name(&path);

                        // Process chunks from manifest
                        if let Some(temporal_manifest) = manifest.get_data() {
                            for chunk_meta in &temporal_manifest.chunk_index {
                                // Create ChunkInfo from metadata
                                let chunk_info = ChunkDisplayInfo {
                                    hash: chunk_meta.hash.clone(),
                                    database: database_name.clone(),
                                    version: temporal_manifest.version.clone(),
                                    taxonomy: extract_taxonomy_info(chunk_meta),
                                    sequence_count: chunk_meta.sequence_count,
                                    size: chunk_meta.size as u64,
                                    compressed_size: chunk_meta
                                        .compressed_size
                                        .unwrap_or(chunk_meta.size)
                                        as u64,
                                    compression_ratio: if let Some(compressed) =
                                        chunk_meta.compressed_size
                                    {
                                        chunk_meta.size as f32 / compressed as f32
                                    } else {
                                        1.0
                                    },
                                    reference_sequences: Vec::new(), // Not available in ManifestMetadata
                                    created_at: temporal_manifest.created_at.to_rfc3339(),
                                };

                                // Index by hash
                                let hash = chunk_meta.hash.clone();
                                index.by_hash.insert(hash.clone(), chunk_info.clone());

                                // Index by database
                                index
                                    .by_database
                                    .entry(database_name.clone())
                                    .or_default()
                                    .insert(hash.clone());

                                // Index by taxonomy
                                for tax_info in &chunk_info.taxonomy {
                                    index
                                        .by_taxid
                                        .entry(tax_info.taxid)
                                        .or_default()
                                        .push(hash.clone());
                                }

                                // Note: Accession indexing would require loading full chunk data
                            }
                        }
                    }
                    Err(_) => {
                        // Skip manifests we can't read
                        continue;
                    }
                }
            }
        }
    }

    Ok(index)
}

fn build_chunk_index() -> Result<ChunkIndex> {
    use glob::glob;
    use talaria_core::system::paths;
    use talaria_sequoia::Manifest;

    let databases_dir = paths::talaria_databases_dir();
    action("Building chunk index from local manifests...");

    let mut index = ChunkIndex {
        by_hash: std::collections::HashMap::new(),
        by_taxid: std::collections::HashMap::new(),
        by_accession: std::collections::HashMap::new(),
        by_database: std::collections::HashMap::new(),
    };

    // Find manifest files only in current versions
    // Pattern for versioned databases: versions/{provider}/{db}/current/manifest.*
    let pattern1 = format!(
        "{}/versions/*/*/current/manifest.tal",
        databases_dir.display()
    );
    let pattern2 = format!(
        "{}/versions/*/*/current/manifest.json",
        databases_dir.display()
    );
    // Also check taxonomy directory (no versioning)
    let pattern3 = format!("{}/taxonomy/*/manifest.tal", databases_dir.display());
    let pattern4 = format!("{}/taxonomy/*/manifest.json", databases_dir.display());

    for pattern in &[pattern1, pattern2, pattern3, pattern4] {
        if let Ok(paths) = glob(pattern) {
            for path in paths.flatten() {
                // Try to load the manifest
                match Manifest::load_file(&path) {
                    Ok(manifest) => {
                        // Get database name from path
                        let database_name = extract_database_name(&path);

                        // Process chunks from manifest
                        if let Some(temporal_manifest) = manifest.get_data() {
                            for chunk_meta in &temporal_manifest.chunk_index {
                                // Create ChunkInfo from metadata
                                let chunk_info = ChunkDisplayInfo {
                                    hash: chunk_meta.hash.clone(),
                                    database: database_name.clone(),
                                    version: temporal_manifest.version.clone(),
                                    taxonomy: extract_taxonomy_info(chunk_meta),
                                    sequence_count: chunk_meta.sequence_count,
                                    size: chunk_meta.size as u64,
                                    compressed_size: chunk_meta
                                        .compressed_size
                                        .unwrap_or(chunk_meta.size)
                                        as u64,
                                    compression_ratio: if let Some(compressed) =
                                        chunk_meta.compressed_size
                                    {
                                        chunk_meta.size as f32 / compressed as f32
                                    } else {
                                        1.0
                                    },
                                    reference_sequences: Vec::new(), // Not available in ManifestMetadata
                                    created_at: temporal_manifest.created_at.to_rfc3339(),
                                };

                                // Index by hash
                                let hash = chunk_meta.hash.clone();
                                index.by_hash.insert(hash.clone(), chunk_info.clone());

                                // Index by database (use HashSet to avoid duplicates)
                                index
                                    .by_database
                                    .entry(database_name.clone())
                                    .or_default()
                                    .insert(hash.clone());

                                // Index by taxonomy
                                for tax_info in &chunk_info.taxonomy {
                                    index
                                        .by_taxid
                                        .entry(tax_info.taxid)
                                        .or_default()
                                        .push(hash.clone());
                                }

                                // Note: Accession indexing would require loading full chunk data
                            }
                        }
                    }
                    Err(_) => {
                        // Skip manifests we can't read
                        continue;
                    }
                }
            }
        }
    }

    Ok(index)
}

fn extract_database_name(path: &std::path::Path) -> String {
    // Extract database name from path like ~/.talaria/databases/versions/custom/cholera/20250918_040507/manifest.tal
    let components: Vec<_> = path.components().collect();

    for (i, comp) in components.iter().enumerate() {
        if comp.as_os_str() == "databases" {
            // Look for pattern after databases/
            if i + 2 < components.len() {
                if let Some(db_type) = components[i + 1].as_os_str().to_str() {
                    if db_type == "versions" && i + 3 < components.len() {
                        // databases/versions/{provider}/{database}
                        if let (Some(provider), Some(db_name)) = (
                            components[i + 2].as_os_str().to_str(),
                            components[i + 3].as_os_str().to_str(),
                        ) {
                            return format!("{}/{}", provider, db_name);
                        }
                    } else if db_type == "taxonomy" {
                        return "taxonomy".to_string();
                    }
                }
            }
        }
    }

    "unknown".to_string()
}

fn extract_taxonomy_info(chunk_meta: &talaria_sequoia::ManifestMetadata) -> Vec<TaxonomyInfo> {
    let mut taxonomy = Vec::new();

    // Extract unique taxon IDs from chunk
    let mut taxid_counts = std::collections::HashMap::new();
    for taxon_id in &chunk_meta.taxon_ids {
        *taxid_counts.entry(taxon_id.0).or_insert(0) += 1;
    }

    // Convert to TaxonomyInfo
    let total_refs = chunk_meta.taxon_ids.len();
    for (taxid, count) in taxid_counts {
        taxonomy.push(TaxonomyInfo {
            taxid,
            name: format!("TaxID {}", taxid), // Could look up actual name from taxonomy DB
            count,
            percentage: if total_refs > 0 {
                (count as f32 / total_refs as f32) * 100.0
            } else {
                0.0
            },
        });
    }

    taxonomy.sort_by(|a, b| b.count.cmp(&a.count));
    taxonomy
}

fn show_chunk_statistics(index: &ChunkIndex) {
    section_header("Chunk Repository Statistics");

    let total_chunks = index.by_hash.len();
    let total_size: u64 = index.by_hash.values().map(|c| c.size).sum();
    let total_compressed: u64 = index.by_hash.values().map(|c| c.compressed_size).sum();
    let avg_compression = if total_size > 0 {
        total_size as f64 / total_compressed as f64
    } else {
        0.0
    };

    tree_item(false, "Total chunks", Some(&format_number(total_chunks)));
    tree_item(
        false,
        "Total size",
        Some(&format!("{:.1} GB", total_size as f64 / 1_073_741_824.0)),
    );
    tree_item(
        false,
        "Compressed size",
        Some(&format!(
            "{:.1} GB",
            total_compressed as f64 / 1_073_741_824.0
        )),
    );
    tree_item(
        false,
        "Average compression",
        Some(&format!("{:.2}x", avg_compression)),
    );

    // Group by database
    subsection_header("Chunks by Database");
    for (db, chunks) in &index.by_database {
        tree_item(false, db, Some(&format!("{} chunks", chunks.len())));
    }
}

fn export_chunk_manifest(matches: &[ChunkMatch], path: &PathBuf) -> Result<()> {
    // Create a minimal manifest listing just these chunks
    let chunk_hashes: Vec<String> = matches.iter().map(|m| m.chunk.hash.to_string()).collect();

    let manifest = serde_json::json!({
        "version": "1.0",
        "chunks": chunk_hashes,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    std::fs::write(path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(())
}
