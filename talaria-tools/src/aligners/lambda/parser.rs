//! Accession parsers for various database formats
#![allow(dead_code)]

use std::collections::HashSet;

/// Trait for parsing accessions from different database formats
pub(super) trait AccessionParser: Send + Sync {
    /// Extract all possible accession forms from a header
    fn parse_header(&self, header: &str) -> Vec<String>;

    /// Check if this parser can handle the given header format
    fn can_parse(&self, header: &str) -> bool;
}

/// UniProt accession parser (sp|P12345|PROT_HUMAN, tr|Q12345|...)
pub(super) struct UniProtParser;

impl AccessionParser for UniProtParser {
    fn can_parse(&self, header: &str) -> bool {
        header.starts_with("sp|") || header.starts_with("tr|") ||
        header.starts_with("sp_") || header.starts_with("tr_") ||
        header.contains("|sp|") || header.contains("|tr|")
    }

    fn parse_header(&self, header: &str) -> Vec<String> {
        let mut accessions = Vec::new();

        // Handle pipe-delimited format: sp|P12345|PROT_HUMAN
        if header.contains('|') {
            let parts: Vec<&str> = header.split('|').collect();

            // Find sp or tr marker and take next part
            for (i, part) in parts.iter().enumerate() {
                if (*part == "sp" || *part == "tr") && i + 1 < parts.len() {
                    let acc = parts[i + 1].to_string();
                    accessions.push(acc.clone());

                    // Also store without any version suffix
                    if let Some(dot_pos) = acc.rfind('.') {
                        accessions.push(acc[..dot_pos].to_string());
                    }
                }
            }
        }

        // Handle underscore format: sp_P12345_HUMAN
        if header.contains('_') && (header.starts_with("sp_") || header.starts_with("tr_")) {
            let parts: Vec<&str> = header.split('_').collect();
            if parts.len() >= 2 {
                accessions.push(parts[1].to_string());
            }
        }

        // UniRef format: UniRef90_P12345
        if header.starts_with("UniRef") {
            if let Some(underscore_pos) = header.find('_') {
                // Make sure we're at a valid UTF-8 boundary
                if underscore_pos < header.len() && header.is_char_boundary(underscore_pos + 1) {
                    let acc = &header[underscore_pos + 1..];
                    if let Some(space_pos) = acc.find(' ') {
                        if acc.is_char_boundary(space_pos) {
                            accessions.push(acc[..space_pos].to_string());
                        }
                    } else {
                        accessions.push(acc.to_string());
                    }
                }
            }
        }

        accessions
    }
}

/// NCBI RefSeq/GenBank parser
pub(super) struct NCBIParser;

impl AccessionParser for NCBIParser {
    fn can_parse(&self, header: &str) -> bool {
        // Check for NCBI prefixes in pipe format
        header.contains("ref|") || header.contains("gb|") ||
        header.contains("emb|") || header.contains("dbj|") ||
        header.contains("gi|") || header.contains("pir|") ||
        header.contains("prf|") || header.contains("tpg|") ||
        header.contains("tpe|") || header.contains("tpd|") ||
        // Direct RefSeq/GenBank accessions
        self.looks_like_ncbi_accession(header.split_whitespace().next().unwrap_or(""))
    }

    fn parse_header(&self, header: &str) -> Vec<String> {
        let mut accessions = Vec::new();

        // Handle pipe-delimited NCBI format
        if header.contains('|') {
            let parts: Vec<&str> = header.split('|').collect();

            for (i, part) in parts.iter().enumerate() {
                // Look for database indicators
                if matches!(*part, "ref" | "gb" | "emb" | "dbj" | "pir" | "prf" | "tpg" | "tpe" | "tpd")
                    && i + 1 < parts.len()
                {
                    let acc = parts[i + 1];
                    // Add with and without version
                    accessions.push(acc.to_string());
                    if let Some(dot_pos) = acc.rfind('.') {
                        accessions.push(acc[..dot_pos].to_string());
                    }
                }

                // Also check if part itself looks like accession
                if self.looks_like_ncbi_accession(part) {
                    accessions.push(part.to_string());
                    if let Some(dot_pos) = part.rfind('.') {
                        accessions.push(part[..dot_pos].to_string());
                    }
                }
            }

            // Handle gi numbers (legacy but still found)
            for (i, part) in parts.iter().enumerate() {
                if *part == "gi" && i + 1 < parts.len()
                    && parts[i + 1].parse::<u64>().is_ok() {
                        accessions.push(parts[i + 1].to_string());
                    }
            }
        }

        // Check first word as potential direct accession
        if let Some(first_word) = header.split_whitespace().next() {
            if self.looks_like_ncbi_accession(first_word) {
                accessions.push(first_word.to_string());
                if let Some(dot_pos) = first_word.rfind('.') {
                    accessions.push(first_word[..dot_pos].to_string());
                }
            }
        }

        accessions
    }
}

