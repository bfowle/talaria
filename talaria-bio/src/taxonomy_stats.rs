use crate::taxonomy::TaxonomyDB;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Statistics for taxonomic coverage analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyCoverage {
    /// Map of taxon ID to sequence count
    pub taxon_counts: HashMap<u32, usize>,
    /// Total number of sequences
    pub total_sequences: usize,
    /// Number of unique taxon IDs
    pub unique_taxa: usize,
    /// Coverage by taxonomic rank
    pub rank_coverage: BTreeMap<String, RankStats>,
    /// Database name/identifier
    pub database: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankStats {
    pub rank: String,
    pub count: usize,
    pub unique_taxa: usize,
    pub percentage: f64,
}

#[derive(Debug, Clone)]
pub struct TaxonNode {
    pub taxon_id: u32,
    pub name: String,
    pub rank: String,
    pub sequence_count: usize,
    pub cumulative_count: usize, // Including all descendants
    pub children: Vec<TaxonNode>,
}

impl TaxonomyCoverage {
    pub fn new(database: String) -> Self {
        Self {
            taxon_counts: HashMap::new(),
            total_sequences: 0,
            unique_taxa: 0,
            rank_coverage: BTreeMap::new(),
            database,
        }
    }

    /// Add a sequence with its taxon ID
    pub fn add_sequence(&mut self, taxon_id: u32) {
        *self.taxon_counts.entry(taxon_id).or_insert(0) += 1;
        self.total_sequences += 1;
    }

    /// Calculate statistics after all sequences are added
    pub fn calculate_stats(&mut self, taxonomy_db: &TaxonomyDB) {
        self.unique_taxa = self.taxon_counts.len();

        // Calculate coverage by rank
        let mut rank_counts: HashMap<String, HashMap<u32, usize>> = HashMap::new();

        for (&taxon_id, &count) in &self.taxon_counts {
            // Get lineage for this taxon
            let lineage = taxonomy_db.get_lineage(taxon_id);

            // Count sequences for each rank in the lineage
            for ancestor_id in lineage {
                if let Some(taxon_info) = taxonomy_db.get_taxon(ancestor_id) {
                    let rank = taxon_info.rank.clone();
                    rank_counts
                        .entry(rank)
                        .or_default()
                        .entry(ancestor_id)
                        .and_modify(|e| *e += count)
                        .or_insert(count);
                }
            }
        }

        // Convert to RankStats
        for (rank, taxa) in rank_counts {
            let total_count: usize = taxa.values().sum();
            let unique_count = taxa.len();
            let percentage = (total_count as f64 / self.total_sequences as f64) * 100.0;

            self.rank_coverage.insert(
                rank.clone(),
                RankStats {
                    rank,
                    count: total_count,
                    unique_taxa: unique_count,
                    percentage,
                },
            );
        }
    }

    /// Build a taxonomic tree for visualization
    pub fn build_tree(&self, taxonomy_db: &TaxonomyDB, root_taxon: Option<u32>) -> TaxonNode {
        let root_id = root_taxon.unwrap_or(1); // Default to root of life
        self.build_tree_recursive(root_id, taxonomy_db, &mut HashMap::new())
    }

    fn build_tree_recursive(
        &self,
        taxon_id: u32,
        taxonomy_db: &TaxonomyDB,
        visited: &mut HashMap<u32, usize>,
    ) -> TaxonNode {
        // Prevent infinite recursion
        if visited.contains_key(&taxon_id) {
            return TaxonNode {
                taxon_id,
                name: "Circular reference".to_string(),
                rank: "error".to_string(),
                sequence_count: 0,
                cumulative_count: 0,
                children: vec![],
            };
        }
        visited.insert(taxon_id, 0);

        let taxon_info = taxonomy_db.get_taxon(taxon_id);
        let name = taxon_info
            .map(|t| t.scientific_name.clone())
            .unwrap_or_else(|| format!("Unknown ({})", taxon_id));
        let rank = taxon_info
            .map(|t| t.rank.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let sequence_count = self.taxon_counts.get(&taxon_id).copied().unwrap_or(0);

        // Find children
        let mut children = Vec::new();
        let mut cumulative_count = sequence_count;

        // Find all taxa that have this as parent
        for (&child_id, &_child_count) in &self.taxon_counts {
            if child_id == taxon_id {
                continue;
            }

            let lineage = taxonomy_db.get_lineage(child_id);
            if lineage.contains(&taxon_id) {
                // This is a descendant
                if let Some(child_info) = taxonomy_db.get_taxon(child_id) {
                    if child_info.parent_id == Some(taxon_id) {
                        // Direct child
                        let child_node = self.build_tree_recursive(child_id, taxonomy_db, visited);
                        cumulative_count += child_node.cumulative_count;
                        children.push(child_node);
                    }
                }
            }
        }

        // Sort children by cumulative count (descending)
        children.sort_by(|a, b| b.cumulative_count.cmp(&a.cumulative_count));

        TaxonNode {
            taxon_id,
            name,
            rank,
            sequence_count,
            cumulative_count,
            children,
        }
    }

    /// Compare coverage between two databases
    pub fn compare(&self, other: &TaxonomyCoverage) -> CoverageComparison {
        let mut common_taxa = Vec::new();
        let mut unique_to_self = Vec::new();
        let mut unique_to_other = Vec::new();

        for &taxon_id in self.taxon_counts.keys() {
            if other.taxon_counts.contains_key(&taxon_id) {
                common_taxa.push(taxon_id);
            } else {
                unique_to_self.push(taxon_id);
            }
        }

        for &taxon_id in other.taxon_counts.keys() {
            if !self.taxon_counts.contains_key(&taxon_id) {
                unique_to_other.push(taxon_id);
            }
        }

        CoverageComparison {
            db1: self.database.clone(),
            db2: other.database.clone(),
            common_taxa_count: common_taxa.len(),
            unique_to_db1: unique_to_self.len(),
            unique_to_db2: unique_to_other.len(),
            common_taxa,
            unique_to_self,
            unique_to_other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageComparison {
    pub db1: String,
    pub db2: String,
    pub common_taxa_count: usize,
    pub unique_to_db1: usize,
    pub unique_to_db2: usize,
    pub common_taxa: Vec<u32>,
    pub unique_to_self: Vec<u32>,
    pub unique_to_other: Vec<u32>,
}

/// Format a taxon node as an ASCII tree
pub fn format_tree(
    node: &TaxonNode,
    prefix: &str,
    is_last: bool,
    max_depth: Option<usize>,
    current_depth: usize,
) -> String {
    let mut result = String::new();

    // Add the branch characters
    if current_depth > 0 {
        result.push_str(prefix);
        if is_last {
            result.push_str("└── ");
        } else {
            result.push_str("├── ");
        }
    }

    // Add node information
    result.push_str(&format!(
        "{} [{}] ({} seqs, {} total)\n",
        node.name, node.rank, node.sequence_count, node.cumulative_count
    ));

    // Check depth limit
    if let Some(max) = max_depth {
        if current_depth >= max {
            if !node.children.is_empty() {
                result.push_str(&format!(
                    "{}    ... ({} children)\n",
                    prefix,
                    node.children.len()
                ));
            }
            return result;
        }
    }

    // Add children
    let child_count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let is_last_child = i == child_count - 1;
        let child_prefix = if current_depth == 0 {
            String::new()
        } else {
            format!("{}{}    ", prefix, if is_last { " " } else { "│" })
        };

        result.push_str(&format_tree(
            child,
            &child_prefix,
            is_last_child,
            max_depth,
            current_depth + 1,
        ));
    }

    result
}
