/// Trait for formatting sequence headers with taxonomy information
///
/// This trait provides a consistent way to handle TaxID in FASTA headers,
/// preventing duplicate TaxID entries and ensuring consistent formatting.
pub trait TaxonomyFormatter {
    /// Format a sequence header with optional taxonomy ID
    ///
    /// This method ensures TaxID is only added once, even if it already exists
    /// in the description.
    fn format_header_with_taxid(
        &self,
        id: &str,
        description: Option<&str>,
        taxon_id: Option<u32>,
    ) -> String;

    /// Check if a description already contains a TaxID
    fn has_taxid(&self, description: &str) -> bool;

    /// Extract TaxID from a description if present
    fn extract_taxid(&self, description: &str) -> Option<u32>;
}

/// Default implementation of TaxonomyFormatter
pub struct StandardTaxonomyFormatter;

impl TaxonomyFormatter for StandardTaxonomyFormatter {
    fn format_header_with_taxid(
        &self,
        id: &str,
        description: Option<&str>,
        taxon_id: Option<u32>,
    ) -> String {
        let mut header = format!(">{}", id);

        if let Some(desc) = description {
            header.push(' ');
            header.push_str(desc);
        }

        // Only add TaxID if:
        // 1. We have a taxon_id
        // 2. The description doesn't already have ANY TaxID
        if let Some(tid) = taxon_id {
            // Check if description already has any TaxID
            if !description.map(|d| self.has_taxid(d)).unwrap_or(false) {
                header.push(' ');
                header.push_str(&format!("TaxID={}", tid));
            }
        }

        header
    }

    fn has_taxid(&self, description: &str) -> bool {
        description.contains("TaxID=") || description.contains("OX=")
    }

    fn extract_taxid(&self, description: &str) -> Option<u32> {
        // Look for TaxID=12345 pattern
        if let Some(pos) = description.find("TaxID=") {
            let start = pos + 6;
            let end = description[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|i| start + i)
                .unwrap_or(description.len());

            description[start..end].parse().ok()
        } else if let Some(pos) = description.find("OX=") {
            // Also check for OX= (UniProt format)
            let start = pos + 3;
            let end = description[start..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|i| start + i)
                .unwrap_or(description.len());

            description[start..end].parse().ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_without_existing_taxid() {
        let formatter = StandardTaxonomyFormatter;
        let header = formatter.format_header_with_taxid(
            "SEQ001",
            Some("Test sequence"),
            Some(12345),
        );
        assert_eq!(header, ">SEQ001 Test sequence TaxID=12345");
    }

    #[test]
    fn test_format_with_existing_taxid() {
        let formatter = StandardTaxonomyFormatter;
        let header = formatter.format_header_with_taxid(
            "SEQ001",
            Some("Test sequence TaxID=12345"),
            Some(12345),
        );
        assert_eq!(header, ">SEQ001 Test sequence TaxID=12345");
    }

    #[test]
    fn test_no_duplicate_taxid() {
        let formatter = StandardTaxonomyFormatter;
        let header = formatter.format_header_with_taxid(
            "SEQ001",
            Some("Test sequence TaxID=99999"),
            Some(12345),
        );
        // Should not add 12345 if description already has a different TaxID
        assert_eq!(header, ">SEQ001 Test sequence TaxID=99999");
    }

    #[test]
    fn test_extract_taxid() {
        let formatter = StandardTaxonomyFormatter;
        assert_eq!(
            formatter.extract_taxid("Some sequence TaxID=12345 more text"),
            Some(12345)
        );
        assert_eq!(
            formatter.extract_taxid("UniProt entry OX=9606 GN=GENE"),
            Some(9606)
        );
        assert_eq!(formatter.extract_taxid("No taxonomy here"), None);
    }
}