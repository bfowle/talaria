use crate::cli::formatting::output::*;
use anyhow::{Context, Result};
use clap::Args;
use std::path::PathBuf;
use talaria_herald::SHA256Hash;

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
    use talaria_herald::database::DatabaseManager;

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

    // If no specific version requested, use default index
    if database.is_none() || version.is_none() {
        return build_chunk_index();
    }

    let db = database.unwrap();
    let ver = version.unwrap();

    // Open DatabaseManager to access RocksDB manifests
    let manager = DatabaseManager::new(None)?;
    let rocksdb = manager.get_repository().storage.sequence_storage.get_rocksdb();

    // Query specific version from RocksDB
    let manifest_key = format!("manifest:{}:{}", db, ver);

    if let Some(manifest_bytes) = rocksdb.get_manifest(&manifest_key)? {
        // Deserialize manifest
        let temporal_manifest: talaria_herald::TemporalManifest =
            bincode::deserialize(&manifest_bytes)?;

        // Process chunks from manifest
        for chunk_meta in &temporal_manifest.chunk_index {
            // Create ChunkInfo from metadata
            let chunk_info = ChunkDisplayInfo {
                hash: chunk_meta.hash.clone(),
                database: db.to_string(),
                version: temporal_manifest.version.clone(),
                taxonomy: extract_taxonomy_info(chunk_meta),
                sequence_count: chunk_meta.sequence_count,
                size: chunk_meta.size as u64,
                compressed_size: chunk_meta
                    .compressed_size
                    .unwrap_or(chunk_meta.size)
                    as u64,
                compression_ratio: if let Some(compressed) = chunk_meta.compressed_size {
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
                .entry(db.to_string())
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

    Ok(index)
}

fn build_chunk_index() -> Result<ChunkIndex> {
    use talaria_herald::database::DatabaseManager;

    action("Building chunk index from RocksDB...");

    let mut index = ChunkIndex {
        by_hash: std::collections::HashMap::new(),
        by_taxid: std::collections::HashMap::new(),
        by_accession: std::collections::HashMap::new(),
        by_database: std::collections::HashMap::new(),
    };

    // Open DatabaseManager to access RocksDB manifests
    let manager = DatabaseManager::new(None)?;
    let databases = manager.list_databases()?;

    // Process each database's manifest from RocksDB
    for db_info in databases {
        // Get the manifest for this database from RocksDB
        let rocksdb = manager.get_repository().storage.sequence_storage.get_rocksdb();

        // Try to get current version manifest
        let alias_key = format!("alias:{}:current", db_info.name);
        if let Some(version_bytes) = rocksdb.get_manifest(&alias_key)? {
            let version = String::from_utf8(version_bytes)?;
            let manifest_key = format!("manifest:{}:{}", db_info.name, version);

            if let Some(manifest_bytes) = rocksdb.get_manifest(&manifest_key)? {
                // Deserialize manifest
                let temporal_manifest: talaria_herald::TemporalManifest =
                    bincode::deserialize(&manifest_bytes)?;

                // Process chunks from manifest
                for chunk_meta in &temporal_manifest.chunk_index {
                    // Create ChunkInfo from metadata
                    let chunk_info = ChunkDisplayInfo {
                        hash: chunk_meta.hash.clone(),
                        database: db_info.name.clone(),
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
                        .entry(db_info.name.clone())
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
    }

    Ok(index)
}

// Removed: extract_database_name() - no longer needed as we query RocksDB directly

fn extract_taxonomy_info(chunk_meta: &talaria_herald::ManifestMetadata) -> Vec<TaxonomyInfo> {
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
