use crate::types::TaxonId;
/// Taxonomy filter with boolean expression support
///
/// Supports expressions like:
/// - "Bacteria" - single taxon
/// - "Bacteria AND NOT Escherichia" - boolean AND/NOT
/// - "9606 OR 10090" - numeric IDs with OR
/// - "(Bacteria OR Archaea) AND NOT Escherichia" - nested expressions
use anyhow::Result;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub enum TaxonomyFilter {
    TaxonId(TaxonId),
    Name(String),
    And(Box<TaxonomyFilter>, Box<TaxonomyFilter>),
    Or(Box<TaxonomyFilter>, Box<TaxonomyFilter>),
    Not(Box<TaxonomyFilter>),
}

impl TaxonomyFilter {
    /// Parse a filter expression string
    pub fn parse(expr: &str) -> Result<Self> {
        let expr = expr.trim();

        // Try parsing as simple taxon ID
        if let Ok(id) = expr.parse::<u32>() {
            return Ok(TaxonomyFilter::TaxonId(TaxonId(id)));
        }

        // Parse boolean expressions
        if let Some(filter) = Self::parse_expression(expr)? {
            return Ok(filter);
        }

        // Default to name lookup
        Ok(TaxonomyFilter::Name(expr.to_string()))
    }

    fn parse_expression(expr: &str) -> Result<Option<Self>> {
        let expr = expr.trim();

        // Handle parentheses
        if expr.starts_with('(') && expr.ends_with(')') {
            return Self::parse_expression(&expr[1..expr.len() - 1]);
        }

        // Parse NOT
        if expr.starts_with("NOT ") {
            let rest = &expr[4..];
            if let Some(filter) = Self::parse_expression(rest)? {
                return Ok(Some(TaxonomyFilter::Not(Box::new(filter))));
            }
        }

        // Parse AND (highest precedence after NOT)
        if let Some(pos) = expr.rfind(" AND ") {
            let left = &expr[..pos];
            let right = &expr[pos + 5..];

            if let (Some(l), Some(r)) = (
                Self::parse_expression(left)?,
                Self::parse_expression(right)?,
            ) {
                return Ok(Some(TaxonomyFilter::And(Box::new(l), Box::new(r))));
            }
        }

        // Parse OR (lowest precedence)
        if let Some(pos) = expr.rfind(" OR ") {
            let left = &expr[..pos];
            let right = &expr[pos + 4..];

            if let (Some(l), Some(r)) = (
                Self::parse_expression(left)?,
                Self::parse_expression(right)?,
            ) {
                return Ok(Some(TaxonomyFilter::Or(Box::new(l), Box::new(r))));
            }
        }

        // Try as taxon ID
        if let Ok(id) = expr.parse::<u32>() {
            return Ok(Some(TaxonomyFilter::TaxonId(TaxonId(id))));
        }

        // Handle AND NOT special case
        if let Some(pos) = expr.find(" AND NOT ") {
            let left = &expr[..pos];
            let right = &expr[pos + 9..];

            if let (Some(l), Some(r)) = (
                Self::parse_expression(left)?,
                Self::parse_expression(right)?,
            ) {
                return Ok(Some(TaxonomyFilter::And(
                    Box::new(l),
                    Box::new(TaxonomyFilter::Not(Box::new(r))),
                )));
            }
        }

        // Must be a taxon name
        if !expr.is_empty() {
            return Ok(Some(TaxonomyFilter::Name(expr.to_string())));
        }

        Ok(None)
    }

    /// Evaluate filter against a set of taxon IDs
    pub fn matches(&self, taxon_ids: &[TaxonId]) -> bool {
        let taxon_set: HashSet<_> = taxon_ids.iter().cloned().collect();
        self.evaluate(&taxon_set)
    }

    fn evaluate(&self, taxon_set: &HashSet<TaxonId>) -> bool {
        match self {
            TaxonomyFilter::TaxonId(id) => taxon_set.contains(id),
            TaxonomyFilter::Name(name) => {
                // Convert common names to IDs
                let id = match name.to_lowercase().as_str() {
                    "bacteria" => TaxonId(2),
                    "archaea" => TaxonId(2157),
                    "eukaryota" => TaxonId(2759),
                    "viruses" => TaxonId(10239),
                    "escherichia" => TaxonId(561),
                    "escherichia coli" | "e. coli" => TaxonId(562),
                    "homo sapiens" | "human" => TaxonId(9606),
                    "mus musculus" | "mouse" => TaxonId(10090),
                    "drosophila" | "drosophila melanogaster" => TaxonId(7227),
                    "arabidopsis" | "arabidopsis thaliana" => TaxonId(3702),
                    "saccharomyces cerevisiae" | "yeast" => TaxonId(559292),
                    _ => return false, // Unknown name
                };
                taxon_set.contains(&id)
            }
            TaxonomyFilter::And(left, right) => {
                left.evaluate(taxon_set) && right.evaluate(taxon_set)
            }
            TaxonomyFilter::Or(left, right) => {
                left.evaluate(taxon_set) || right.evaluate(taxon_set)
            }
            TaxonomyFilter::Not(filter) => !filter.evaluate(taxon_set),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_filters() {
        let filter = TaxonomyFilter::parse("2").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
        assert!(!filter.matches(&[TaxonId(3)]));

        let filter = TaxonomyFilter::parse("Bacteria").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
    }

    #[test]
    fn test_boolean_expressions() {
        // AND
        let filter = TaxonomyFilter::parse("Bacteria AND 561").unwrap();
        assert!(filter.matches(&[TaxonId(2), TaxonId(561)]));
        assert!(!filter.matches(&[TaxonId(2)]));

        // OR
        let filter = TaxonomyFilter::parse("Bacteria OR Archaea").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
        assert!(filter.matches(&[TaxonId(2157)]));
        assert!(!filter.matches(&[TaxonId(9606)]));

        // NOT
        let filter = TaxonomyFilter::parse("NOT Escherichia").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
        assert!(!filter.matches(&[TaxonId(561)]));

        // AND NOT
        let filter = TaxonomyFilter::parse("Bacteria AND NOT Escherichia").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
        assert!(!filter.matches(&[TaxonId(2), TaxonId(561)]));
    }

    #[test]
    fn test_complex_expressions() {
        let filter = TaxonomyFilter::parse("(Bacteria OR Archaea) AND NOT Escherichia").unwrap();
        assert!(filter.matches(&[TaxonId(2)]));
        assert!(filter.matches(&[TaxonId(2157)]));
        assert!(!filter.matches(&[TaxonId(2), TaxonId(561)]));
        assert!(!filter.matches(&[TaxonId(9606)]));
    }
}
