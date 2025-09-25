pub mod core;
pub mod formatter;
pub mod stats;

// Re-export commonly used types
pub use core::{
    TaxonomyDB, TaxonomyInfo, TaxonomySources, TaxonomicRank,
    TaxonomySource, TaxonomyConfidence, TaxonomyResolution,
    TaxonomyDiscrepancy, TaxonomyResolver, TaxonomyEnrichable,
    SequenceProvider, parse_taxonomy_from_description, extract_accession_from_id,
    ncbi
};
pub use formatter::{TaxonomyFormatter, StandardTaxonomyFormatter};
pub use stats::{TaxonomyCoverage, RankStats, TaxonNode, CoverageComparison, format_tree};