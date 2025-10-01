pub mod core;
pub mod formatter;
pub mod stats;

// Re-export commonly used types
pub use core::{
    extract_accession_from_id, ncbi, parse_taxonomy_from_description, SequenceProvider,
    TaxonomicRank, TaxonomyConfidence, TaxonomyDB, TaxonomyDiscrepancy, TaxonomyEnrichable,
    TaxonomyInfo, TaxonomyResolution, TaxonomyResolver, TaxonomySource, TaxonomySources,
};
pub use formatter::{StandardTaxonomyFormatter, TaxonomyFormatter};
pub use stats::{format_tree, CoverageComparison, RankStats, TaxonNode, TaxonomyCoverage};