impl NCBIParser {
    fn looks_like_ncbi_accession(&self, text: &str) -> bool {
        // Skip non-ASCII text early to avoid UTF-8 issues
        if !text.is_ascii() {
            return false;
        }

        // Remove version if present
        let acc = text.split('.').next().unwrap_or(text);

        // RefSeq patterns: NP_, XP_, YP_, WP_, AP_, NM_, XM_, NR_, XR_, etc.
        if acc.len() >= 3 && acc.is_char_boundary(2) {
            let prefix = &acc[..2];
            if matches!(prefix, "NP" | "XP" | "YP" | "WP" | "AP" |
                               "NM" | "XM" | "NR" | "XR" | "NG" | "NC" |
                               "NT" | "NW" | "NZ" | "AC" | "AE" | "AF" |
                               "AJ" | "AM" | "AY" | "BK" | "CP" | "CU")
                && acc.chars().nth(2) == Some('_')
            {
                return true;
            }
        }

        // GenBank protein: 3 letters + 5+ digits (e.g., AAA12345, CAA12345)
        if acc.len() >= 8 && acc.is_char_boundary(3) {
            let (prefix, suffix) = acc.split_at(3);
            if prefix.chars().all(|c| c.is_ascii_alphabetic())
                && suffix.chars().all(|c| c.is_ascii_digit())
                && suffix.len() >= 5
            {
                return true;
            }
        }

        // GenPept format: 1-2 letters + 5-6 digits (e.g., P12345, AAL12345)
        if acc.len() >= 6 && acc.len() <= 10 {
            let alpha_count = acc.chars().take_while(|c| c.is_ascii_alphabetic()).count();
            let digit_count = acc[alpha_count..].chars().filter(|c| c.is_ascii_digit()).count();

            if (1..=3).contains(&alpha_count) && digit_count >= 5 {
                return true;
            }
        }

        false
    }
}

/// PDB parser for Protein Data Bank accessions
pub(super) struct PDBParser;

impl AccessionParser for PDBParser {
    fn can_parse(&self, header: &str) -> bool {
        header.contains("pdb|") ||
        (header.len() >= 4 && self.looks_like_pdb(header.split_whitespace().next().unwrap_or("")))
    }

    fn parse_header(&self, header: &str) -> Vec<String> {
        let mut accessions = Vec::new();

        // Handle pdb|1ABC|A format
        if header.contains("pdb|") {
            let parts: Vec<&str> = header.split('|').collect();
            for (i, part) in parts.iter().enumerate() {
                if *part == "pdb" && i + 1 < parts.len() {
                    accessions.push(parts[i + 1].to_string());
                    // Sometimes chain is in next part
                    if i + 2 < parts.len() {
                        accessions.push(format!("{}_{}", parts[i + 1], parts[i + 2]));
                    }
                }
            }
        }

        // Check for direct PDB format: 1ABC_A or 1ABC
        if let Some(first) = header.split_whitespace().next() {
            if self.looks_like_pdb(first) {
                accessions.push(first.to_string());
                // Also try without chain
                if first.contains('_') {
                    accessions.push(first.split('_').next().unwrap_or("").to_string());
                }
            }
        }

        accessions
    }
}

