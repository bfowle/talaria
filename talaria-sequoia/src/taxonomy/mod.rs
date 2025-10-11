/// Taxonomy management for SEQUOIA
pub mod discrepancy;
pub mod evolution;
pub mod extractor;
pub mod filter;
pub mod manifest;
pub mod prerequisites;
pub mod types;

// Re-export commonly used types
pub use prerequisites::TaxonomyPrerequisites;
pub use types::{
    AuditEntry, InstalledComponent, TaxonomyManifest, TaxonomyManifestFormat,
    TaxonomyVersionPolicy, VersionDecision,
};

use chrono::{DateTime, Utc};

use crate::storage::SequoiaStorage;
use crate::types::*;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use talaria_core::system::paths;
// UI imports removed - using progress_callback pattern instead

pub struct TaxonomyManager {
    base_path: PathBuf,
    taxonomy_tree: Option<TaxonomyTree>,
    taxon_to_chunks: HashMap<TaxonId, Vec<SHA256Hash>>,
    accession_to_taxon: HashMap<String, TaxonId>,
    version_history: Vec<TaxonomyVersion>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaxonomyVersion {
    pub version: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub changes: TaxonomyChanges,
}

#[derive(Debug, Clone)]
pub struct TaxonomyTree {
    pub root: TaxonomyNode,
    pub id_to_node: HashMap<TaxonId, TaxonomyNode>,
}

#[derive(Debug, Clone)]
pub struct TaxonomyNode {
    pub taxon_id: TaxonId,
    pub parent_id: Option<TaxonId>,
    pub name: String,
    pub rank: String,
    pub children: Vec<TaxonId>,
}

impl TaxonomyManager {
    pub fn new(_base_path: &Path) -> Result<Self> {
        // Use unified taxonomy directory
        let taxonomy_dir = paths::talaria_taxonomy_current_dir();

        // Don't create any directories here - let the actual taxonomy download handle it
        // This prevents creating empty version directories before the real data arrives

        // Use the taxonomy_dir if it exists, otherwise use a placeholder path
        let base_path = if taxonomy_dir.exists() {
            taxonomy_dir
        } else {
            // Return a path that will be updated when taxonomy is actually downloaded
            paths::talaria_taxonomy_versions_dir()
        };

        Ok(Self {
            base_path,
            taxonomy_tree: None,
            taxon_to_chunks: HashMap::new(),
            accession_to_taxon: HashMap::new(),
            version_history: Vec::new(),
        })
    }

    pub fn load(base_path: &Path) -> Result<Self> {
        // Create a new manager which will set the correct base_path
        let mut manager = Self::new(base_path)?;

        // Check if taxonomy directory exists and has the tree file
        let taxonomy_dir = paths::talaria_taxonomy_current_dir();
        if taxonomy_dir.exists() {
            // Update base_path to the actual taxonomy directory
            manager.base_path = taxonomy_dir.clone();

            // Load taxonomy tree if it exists
            let tree_path = taxonomy_dir.join("taxonomy_tree.json");
            if tree_path.exists() {
                manager.load_taxonomy_tree(&tree_path)?;
            }

            // Load mappings
            let mappings_path = taxonomy_dir.join("mappings.json");
            if mappings_path.exists() {
                manager.load_mappings(&mappings_path)?;
            }
        }

        // Load version history from TaxonomyEvolution
        manager.load_version_history()?;

        Ok(manager)
    }

    /// Load NCBI taxonomy from tree directory files
    pub fn load_ncbi_taxonomy(&mut self, tree_dir: &Path) -> Result<()> {
        // No progress callback - silent loading
        self.load_ncbi_taxonomy_with_progress(tree_dir, None)
    }

    /// Load NCBI taxonomy with optional progress display
    pub fn load_ncbi_taxonomy_quiet(&mut self, tree_dir: &Path) -> Result<()> {
        self.load_ncbi_taxonomy_with_progress(tree_dir, None)
    }

