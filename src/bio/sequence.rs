use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sequence {
    pub id: String,
    pub description: Option<String>,
    pub sequence: Vec<u8>,
    pub taxon_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SequenceType {
    Protein,
    Nucleotide,
}

impl Sequence {
    pub fn new(id: String, sequence: Vec<u8>) -> Self {
        Self {
            id,
            description: None,
            sequence,
            taxon_id: None,
        }
    }
    
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    
    pub fn with_taxon(mut self, taxon_id: u32) -> Self {
        self.taxon_id = Some(taxon_id);
        self
    }
    
    pub fn len(&self) -> usize {
        self.sequence.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }
    
    pub fn detect_type(&self) -> SequenceType {
        let protein_chars = b"EFILPQXZ";
        let has_protein = self.sequence.iter()
            .any(|&c| protein_chars.contains(&c.to_ascii_uppercase()));
        
        if has_protein {
            SequenceType::Protein
        } else {
            SequenceType::Nucleotide
        }
    }
    
    pub fn to_string(&self) -> String {
        String::from_utf8_lossy(&self.sequence).to_string()
    }
    
    pub fn header(&self) -> String {
        let mut header = format!(">{}", self.id);

        if let Some(desc) = &self.description {
            header.push(' ');
            header.push_str(desc);
        }

        // Add TaxID to header if present and not already in description
        if let Some(taxon) = self.taxon_id {
            // Check if TaxID is already in the description to avoid duplication
            let has_taxid = self.description
                .as_ref()
                .map(|d| d.contains("TaxID=") || d.contains("OX=") || d.contains("taxon:"))
                .unwrap_or(false);

            if !has_taxid {
                header.push_str(&format!(" TaxID={}", taxon));
            }
        }

        header
    }
}