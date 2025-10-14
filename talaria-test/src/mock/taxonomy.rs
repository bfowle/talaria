//! Mock taxonomy manager for testing

use std::collections::HashMap;
use talaria_core::types::TaxonId;

/// Mock taxonomy entry
#[derive(Debug, Clone)]
pub struct MockTaxonomyEntry {
    pub taxon_id: TaxonId,
    pub name: String,
    pub rank: String,
    pub parent: Option<TaxonId>,
}

/// Mock taxonomy manager
pub struct MockTaxonomyManager {
    entries: HashMap<TaxonId, MockTaxonomyEntry>,
    name_to_id: HashMap<String, TaxonId>,
}

impl MockTaxonomyManager {
    /// Create a new mock taxonomy manager
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            name_to_id: HashMap::new(),
        }
    }

    /// Create with default test taxonomy
    pub fn with_defaults() -> Self {
        let mut manager = Self::new();

        // Add common test entries
        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(1),
            name: "root".to_string(),
            rank: "no rank".to_string(),
            parent: None,
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(2),
            name: "Bacteria".to_string(),
            rank: "superkingdom".to_string(),
            parent: Some(TaxonId(1)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(562),
            name: "Escherichia coli".to_string(),
            rank: "species".to_string(),
            parent: Some(TaxonId(561)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(543),
            name: "Enterobacteriaceae".to_string(),
            rank: "family".to_string(),
            parent: Some(TaxonId(1236)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(1236),
            name: "Gammaproteobacteria".to_string(),
            rank: "class".to_string(),
            parent: Some(TaxonId(1224)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(1224),
            name: "Proteobacteria".to_string(),
            rank: "phylum".to_string(),
            parent: Some(TaxonId(2)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(561),
            name: "Escherichia".to_string(),
            rank: "genus".to_string(),
            parent: Some(TaxonId(543)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(2157),
            name: "Archaea".to_string(),
            rank: "superkingdom".to_string(),
            parent: Some(TaxonId(1)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(2759),
            name: "Eukaryota".to_string(),
            rank: "superkingdom".to_string(),
            parent: Some(TaxonId(1)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(10239),
            name: "Viruses".to_string(),
            rank: "superkingdom".to_string(),
            parent: Some(TaxonId(1)),
        });

        manager.add_entry(MockTaxonomyEntry {
            taxon_id: TaxonId(9606),
            name: "Homo sapiens".to_string(),
            rank: "species".to_string(),
            parent: Some(TaxonId(9605)),
        });

        manager
    }

    /// Add a taxonomy entry
    pub fn add_entry(&mut self, entry: MockTaxonomyEntry) {
        self.name_to_id.insert(entry.name.clone(), entry.taxon_id);
        self.entries.insert(entry.taxon_id, entry);
    }

    /// Get entry by taxon ID
    pub fn get_entry(&self, taxon_id: TaxonId) -> Option<&MockTaxonomyEntry> {
        self.entries.get(&taxon_id)
    }

    /// Get taxon ID by name
    pub fn get_taxon_by_name(&self, name: &str) -> Option<TaxonId> {
        self.name_to_id.get(name).copied()
    }

    /// Check if taxon is ancestor of another
    pub fn is_ancestor(&self, ancestor: TaxonId, descendant: TaxonId) -> bool {
        let mut current = descendant;
        while let Some(entry) = self.get_entry(current) {
            if current == ancestor {
                return true;
            }
            match entry.parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
        false
    }

    /// Get lineage for a taxon
    pub fn get_lineage(&self, taxon_id: TaxonId) -> Vec<TaxonId> {
        let mut lineage = Vec::new();
        let mut current = taxon_id;

        while let Some(entry) = self.get_entry(current) {
            lineage.push(current);
            match entry.parent {
                Some(parent) => current = parent,
                None => break,
            }
        }

        lineage.reverse();
        lineage
    }

    /// Get all descendants of a taxon
    pub fn get_descendants(&self, taxon_id: TaxonId) -> Vec<TaxonId> {
        let mut descendants = Vec::new();

        for id in self.entries.keys() {
            if self.is_ancestor(taxon_id, *id) && *id != taxon_id {
                descendants.push(*id);
            }
        }

        descendants
    }
}

impl Default for MockTaxonomyManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_taxonomy() {
        let manager = MockTaxonomyManager::with_defaults();

        // Test lookup
        assert_eq!(
            manager.get_taxon_by_name("Escherichia coli"),
            Some(TaxonId(562))
        );

        // Test ancestor relationship
        assert!(manager.is_ancestor(TaxonId(2), TaxonId(562))); // Bacteria -> E. coli
        assert!(!manager.is_ancestor(TaxonId(2157), TaxonId(562))); // Archaea -/-> E. coli

        // Test lineage
        let lineage = manager.get_lineage(TaxonId(562));
        assert!(lineage.contains(&TaxonId(1))); // root
        assert!(lineage.contains(&TaxonId(561))); // Escherichia
        assert!(lineage.contains(&TaxonId(562))); // E. coli
    }
}