    fn load_ncbi_taxonomy_with_progress(
        &mut self,
        tree_dir: &Path,
        progress_callback: Option<&dyn Fn(&str)>,
    ) -> Result<()> {
        // Files should be directly in the tree directory
        let nodes_file = tree_dir.join("nodes.dmp");
        let names_file = tree_dir.join("names.dmp");

        if !nodes_file.exists() || !names_file.exists() {
            return Err(anyhow::anyhow!(
                "Taxonomy files not found in {:?}",
                tree_dir
            ));
        }

        // Parse nodes.dmp
        let nodes_content = fs::read_to_string(&nodes_file)?;
        let mut nodes: HashMap<TaxonId, TaxonomyNode> = HashMap::new();
        let mut parent_map: HashMap<TaxonId, TaxonId> = HashMap::new();

        let total_nodes = nodes_content.lines().count();
        if let Some(cb) = progress_callback {
            cb(&format!("Parsing {} nodes from nodes.dmp", total_nodes));
        }

        for (idx, line) in nodes_content.lines().enumerate() {
            if idx % 10000 == 0 && idx > 0 {
                if let Some(cb) = progress_callback {
                    cb(&format!("Parsed {}/{} nodes", idx, total_nodes));
                }
            }
            let parts: Vec<&str> = line.split("\t|\t").collect();
            if parts.len() < 3 {
                continue;
            }

            let taxon_id = TaxonId(parts[0].parse()?);
            let parent_id = TaxonId(parts[1].parse()?);
            let rank = parts[2].to_string();

            nodes.insert(
                taxon_id,
                TaxonomyNode {
                    taxon_id,
                    parent_id: if parent_id == taxon_id {
                        None
                    } else {
                        Some(parent_id)
                    },
                    name: String::new(), // Will be filled from names.dmp
                    rank,
                    children: Vec::new(),
                },
            );

            if parent_id != taxon_id {
                parent_map.insert(taxon_id, parent_id);
            }
        }
        if let Some(cb) = progress_callback {
            cb("Nodes parsed");
        }

        // Parse names.dmp for scientific names
        let names_content = fs::read_to_string(&names_file)?;

        let total_names = names_content.lines().count();
        if let Some(cb) = progress_callback {
            cb(&format!("Parsing {} names from names.dmp", total_names));
        }

        for (idx, line) in names_content.lines().enumerate() {
            if idx % 10000 == 0 && idx > 0 {
                if let Some(cb) = progress_callback {
                    cb(&format!("Parsed {}/{} names", idx, total_names));
                }
            }
            let parts: Vec<&str> = line.split("\t|\t").collect();
            if parts.len() < 4 {
                continue;
            }

            let taxon_id = TaxonId(parts[0].parse()?);
            let name = parts[1].to_string();
            let name_class = parts[3].trim_end_matches("\t|");

            // Only use scientific names
            if name_class == "scientific name" {
                if let Some(node) = nodes.get_mut(&taxon_id) {
                    node.name = name;
                }
            }
        }
        if let Some(cb) = progress_callback {
            cb("Names parsed");
        }

        // Build children lists
        if let Some(cb) = progress_callback {
            cb("Building taxonomy tree");
        }
        for (child_id, parent_id) in &parent_map {
            if let Some(parent) = nodes.get_mut(parent_id) {
                parent.children.push(*child_id);
            }
        }
        if let Some(cb) = progress_callback {
            cb("Taxonomy tree built");
        }

        // Find root (taxon ID 1 is typically the root)
        let root = nodes
            .get(&TaxonId(1))
            .ok_or_else(|| anyhow::anyhow!("Root taxon not found"))?
            .clone();

        self.taxonomy_tree = Some(TaxonomyTree {
            root,
            id_to_node: nodes,
        });

        // Save the loaded tree only if it doesn't already exist
        // This prevents expensive re-serialization on every load
        let tree_path = paths::talaria_taxonomy_current_dir().join("taxonomy_tree.json");
        if !tree_path.exists() {
            self.save_taxonomy_tree()?;
        }

        Ok(())
    }