impl PDBParser {
    fn looks_like_pdb(&self, text: &str) -> bool {
        // PDB codes are 4 alphanumeric characters, optionally with _chain
        let base = text.split('_').next().unwrap_or(text);
        base.len() == 4 && base.chars().all(|c| c.is_ascii_alphanumeric())
    }
}

/// Generic/fallback parser for custom formats
pub(super) struct GenericParser;

impl AccessionParser for GenericParser {
    fn can_parse(&self, _header: &str) -> bool {
        true // Can always try generic parsing
    }

    fn parse_header(&self, header: &str) -> Vec<String> {
        let mut accessions = Vec::new();

        // Take the first word as potential accession
        if let Some(first_word) = header.split_whitespace().next() {
            // Remove common prefixes
            let cleaned = first_word
                .trim_start_matches('>')
                .trim_start_matches("lcl|")
                .trim_start_matches("gnl|")
                .trim_start_matches("local|");

            // If it contains pipes, try to extract meaningful parts
            if cleaned.contains('|') {
                let parts_vec: Vec<&str> = cleaned.split('|').collect();

                for (i, part) in parts_vec.iter().enumerate() {
                    if !part.is_empty() && part.len() > 2 {
                        // Skip database indicators and very short strings
                        if !matches!(*part, "gi" | "ref" | "gb" | "emb" | "dbj" |
                                          "pir" | "sp" | "tr" | "pdb" | "lcl" | "gnl") {

                            // Skip UniProt entry names (third field in sp|ACC|ENTRY_SPECIES format)
                            // Check if this looks like a UniProt header and we're at position 2
                            if i == 2 && parts_vec.len() >= 3 &&
                               (parts_vec[0] == "sp" || parts_vec[0] == "tr") &&
                               part.contains('_') {
                                // This is likely an entry name like Q0NJW0_VARV, skip it
                                continue;
                            }

                            accessions.push(part.to_string());
                            // Also without version
                            if let Some(dot_pos) = part.rfind('.') {
                                accessions.push(part[..dot_pos].to_string());
                            }
                        }
                    }
                }
            } else if !cleaned.is_empty() && cleaned.len() > 2 {
                // Use as-is if it looks reasonable
                accessions.push(cleaned.to_string());
                // Also without version
                if let Some(dot_pos) = cleaned.rfind('.') {
                    accessions.push(cleaned[..dot_pos].to_string());
                }
            }
        }

        accessions
    }
}

/// Comprehensive accession parser that combines all parsers
pub(crate) struct ComprehensiveAccessionParser {
    parsers: Vec<Box<dyn AccessionParser>>,
}

impl ComprehensiveAccessionParser {
    pub(crate) fn new() -> Self {
        Self {
            parsers: vec![
                Box::new(UniProtParser),
                Box::new(NCBIParser),
                Box::new(PDBParser),
                Box::new(GenericParser),
            ],
        }
    }

    /// Parse a FASTA header and extract all possible accession forms
    pub(crate) fn parse_accessions(&self, header: &str) -> HashSet<String> {
        let mut all_accessions = HashSet::new();

        // Remove '>' if present
        let header = header.trim_start_matches('>');

        // Try each parser
        for parser in &self.parsers {
            if parser.can_parse(header) {
                for acc in parser.parse_header(header) {
                    if !acc.is_empty() && acc.len() > 2 {
                        // Store original
                        all_accessions.insert(acc.clone());

                        // Store uppercase version for case-insensitive matching
                        all_accessions.insert(acc.to_uppercase());

                        // Store lowercase version
                        all_accessions.insert(acc.to_lowercase());
                    }
                }
            }
        }

        all_accessions
    }

    /// Debug helper: show what formats were detected
    #[allow(dead_code)]
    pub(crate) fn identify_formats(&self, header: &str) -> Vec<&'static str> {
        let mut formats = Vec::new();
        let header = header.trim_start_matches('>');

        for (i, parser) in self.parsers.iter().enumerate() {
            if parser.can_parse(header) {
                let name = match i {
                    0 => "UniProt",
                    1 => "NCBI",
                    2 => "PDB",
                    _ => "Generic",
                };
                formats.push(name);
            }
        }

        formats
    }
}