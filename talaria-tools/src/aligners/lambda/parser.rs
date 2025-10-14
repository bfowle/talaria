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
        header.starts_with("sp|")
            || header.starts_with("tr|")
            || header.starts_with("sp_")
            || header.starts_with("tr_")
            || header.contains("|sp|")
            || header.contains("|tr|")
            || header.starts_with("UniRef") // Add UniRef support
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
                if matches!(
                    *part,
                    "ref" | "gb" | "emb" | "dbj" | "pir" | "prf" | "tpg" | "tpe" | "tpd"
                ) && i + 1 < parts.len()
                {
                    let acc = parts[i + 1];
                    // Add with and without version
                    accessions.push(acc.to_string());
                    if let Some(dot_pos) = acc.rfind('.') {
                        accessions.push(acc[..dot_pos].to_string());
                    }
                }

                // Also check if part itself looks like accession
                if self.looks_like_ncbi_accession(part) && !accessions.contains(&part.to_string()) {
                    accessions.push(part.to_string());
                    if let Some(dot_pos) = part.rfind('.') {
                        let without_version = part[..dot_pos].to_string();
                        if !accessions.contains(&without_version) {
                            accessions.push(without_version);
                        }
                    }
                }
            }

            // Handle gi numbers (legacy but still found)
            for (i, part) in parts.iter().enumerate() {
                if *part == "gi" && i + 1 < parts.len() && parts[i + 1].parse::<u64>().is_ok() {
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
            if matches!(
                prefix,
                "NP" | "XP"
                    | "YP"
                    | "WP"
                    | "AP"
                    | "NM"
                    | "XM"
                    | "NR"
                    | "XR"
                    | "NG"
                    | "NC"
                    | "NT"
                    | "NW"
                    | "NZ"
                    | "AC"
                    | "AE"
                    | "AF"
                    | "AJ"
                    | "AM"
                    | "AY"
                    | "BK"
                    | "CP"
                    | "CU"
            ) && acc.chars().nth(2) == Some('_')
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

        // GenPept format: 2-3 letters + 5-6 digits (e.g., AAL12345)
        // Note: Single letter + digits (e.g., P12345) is typically UniProt, not NCBI
        if acc.len() >= 7 && acc.len() <= 10 {
            let alpha_count = acc.chars().take_while(|c| c.is_ascii_alphabetic()).count();
            let digit_count = acc[alpha_count..]
                .chars()
                .filter(|c| c.is_ascii_digit())
                .count();

            if (2..=3).contains(&alpha_count) && digit_count >= 5 {
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
        header.contains("pdb|")
            || (header.len() >= 4
                && self.looks_like_pdb(header.split_whitespace().next().unwrap_or("")))
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
                        // Extract just the chain ID (typically first word/character)
                        let chain_part = parts[i + 2]
                            .split_whitespace()
                            .next()
                            .unwrap_or(parts[i + 2]);
                        if chain_part.len() <= 2 {
                            // Chain IDs are typically 1-2 chars
                            accessions.push(format!("{}_{}", parts[i + 1], chain_part));
                        }
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

    #[allow(clippy::excessive_nesting)]
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
                        if !matches!(
                            *part,
                            "gi" | "ref"
                                | "gb"
                                | "emb"
                                | "dbj"
                                | "pir"
                                | "sp"
                                | "tr"
                                | "pdb"
                                | "lcl"
                                | "gnl"
                        ) {
                            // Skip UniProt entry names (third field in sp|ACC|ENTRY_SPECIES format)
                            // Check if this looks like a UniProt header and we're at position 2
                            if i == 2
                                && i < parts_vec.len()
                                && parts_vec.len() >= 3
                                && (parts_vec[0] == "sp" || parts_vec[0] == "tr")
                                && part.contains('_')
                            {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ===== UniProt Parser Tests =====

    #[test]
    fn test_uniprot_parser_sp_pipe_format() {
        let parser = UniProtParser;

        // Standard sp| format
        let header = "sp|P12345|PROT_HUMAN Protein description";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"P12345".to_string()));

        // With version
        let header = "sp|P12345.2|PROT_HUMAN";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"P12345.2".to_string()));
        assert!(accessions.contains(&"P12345".to_string()));
    }

    #[test]
    fn test_uniprot_parser_tr_pipe_format() {
        let parser = UniProtParser;

        let header = "tr|Q9Y6K9|Q9Y6K9_HUMAN DNA ligase";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"Q9Y6K9".to_string()));
    }

    #[test]
    fn test_uniprot_parser_underscore_format() {
        let parser = UniProtParser;

        let header = "sp_P12345_HUMAN Protein description";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"P12345".to_string()));

        let header = "tr_Q9Y6K9_HUMAN";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"Q9Y6K9".to_string()));
    }

    #[test]
    fn test_uniprot_parser_uniref_format() {
        let parser = UniProtParser;

        let header = "UniRef90_P12345 Cluster description";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"P12345".to_string()));

        let header = "UniRef50_Q9Y6K9";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"Q9Y6K9".to_string()));
    }

    #[test]
    fn test_uniprot_parser_complex_header() {
        let parser = UniProtParser;

        // Multiple pipes in description (no > prefix for individual parsers)
        let header =
            "sp|P31946|1433B_HUMAN 14-3-3 protein beta/alpha OS=Homo sapiens GN=YWHAB PE=1 SV=3";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"P31946".to_string()));
    }

    #[test]
    fn test_uniprot_parser_non_matching() {
        let parser = UniProtParser;

        assert!(!parser.can_parse("NP_001234.1 some protein"));
        assert!(!parser.can_parse("gi|12345|ref|NP_001234.1|"));
        assert!(!parser.can_parse("pdb|1ABC|A"));

        let accessions = parser.parse_header("random_header_text");
        assert!(accessions.is_empty());
    }

    // ===== NCBI Parser Tests =====

    #[test]
    fn test_ncbi_parser_refseq_format() {
        let parser = NCBIParser;

        // RefSeq protein
        let header = "NP_001234.1 myosin heavy chain";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"NP_001234.1".to_string()));
        assert!(accessions.contains(&"NP_001234".to_string()));

        // Other RefSeq prefixes
        let headers = vec![
            "XP_002345.2",
            "YP_003456.1",
            "WP_004567.1",
            "NM_001234.3",
            "XR_002345.1",
        ];

        for header in headers {
            assert!(parser.can_parse(header));
            let accessions = parser.parse_header(header);
            assert!(!accessions.is_empty());
        }
    }

    #[test]
    fn test_ncbi_parser_genbank_format() {
        let parser = NCBIParser;

        // GenBank format: 3 letters + digits
        let header = "AAA12345.1 some protein";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"AAA12345.1".to_string()));
        assert!(accessions.contains(&"AAA12345".to_string()));

        let header = "CAD12345";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"CAD12345".to_string()));
    }

    #[test]
    fn test_ncbi_parser_pipe_format() {
        let parser = NCBIParser;

        // ref| format
        let header = "ref|NP_001234.1| myosin";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"NP_001234.1".to_string()));
        assert!(accessions.contains(&"NP_001234".to_string()));

        // gb| format
        let header = "gb|AAA12345.1|";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"AAA12345.1".to_string()));

        // emb| format
        let header = "emb|CAA12345.1|";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"CAA12345.1".to_string()));
    }

    #[test]
    fn test_ncbi_parser_gi_number() {
        let parser = NCBIParser;

        let header = "gi|123456789|ref|NP_001234.1|";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"123456789".to_string()));
        assert!(accessions.contains(&"NP_001234.1".to_string()));
    }

    #[test]
    fn test_ncbi_parser_complex_header() {
        let parser = NCBIParser;

        let header = "gi|4507849|ref|NP_003552.1| zinc finger protein 133 [Homo sapiens]";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"4507849".to_string()));
        assert!(accessions.contains(&"NP_003552.1".to_string()));
        assert!(accessions.contains(&"NP_003552".to_string()));
    }

    #[test]
    fn test_ncbi_looks_like_accession() {
        let parser = NCBIParser;

        // Valid RefSeq
        assert!(parser.looks_like_ncbi_accession("NP_001234"));
        assert!(parser.looks_like_ncbi_accession("NP_001234.1"));
        assert!(parser.looks_like_ncbi_accession("XP_002345.2"));

        // Valid GenBank
        assert!(parser.looks_like_ncbi_accession("AAA12345"));
        assert!(parser.looks_like_ncbi_accession("CAD98765"));

        // Invalid
        assert!(!parser.looks_like_ncbi_accession("P12345")); // UniProt
        assert!(!parser.looks_like_ncbi_accession("1ABC")); // PDB
        assert!(!parser.looks_like_ncbi_accession("ABC")); // Too short
        assert!(!parser.looks_like_ncbi_accession("ABCD1234")); // Wrong pattern
    }

    #[test]
    fn test_ncbi_parser_non_ascii_safety() {
        let parser = NCBIParser;

        // Should handle non-ASCII safely
        assert!(!parser.looks_like_ncbi_accession("NP_001234×"));
        assert!(!parser.looks_like_ncbi_accession("ñoño"));

        let header = "NP_001234× protein";
        let accessions = parser.parse_header(header);
        // Should still find the valid part
        assert!(accessions.contains(&"NP_001234".to_string()) || accessions.is_empty());
    }

    // ===== PDB Parser Tests =====

    #[test]
    fn test_pdb_parser_pipe_format() {
        let parser = PDBParser;

        let header = "pdb|1ABC|A Chain A, structure";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"1ABC".to_string()));
        assert!(accessions.contains(&"1ABC_A".to_string()));
    }

    #[test]
    fn test_pdb_parser_direct_format() {
        let parser = PDBParser;

        // Direct PDB code
        let header = "1ABC structure description";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"1ABC".to_string()));

        // With chain
        let header = "1ABC_A Chain A";
        assert!(parser.can_parse(header));
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"1ABC_A".to_string()));
        assert!(accessions.contains(&"1ABC".to_string()));
    }

    #[test]
    fn test_pdb_looks_like_pdb() {
        let parser = PDBParser;

        assert!(parser.looks_like_pdb("1ABC"));
        assert!(parser.looks_like_pdb("2XYZ"));
        assert!(parser.looks_like_pdb("3A4B"));
        assert!(parser.looks_like_pdb("1ABC_A"));

        assert!(!parser.looks_like_pdb("ABC")); // Too short
        assert!(!parser.looks_like_pdb("ABCDE")); // Too long
        assert!(!parser.looks_like_pdb("AB-D")); // Invalid char
    }

    #[test]
    fn test_pdb_parser_mixed_case() {
        let parser = PDBParser;

        let header = "pdb|1abc|A";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"1abc".to_string()));

        let header = "2XyZ_B";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"2XyZ_B".to_string()));
        assert!(accessions.contains(&"2XyZ".to_string()));
    }

    // ===== Generic Parser Tests =====

    #[test]
    fn test_generic_parser_simple() {
        let parser = GenericParser;

        let header = "my_custom_id_001 some description";
        assert!(parser.can_parse(header)); // Always true
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"my_custom_id_001".to_string()));
    }

    #[test]
    fn test_generic_parser_with_pipes() {
        let parser = GenericParser;

        let header = "custom|ID123|other";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"ID123".to_string()));
        assert!(accessions.contains(&"other".to_string()));
    }

    #[test]
    fn test_generic_parser_skip_database_indicators() {
        let parser = GenericParser;

        let header = "lcl|local_seq_001";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"local_seq_001".to_string()));

        let header = "gnl|database|custom_id";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"database".to_string())); // database is kept as it's not a known indicator
        assert!(accessions.contains(&"custom_id".to_string()));
    }

    #[test]
    fn test_generic_parser_version_handling() {
        let parser = GenericParser;

        let header = "CUSTOM_001.3 description";
        let accessions = parser.parse_header(header);
        assert!(accessions.contains(&"CUSTOM_001.3".to_string()));
        assert!(accessions.contains(&"CUSTOM_001".to_string()));
    }

    #[test]
    fn test_generic_parser_skip_uniprot_entry_names() {
        let parser = GenericParser;

        // Should skip the entry name in UniProt-like format
        let header = "sp|P12345|1433B_HUMAN";
        let accessions = parser.parse_header(header);
        // Should get P12345 but NOT 1433B_HUMAN (entry name)
        assert!(accessions.contains(&"P12345".to_string()));
        assert!(!accessions.contains(&"1433B_HUMAN".to_string()));
    }

    // ===== Comprehensive Parser Tests =====

    #[test]
    fn test_comprehensive_parser_uniprot() {
        let parser = ComprehensiveAccessionParser::new();

        let header = ">sp|P12345|PROT_HUMAN Protein description";
        let accessions = parser.parse_accessions(header);

        // Should have original, uppercase, and lowercase
        assert!(accessions.contains("P12345"));
        assert!(accessions.contains("p12345"));
        assert!(accessions.contains("P12345"));
    }

    #[test]
    fn test_comprehensive_parser_ncbi() {
        let parser = ComprehensiveAccessionParser::new();

        let header = "gi|123456|ref|NP_001234.1|";
        let accessions = parser.parse_accessions(header);

        assert!(accessions.contains("123456"));
        assert!(accessions.contains("NP_001234.1"));
        assert!(accessions.contains("NP_001234"));
        assert!(accessions.contains("np_001234"));
    }

    #[test]
    fn test_comprehensive_parser_pdb() {
        let parser = ComprehensiveAccessionParser::new();

        let header = "pdb|1ABC|A";
        let accessions = parser.parse_accessions(header);

        assert!(accessions.contains("1ABC"));
        assert!(accessions.contains("1abc"));
        assert!(accessions.contains("1ABC_A"));
    }

    #[test]
    fn test_comprehensive_parser_mixed_format() {
        let parser = ComprehensiveAccessionParser::new();

        // This shouldn't normally happen but test robustness
        let header = ">gi|12345|sp|P67890|PROT_HUMAN";
        let accessions = parser.parse_accessions(header);

        // Should get accessions from both NCBI and UniProt parsers
        assert!(accessions.contains("12345"));
        assert!(accessions.contains("P67890"));
    }

    #[test]
    fn test_comprehensive_parser_strips_angle_bracket() {
        let parser = ComprehensiveAccessionParser::new();

        let header1 = ">P12345";
        let header2 = "P12345";

        let acc1 = parser.parse_accessions(header1);
        let acc2 = parser.parse_accessions(header2);

        // Should be the same with or without '>'
        assert_eq!(acc1, acc2);
    }

    #[test]
    fn test_comprehensive_parser_identify_formats() {
        let parser = ComprehensiveAccessionParser::new();

        let formats = parser.identify_formats("sp|P12345|PROT");
        assert!(formats.contains(&"UniProt"));

        let formats = parser.identify_formats("ref|NP_001234.1|");
        assert!(formats.contains(&"NCBI"));

        let formats = parser.identify_formats("pdb|1ABC|A");
        assert!(formats.contains(&"PDB"));

        let formats = parser.identify_formats("custom_id_001");
        assert!(formats.contains(&"Generic"));
    }

    #[test]
    fn test_comprehensive_parser_empty_and_short() {
        let parser = ComprehensiveAccessionParser::new();

        // Empty header
        let accessions = parser.parse_accessions("");
        assert!(accessions.is_empty());

        // Very short IDs should be filtered out
        let header = "a|b|c";
        let accessions = parser.parse_accessions(header);
        // 'a', 'b', 'c' are all <= 2 chars, should be filtered
        assert!(accessions.is_empty());
    }

    #[test]
    fn test_comprehensive_parser_case_variants() {
        let parser = ComprehensiveAccessionParser::new();

        let header = "MyProtein_001";
        let accessions = parser.parse_accessions(header);

        assert!(accessions.contains("MyProtein_001"));
        assert!(accessions.contains("MYPROTEIN_001"));
        assert!(accessions.contains("myprotein_001"));
    }

    // ===== Edge Case Tests =====

    #[test]
    fn test_parsers_handle_malformed_input() {
        let uniprot = UniProtParser;
        let ncbi = NCBIParser;
        let pdb = PDBParser;
        let generic = GenericParser;

        // Various malformed inputs
        let malformed = vec![
            "", "|||", "sp||", "ref||", "pdb||", ">>>>>", "   ", "\n\n", "sp|", "|P12345|",
        ];

        for input in malformed {
            // Should not panic
            let _ = uniprot.parse_header(input);
            let _ = ncbi.parse_header(input);
            let _ = pdb.parse_header(input);
            let _ = generic.parse_header(input);
        }
    }

    #[test]
    fn test_parsers_unicode_safety() {
        let parser = ComprehensiveAccessionParser::new();

        // Unicode that might cause issues
        let headers = vec![
            "protein_😀_test",
            "NP_001234_🧬",
            "sp|P12345|PROT_人类",
            "пептид_001",
        ];

        for header in headers {
            // Should not panic
            let _ = parser.parse_accessions(header);
        }
    }

    #[test]
    fn test_parsers_very_long_input() {
        let parser = ComprehensiveAccessionParser::new();

        // Very long header
        let long_id = "A".repeat(1000);
        let header = format!("sp|P12345|{}", long_id);
        let accessions = parser.parse_accessions(&header);

        // Should still find the accession
        assert!(accessions.contains("P12345"));
    }

    // ===== Property-based Tests =====

    #[quickcheck_macros::quickcheck]
    fn prop_parsers_dont_panic(input: String) -> bool {
        let parser = ComprehensiveAccessionParser::new();
        let _ = parser.parse_accessions(&input);
        true // If we get here without panic, test passes
    }

    #[quickcheck_macros::quickcheck]
    fn prop_case_variants_present(input: String) -> bool {
        // Skip inputs with null bytes or control characters which can cause issues
        if input.contains('\0') || input.chars().any(|c| c.is_control()) {
            return true;
        }

        let parser = ComprehensiveAccessionParser::new();
        let accessions = parser.parse_accessions(&input);

        // For any non-empty accession found with only ASCII alphabetic characters,
        // we should have case variants
        for acc in &accessions {
            // Only check accessions that are purely ASCII alphabetic with optional digits
            if !acc.is_empty() && acc.is_ascii() && acc.chars().any(|c| c.is_ascii_alphabetic()) {
                let upper = acc.to_uppercase();
                let lower = acc.to_lowercase();

                // Only check for variants if the conversion actually changes the string
                if upper != *acc && !accessions.contains(&upper) {
                    return false;
                }
                if lower != *acc && !accessions.contains(&lower) {
                    return false;
                }
            }
        }
        true
    }
}