    /// Load UniProt ID mapping
    pub fn load_uniprot_mapping(&mut self, idmapping_file: &Path) -> Result<()> {
        // Check if file exists in current taxonomy version
        let idmapping_path = if idmapping_file.is_absolute() {
            idmapping_file.to_path_buf()
        } else {
            // Try in current taxonomy version
            paths::talaria_taxonomy_current_dir().join(idmapping_file)
        };

        if !idmapping_path.exists() {
            return Err(anyhow::anyhow!(
                "UniProt ID mapping file not found: {}",
                idmapping_path.display()
            ));
        }

        let idmapping_file = &idmapping_path;
        use flate2::read::GzDecoder;
        use std::io::{BufRead, BufReader};

        let file = fs::File::open(idmapping_file)?;
        let decoder = GzDecoder::new(file);
        let reader = BufReader::new(decoder);

        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split('\t').collect();

            if parts.len() >= 3 {
                let accession = parts[0].to_string();
                if let Ok(taxon_id) = parts[2].parse::<u32>() {
                    self.accession_to_taxon.insert(accession, TaxonId(taxon_id));
                }
            }
        }

        Ok(())
    }

    /// Get chunks containing sequences for a specific taxon
    pub fn get_chunks_for_taxon(&self, taxon_name: &str) -> Result<Vec<SHA256Hash>> {
        // Find taxon ID by name
        let taxon_id = self.find_taxon_by_name(taxon_name)?;

        // Get direct chunks
        let mut chunks = self
            .taxon_to_chunks
            .get(&taxon_id)
            .cloned()
            .unwrap_or_default();

        // Include descendant taxa
        if let Some(_tree) = &self.taxonomy_tree {
            let descendants = self.get_descendant_taxa(&taxon_id)?;
            for descendant in descendants {
                if let Some(descendant_chunks) = self.taxon_to_chunks.get(&descendant) {
                    chunks.extend(descendant_chunks.clone());
                }
            }
        }

        // Deduplicate
        chunks.sort();
        chunks.dedup();

        Ok(chunks)
    }

    fn find_taxon_by_name(&self, name: &str) -> Result<TaxonId> {
        let tree = self
            .taxonomy_tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy loaded"))?;

        // Try exact match
        for (taxon_id, node) in &tree.id_to_node {
            if node.name.eq_ignore_ascii_case(name) {
                return Ok(*taxon_id);
            }
        }

        // Try common names/abbreviations
        let normalized = name.to_lowercase();
        match normalized.as_str() {
            "e.coli" | "ecoli" => Ok(TaxonId(562)),
            "human" | "homo sapiens" => Ok(TaxonId(9606)),
            "mouse" | "mus musculus" => Ok(TaxonId(10090)),
            _ => {
                // Try partial match
                for (taxon_id, node) in &tree.id_to_node {
                    if node.name.to_lowercase().contains(&normalized) {
                        return Ok(*taxon_id);
                    }
                }
                Err(anyhow::anyhow!("Taxon not found: {}", name))
            }
        }
    }

    fn get_descendant_taxa(&self, taxon_id: &TaxonId) -> Result<Vec<TaxonId>> {
        let tree = self
            .taxonomy_tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy loaded"))?;

        let mut descendants = Vec::new();
        let mut to_visit = vec![*taxon_id];

        while let Some(current) = to_visit.pop() {
            if let Some(node) = tree.id_to_node.get(&current) {
                for child in &node.children {
                    descendants.push(*child);
                    to_visit.push(*child);
                }
            }
        }

        Ok(descendants)
    }

    /// Check if a taxon exists in the taxonomy
    pub fn taxon_exists(&self, taxon_id: TaxonId) -> bool {
        if let Some(tree) = &self.taxonomy_tree {
            tree.id_to_node.contains_key(&taxon_id)
        } else {
            false
        }
    }

