/// Taxonomy management for SEQUOIA

pub mod discrepancy;
pub mod evolution;
pub mod extractor;
pub mod filter;
pub mod manifest;
pub mod prerequisites;
pub mod types;
pub mod version_store;

// Re-export commonly used types
pub use types::{
    VersionDecision, TaxonomyManifestFormat, InstalledComponent,
    AuditEntry, TaxonomyManifest, TaxonomyVersionPolicy
};
pub use prerequisites::TaxonomyPrerequisites;

use crate::storage::SEQUOIAStorage;
use crate::types::*;
use talaria_core::system::paths;
use talaria_utils::display::progress::{create_progress_bar, create_spinner};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct TaxonomyManager {
    base_path: PathBuf,
    taxonomy_tree: Option<TaxonomyTree>,
    taxon_to_chunks: HashMap<TaxonId, Vec<SHA256Hash>>,
    accession_to_taxon: HashMap<String, TaxonId>,
    version_history: Vec<TaxonomyVersion>,
}

#[derive(Debug, Clone)]
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
        // Files should be directly in the tree directory
        let nodes_file = tree_dir.join("nodes.dmp");
        let names_file = tree_dir.join("names.dmp");

        if !nodes_file.exists() || !names_file.exists() {
            return Err(anyhow::anyhow!("Taxonomy files not found in {:?}", tree_dir));
        }

        // Parse nodes.dmp
        let nodes_content = fs::read_to_string(&nodes_file)?;
        let mut nodes: HashMap<TaxonId, TaxonomyNode> = HashMap::new();
        let mut parent_map: HashMap<TaxonId, TaxonId> = HashMap::new();

        // Count lines for progress bar
        let total_nodes = nodes_content.lines().count() as u64;
        let nodes_progress = create_progress_bar(total_nodes, "Parsing nodes.dmp");

        for line in nodes_content.lines() {
            nodes_progress.inc(1);
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
        nodes_progress.finish_with_message("Nodes parsed");

        // Parse names.dmp for scientific names
        let names_content = fs::read_to_string(&names_file)?;

        // Count lines for progress bar
        let total_names = names_content.lines().count() as u64;
        let names_progress = create_progress_bar(total_names, "Parsing names.dmp");

        for line in names_content.lines() {
            names_progress.inc(1);
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
        names_progress.finish_with_message("Names parsed");

        // Build children lists
        let build_progress = create_spinner("Building taxonomy tree");
        for (child_id, parent_id) in &parent_map {
            if let Some(parent) = nodes.get_mut(parent_id) {
                parent.children.push(*child_id);
            }
        }
        build_progress.finish_with_message("Taxonomy tree built");

        // Find root (taxon ID 1 is typically the root)
        let root = nodes
            .get(&TaxonId(1))
            .ok_or_else(|| anyhow::anyhow!("Root taxon not found"))?
            .clone();

        self.taxonomy_tree = Some(TaxonomyTree {
            root,
            id_to_node: nodes,
        });

        // Save the loaded tree
        self.save_taxonomy_tree()?;

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
    pub fn detect_discrepancies(&self, storage: &SEQUOIAStorage) -> Result<Vec<TaxonomicDiscrepancy>> {
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
            // TODO: Implement evolution-based changes when SEQUOIARepository is available
            return Ok(TaxonomyChanges::default());
        }

        // TODO: Implement evolution-based changes when SEQUOIARepository is available
        Ok(TaxonomyChanges::default())
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

        // If no taxonomy is loaded, return a placeholder hash
        if !self.has_taxonomy() {
            // Return a deterministic placeholder hash for "no taxonomy"
            return Ok(SHA256Hash::compute(b"NO_TAXONOMY_LOADED"));
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
        

        // Check if evolution directory exists before trying to load
        let evolution_dir = self.base_path.join("evolution");
        if !evolution_dir.exists() {
            // No evolution directory, nothing to load
            return Ok(());
        }

        // TODO: Load taxonomy evolution history when TaxonomySnapshot is implemented
        // let mut evolution = TaxonomyEvolution::new(&self.base_path)?;
        // if let Ok(()) = evolution.load() {
        //     // Convert TaxonomyEvolution snapshots to our TaxonomyVersion format
        //     let history_file = self.base_path.join("evolution/history.json");
        //     if history_file.exists() {
        //         let content = fs::read_to_string(&history_file)?;
        //         if let Ok(snapshots) =
        //             serde_json::from_str::<Vec<evolution::TaxonomySnapshot>>(&content)
        //         {
        //             for snapshot in snapshots {
        //                 self.version_history.push(TaxonomyVersion {
        //                     version: snapshot.version,
        //                     date: snapshot.date,
        //                     source: "NCBI".to_string(),
        //                     changes: snapshot.changes_from_previous.unwrap_or_default(),
        //                 });
        //             }
        //         }
        //     }
        // }
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
        let filename = path.file_name()
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
