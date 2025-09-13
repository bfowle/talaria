use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyInfo {
    pub taxon_id: u32,
    pub scientific_name: String,
    pub rank: String,
    pub parent_id: Option<u32>,
}

#[derive(Debug)]
pub struct TaxonomyDB {
    taxa: HashMap<u32, TaxonomyInfo>,
}

impl TaxonomyDB {
    pub fn new() -> Self {
        Self {
            taxa: HashMap::new(),
        }
    }
    
    pub fn add_taxon(&mut self, info: TaxonomyInfo) {
        self.taxa.insert(info.taxon_id, info);
    }
    
    pub fn get_taxon(&self, taxon_id: u32) -> Option<&TaxonomyInfo> {
        self.taxa.get(&taxon_id)
    }
    
    pub fn get_lineage(&self, taxon_id: u32) -> Vec<u32> {
        let mut lineage = Vec::new();
        let mut current_id = Some(taxon_id);
        
        while let Some(id) = current_id {
            lineage.push(id);
            current_id = self.taxa.get(&id).and_then(|t| t.parent_id);
        }
        
        lineage.reverse();
        lineage
    }
    
    pub fn common_ancestor(&self, taxon_a: u32, taxon_b: u32) -> Option<u32> {
        let lineage_a = self.get_lineage(taxon_a);
        let lineage_b = self.get_lineage(taxon_b);
        
        let mut common = None;
        for (a, b) in lineage_a.iter().zip(lineage_b.iter()) {
            if a == b {
                common = Some(*a);
            } else {
                break;
            }
        }
        
        common
    }
    
    pub fn distance(&self, taxon_a: u32, taxon_b: u32) -> Option<usize> {
        let lineage_a = self.get_lineage(taxon_a);
        let lineage_b = self.get_lineage(taxon_b);
        
        if let Some(common) = self.common_ancestor(taxon_a, taxon_b) {
            let dist_a = lineage_a.iter().position(|&x| x == common)?;
            let dist_b = lineage_b.iter().position(|&x| x == common)?;
            Some(lineage_a.len() - dist_a + lineage_b.len() - dist_b - 2)
        } else {
            None
        }
    }
}

/// Parse NCBI taxonomy dump files
pub mod ncbi {
    use super::*;
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::Path;
    
    pub fn load_names<P: AsRef<Path>>(path: P) -> Result<HashMap<u32, String>, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut names = HashMap::new();
        
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();
            
            if parts.len() >= 4 && parts[3].trim_end_matches("\t|") == "scientific name" {
                if let Ok(taxon_id) = parts[0].parse::<u32>() {
                    names.insert(taxon_id, parts[1].to_string());
                }
            }
        }
        
        Ok(names)
    }
    
    pub fn load_nodes<P: AsRef<Path>>(path: P) -> Result<HashMap<u32, (u32, String)>, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut nodes = HashMap::new();
        
        for line in reader.lines() {
            let line = line?;
            let parts: Vec<&str> = line.split("\t|\t").collect();
            
            if parts.len() >= 3 {
                if let (Ok(taxon_id), Ok(parent_id)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    let rank = parts[2].to_string();
                    nodes.insert(taxon_id, (parent_id, rank));
                }
            }
        }
        
        Ok(nodes)
    }
    
    pub fn build_taxonomy_db<P: AsRef<Path>>(names_path: P, nodes_path: P) -> Result<TaxonomyDB, std::io::Error> {
        let names = load_names(names_path)?;
        let nodes = load_nodes(nodes_path)?;
        
        let mut db = TaxonomyDB::new();
        
        for (taxon_id, name) in names {
            if let Some((parent_id, rank)) = nodes.get(&taxon_id) {
                let info = TaxonomyInfo {
                    taxon_id,
                    scientific_name: name,
                    rank: rank.clone(),
                    parent_id: if *parent_id == taxon_id { None } else { Some(*parent_id) },
                };
                db.add_taxon(info);
            }
        }
        
        Ok(db)
    }
}