    /// Get parent of a taxon
    pub fn get_parent(&self, taxon_id: TaxonId) -> Option<TaxonId> {
        if let Some(tree) = &self.taxonomy_tree {
            tree.id_to_node
                .get(&taxon_id)
                .and_then(|node| node.parent_id)
        } else {
            None
        }
    }

    /// Get ancestor at a specific rank
    pub fn get_ancestor_at_rank(&self, taxon_id: TaxonId, rank: &str) -> Option<TaxonId> {
        if let Some(tree) = &self.taxonomy_tree {
            let mut current = taxon_id;
            while let Some(node) = tree.id_to_node.get(&current) {
                if node.rank == rank {
                    return Some(current);
                }
                current = node.parent_id?;
            }
        }
        None
    }

    /// Detect discrepancies between sequence annotations and taxonomy
    pub fn detect_discrepancies(
        &self,
        storage: &SequoiaStorage,
    ) -> Result<Vec<TaxonomicDiscrepancy>> {
        use discrepancy::DiscrepancyDetector;

        let mut detector = DiscrepancyDetector::new();
        // Pass accession mappings to detector
        detector.set_taxonomy_mappings(self.accession_to_taxon.clone());
        detector.detect_all(storage)
    }

    /// Update taxon-to-chunk mappings
    pub fn update_chunk_mapping(&mut self, chunk: &ChunkManifest) {
        for taxon_id in &chunk.taxon_ids {
            self.taxon_to_chunks
                .entry(*taxon_id)
                .or_default()
                .push(chunk.chunk_hash.clone());
        }
    }

    /// Compare two taxonomy versions
    pub fn compare_versions(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<TaxonomyChanges> {
        // First check if we have the versions in our history
        let old_found = self
            .version_history
            .iter()
            .any(|v| v.version == old_version);
        let new_found = self
            .version_history
            .iter()
            .any(|v| v.version == new_version);

        if !old_found || !new_found {
            // If versions not in history, try to load from RocksDB
            return self.load_taxonomy_changes_from_storage(old_version, new_version);
        }

        // Get the two versions
        let old_idx = self
            .version_history
            .iter()
            .position(|v| v.version == old_version)
            .unwrap();
        let new_idx = self
            .version_history
            .iter()
            .position(|v| v.version == new_version)
            .unwrap();

        // If these are the same version, no changes
        if old_idx == new_idx {
            return Ok(TaxonomyChanges::default());
        }

        // Compute changes between the versions
        self.compute_version_changes(old_idx, new_idx)
    }

    /// Get taxonomy node by ID
    pub fn get_node(&self, taxon_id: &TaxonId) -> Option<&TaxonomyNode> {
        self.taxonomy_tree.as_ref()?.id_to_node.get(taxon_id)
    }

    /// Get lineage for a taxon
    pub fn get_lineage(&self, taxon_id: &TaxonId) -> Result<Vec<TaxonomyNode>> {
        let tree = self
            .taxonomy_tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy loaded"))?;

        let mut lineage = Vec::new();
        let mut current_id = Some(*taxon_id);

        while let Some(id) = current_id {
            if let Some(node) = tree.id_to_node.get(&id) {
                lineage.push(node.clone());
                current_id = node.parent_id;
            } else {
                break;
            }
        }

        lineage.reverse(); // Root to leaf order
        Ok(lineage)
    }

    fn save_taxonomy_tree(&self) -> Result<()> {
        if let Some(tree) = &self.taxonomy_tree {
            // Save to the taxonomy current directory
            let taxonomy_dir = paths::talaria_taxonomy_current_dir();

            // Ensure the directory exists
            if !taxonomy_dir.exists() {
                std::fs::create_dir_all(&taxonomy_dir)?;
            }

            let tree_path = taxonomy_dir.join("taxonomy_tree.json");
            let content = serde_json::to_string_pretty(tree)?;
            fs::write(tree_path, content)?;
        }
        Ok(())
    }

    fn load_taxonomy_tree(&mut self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)?;
        self.taxonomy_tree = Some(serde_json::from_str(&content)?);
        Ok(())
    }

    fn load_mappings(&mut self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)?;
        let mappings: serde_json::Value = serde_json::from_str(&content)?;

        if let Some(acc_map) = mappings
            .get("accession_to_taxon")
            .and_then(|v| v.as_object())
        {
            for (acc, taxon_value) in acc_map {
                if let Some(taxon_id) = taxon_value.as_u64() {
                    self.accession_to_taxon
                        .insert(acc.clone(), TaxonId(taxon_id as u32));
                }
            }
        }

        Ok(())
    }

    /// Get accession to taxon mapping
    pub fn get_accession_mapping(&self) -> &HashMap<String, TaxonId> {
        &self.accession_to_taxon
    }

    /// Get taxonomy root hash
    pub fn get_taxonomy_root(&self) -> Result<crate::MerkleHash> {
        use crate::verification::merkle::MerkleDAG;

        // If no taxonomy is loaded, return an error
        if !self.has_taxonomy() {
            return Err(anyhow::anyhow!(
                "Taxonomy data not loaded. Please download with: talaria database download ncbi/taxonomy"
            ));
        }

        // Convert our taxonomy tree to the Merkle tree format
        let tree = self
            .taxonomy_tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy loaded"))?;

        // Convert the root node recursively
        let merkle_root = self.convert_to_merkle_node(&tree.root)?;

        let merkle_tree = crate::verification::merkle::TaxonomyTree { root: merkle_root };

        // Build DAG and get root hash
        let dag = MerkleDAG::build_taxonomy_dag(merkle_tree)?;

        dag.root_hash()
            .ok_or_else(|| anyhow::anyhow!("Failed to compute taxonomy root"))
    }

    /// Convert internal taxonomy node to Merkle tree node
    fn convert_to_merkle_node(
        &self,
        node: &TaxonomyNode,
    ) -> Result<crate::verification::merkle::TaxonomyNode> {
        let tree = self
            .taxonomy_tree
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No taxonomy loaded"))?;

        // Recursively convert children
        let mut merkle_children = Vec::new();
        for child_id in &node.children {
            if let Some(child_node) = tree.id_to_node.get(child_id) {
                merkle_children.push(self.convert_to_merkle_node(child_node)?);
            }
        }

        Ok(crate::verification::merkle::TaxonomyNode {
            taxon_id: node.taxon_id,
            name: node.name.clone(),
            rank: node.rank.clone(),
            children: merkle_children,
        })
    }

    /// Load version history from TaxonomyEvolution storage
    fn load_version_history(&mut self) -> Result<()> {
        // Try to load from RocksDB first
        use rocksdb::{Options, DB};

        let evolution_db_path = self.base_path.join("evolution.db");
        if evolution_db_path.exists() {
            let mut opts = Options::default();
            opts.create_if_missing(false);

            if let Ok(db) = DB::open_for_read_only(&opts, &evolution_db_path, false) {
                // Load version history from RocksDB
                let mut versions = Vec::new();

                // Iterate through all keys with "version:" prefix
                let prefix = b"version:";
                let iter = db.iterator(rocksdb::IteratorMode::From(
                    prefix,
                    rocksdb::Direction::Forward,
                ));

                for item in iter {
                    if let Ok((key, value)) = item {
                        if !key.starts_with(prefix) {
                            break; // We've moved past version keys
                        }

                        // Deserialize the version data
                        if let Ok(version) = bincode::deserialize::<TaxonomyVersion>(&value) {
                            versions.push(version);
                        } else if let Ok(version) =
                            serde_json::from_slice::<TaxonomyVersion>(&value)
                        {
                            versions.push(version);
                        }
                    }
                }

                // Sort by date
                versions.sort_by(|a, b| a.date.cmp(&b.date));
                self.version_history = versions;

                return Ok(());
            }
        }

        // Fallback: Check if evolution directory exists with JSON files
        let evolution_dir = self.base_path.join("evolution");
        if evolution_dir.exists() {
            let history_file = evolution_dir.join("history.json");
            if history_file.exists() {
                let content = fs::read_to_string(&history_file)?;

                // Define a simple snapshot structure for deserialization
                #[derive(serde::Deserialize)]
                struct TaxonomySnapshot {
                    version: String,
                    date: DateTime<Utc>,
                    changes_from_previous: Option<TaxonomyChanges>,
                }

                if let Ok(snapshots) = serde_json::from_str::<Vec<TaxonomySnapshot>>(&content) {
                    for snapshot in snapshots {
                        self.version_history.push(TaxonomyVersion {
                            version: snapshot.version,
                            date: snapshot.date,
                            source: "NCBI".to_string(),
                            changes: snapshot.changes_from_previous.unwrap_or_default(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Add a new taxonomy version to history
    pub fn add_version(&mut self, version: String, source: String, changes: TaxonomyChanges) {
        self.version_history.push(TaxonomyVersion {
            version,
            date: chrono::Utc::now(),
            source,
            changes,
        });
    }

    /// Get version history
    pub fn get_version_history(&self) -> &[TaxonomyVersion] {
        &self.version_history
    }

    /// Get a specific version from history
    pub fn get_version(&self, version: &str) -> Option<&TaxonomyVersion> {
        self.version_history.iter().find(|v| v.version == version)
    }

    /// Check if taxonomy data is loaded
    pub fn has_taxonomy(&self) -> bool {
        self.taxonomy_tree.is_some()
    }

    /// Initialize with empty taxonomy if needed
    pub fn ensure_taxonomy(&mut self) -> Result<()> {
        if self.taxonomy_tree.is_none() {
            // Create a minimal root node
            let root = TaxonomyNode {
                taxon_id: TaxonId(1),
                parent_id: None,
                name: "root".to_string(),
                rank: "no rank".to_string(),
                children: Vec::new(),
            };

            self.taxonomy_tree = Some(TaxonomyTree {
                root: root.clone(),
                id_to_node: std::iter::once((TaxonId(1), root)).collect(),
            });
        }
        Ok(())
    }

    /// Get default taxonomy components
    pub fn default_components() -> Vec<String> {
        vec![
            "nodes.dmp".to_string(),
            "names.dmp".to_string(),
            "merged.dmp".to_string(),
            "delnodes.dmp".to_string(),
        ]
    }

    /// Detect file format from a taxonomy file
    pub fn detect_file_format(path: &Path) -> Result<String> {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid filename"))?;

        if filename.ends_with(".dmp") {
            Ok("dmp".to_string())
        } else if filename.ends_with(".tsv") || filename.ends_with(".tab") {
            Ok("tsv".to_string())
        } else if filename.ends_with(".csv") {
            Ok("csv".to_string())
        } else {
            Ok("unknown".to_string())
        }
    }

    /// Check if we should create a new taxonomy version
    pub fn should_create_new_version(&self, _changes: &[String]) -> bool {
        // For now, always return false to append to current version
        // In the future, this could check the nature of changes
        false
    }

    /// Load taxonomy changes from storage backend
    fn load_taxonomy_changes_from_storage(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<TaxonomyChanges> {
        // Try to load from RocksDB using version store
        let version_store_path = self.base_path.join("version_store.db");
        if !version_store_path.exists() {
            return Ok(TaxonomyChanges::default());
        }

        // Access RocksDB directly to retrieve stored changes
        use rocksdb::{Options, DB};
        let mut opts = Options::default();
        opts.create_if_missing(false);

        match DB::open_for_read_only(&opts, &version_store_path, false) {
            Ok(db) => {
                // Try to find cached changes between these versions
                let change_key = format!("changes:{}:{}", old_version, new_version);
                match db.get(change_key.as_bytes()) {
                    Ok(Some(data)) => {
                        // Deserialize the changes
                        Ok(bincode::deserialize(&data)
                            .or_else(|_| serde_json::from_slice(&data))
                            .unwrap_or_else(|_| TaxonomyChanges::default()))
                    }
                    _ => {
                        // No cached changes, compute them if possible
                        self.compute_changes_from_trees(old_version, new_version)
                    }
                }
            }
            Err(_) => Ok(TaxonomyChanges::default()),
        }
    }

    /// Compute changes between version indices
    fn compute_version_changes(&self, old_idx: usize, new_idx: usize) -> Result<TaxonomyChanges> {
        let mut combined_changes = TaxonomyChanges::default();

        // Determine direction (forward or backward in time)
        let (start, end, reverse) = if old_idx < new_idx {
            (old_idx + 1, new_idx + 1, false)
        } else {
            (new_idx + 1, old_idx + 1, true)
        };

        // Accumulate changes between versions
        for i in start..end {
            let version = &self.version_history[i];
            let changes = &version.changes;

            if !reverse {
                // Forward: apply changes as-is
                combined_changes
                    .reclassifications
                    .extend(changes.reclassifications.clone());
                combined_changes.new_taxa.extend(changes.new_taxa.clone());
                combined_changes
                    .deprecated_taxa
                    .extend(changes.deprecated_taxa.clone());
                combined_changes
                    .merged_taxa
                    .extend(changes.merged_taxa.clone());
            } else {
                // Backward: reverse the changes
                // New taxa become deprecated, deprecated become new
                combined_changes
                    .deprecated_taxa
                    .extend(changes.new_taxa.clone());
                combined_changes
                    .new_taxa
                    .extend(changes.deprecated_taxa.clone());

                // Reverse merges (merged becomes splits conceptually)
                for (old, new) in &changes.merged_taxa {
                    // In reverse, the new taxon splits back to the old
                    combined_changes.merged_taxa.push((*new, *old));
                }

                // Reverse reclassifications
                for reclass in &changes.reclassifications {
                    combined_changes.reclassifications.push(Reclassification {
                        taxon_id: reclass.taxon_id,
                        old_parent: reclass.new_parent,
                        new_parent: reclass.old_parent,
                        reason: format!("Reversed: {}", reclass.reason),
                    });
                }
            }
        }

        Ok(combined_changes)
    }

    /// Compute changes by comparing taxonomy trees directly
    fn compute_changes_from_trees(
        &self,
        old_version: &str,
        new_version: &str,
    ) -> Result<TaxonomyChanges> {
        use rocksdb::{Options, DB};
        use std::collections::{HashMap, HashSet};

        let mut changes = TaxonomyChanges::default();

        // Load both taxonomy trees from storage
        let tree_store_path = self.base_path.join("tree_store.db");
        if !tree_store_path.exists() {
            // Try alternative RocksDB path
            let alt_path = self.base_path.join("taxonomy.db");
            if !alt_path.exists() {
                return Err(anyhow::anyhow!("No taxonomy tree storage found"));
            }
        }

        let mut opts = Options::default();
        opts.create_if_missing(false);

        let db = DB::open_for_read_only(&opts, &tree_store_path, false)?;

        // Load old tree
        let old_key = format!("tree:{}", old_version);
        let old_tree_data = db
            .get(old_key.as_bytes())?
            .ok_or_else(|| anyhow::anyhow!("Old taxonomy version {} not found", old_version))?;
        let old_tree: TaxonomyTree = bincode::deserialize(&old_tree_data)?;

        // Load new tree
        let new_key = format!("tree:{}", new_version);
        let new_tree_data = db
            .get(new_key.as_bytes())?
            .ok_or_else(|| anyhow::anyhow!("New taxonomy version {} not found", new_version))?;
        let new_tree: TaxonomyTree = bincode::deserialize(&new_tree_data)?;

        // Build sets of taxon IDs
        let old_taxa: HashSet<TaxonId> = old_tree.id_to_node.keys().cloned().collect();
        let new_taxa: HashSet<TaxonId> = new_tree.id_to_node.keys().cloned().collect();

        // Find new taxa (in new but not in old)
        for taxon_id in new_taxa.difference(&old_taxa) {
            changes.new_taxa.push(*taxon_id);
        }

        // Find deprecated taxa (in old but not in new)
        for taxon_id in old_taxa.difference(&new_taxa) {
            changes.deprecated_taxa.push(*taxon_id);
        }

        // Find reclassifications and merges
        let mut merge_candidates: HashMap<TaxonId, Vec<TaxonId>> = HashMap::new();

        for taxon_id in old_taxa.intersection(&new_taxa) {
            let old_node = &old_tree.id_to_node[taxon_id];
            let new_node = &new_tree.id_to_node[taxon_id];

            // Check if parent changed (reclassification)
            if old_node.parent_id != new_node.parent_id {
                changes.reclassifications.push(Reclassification {
                    taxon_id: *taxon_id,
                    old_parent: old_node.parent_id.unwrap_or(TaxonId(0)),
                    new_parent: new_node.parent_id.unwrap_or(TaxonId(0)),
                    reason: "Taxonomic reclassification".to_string(),
                });
            }

            // Track nodes with same parent for potential merges
            if let Some(parent) = new_node.parent_id {
                merge_candidates.entry(parent).or_default().push(*taxon_id);
            }
        }

        // Detect merges: multiple old taxa mapping to single new taxon
        // This happens when deprecated taxa had children that got reclassified to same parent
        for deprecated_id in &changes.deprecated_taxa {
            if let Some(old_node) = old_tree.id_to_node.get(deprecated_id) {
                // Check if its children got reclassified to a single new parent
                let mut new_parents: HashSet<TaxonId> = HashSet::new();
                for child_id in &old_node.children {
                    if let Some(new_node) = new_tree.id_to_node.get(child_id) {
                        if let Some(parent) = new_node.parent_id {
                            new_parents.insert(parent);
                        }
                    }
                }

                // If all children went to single parent, it's likely a merge
                if new_parents.len() == 1 {
                    if let Some(new_parent) = new_parents.iter().next() {
                        changes.merged_taxa.push((*deprecated_id, *new_parent));
                    }
                }
            }
        }

        Ok(changes)
    }
}

// Implement serialization for TaxonomyTree
impl serde::Serialize for TaxonomyTree {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TaxonomyTree", 2)?;
        state.serialize_field("root", &self.root)?;
        state.serialize_field("id_to_node", &self.id_to_node)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for TaxonomyTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct TaxonomyTreeHelper {
            root: TaxonomyNode,
            id_to_node: HashMap<TaxonId, TaxonomyNode>,
        }

        let helper = TaxonomyTreeHelper::deserialize(deserializer)?;
        Ok(TaxonomyTree {
            root: helper.root,
            id_to_node: helper.id_to_node,
        })
    }
}

// Implement serialization for TaxonomyNode
impl serde::Serialize for TaxonomyNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TaxonomyNode", 5)?;
        state.serialize_field("taxon_id", &self.taxon_id)?;
        state.serialize_field("parent_id", &self.parent_id)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("rank", &self.rank)?;
        state.serialize_field("children", &self.children)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for TaxonomyNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct TaxonomyNodeHelper {
            taxon_id: TaxonId,
            parent_id: Option<TaxonId>,
            name: String,
            rank: String,
            children: Vec<TaxonId>,
        }

        let helper = TaxonomyNodeHelper::deserialize(deserializer)?;
        Ok(TaxonomyNode {
            taxon_id: helper.taxon_id,
            parent_id: helper.parent_id,
            name: helper.name,
            rank: helper.rank,
            children: helper.children,
        })
    }
}
