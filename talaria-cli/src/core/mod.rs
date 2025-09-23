//! Core functionality for Talaria CLI

// Algorithm modules
pub mod reducer;
pub mod reference_selector;
pub mod reference_selector_optimized;
pub mod selection;
// delta_encoder is now in talaria-bio
pub use talaria_bio::delta_encoder;
pub mod validator;

// Database modules
pub mod database_manager;
pub mod database_diff;
pub mod version_store;
pub mod migrator;
pub mod resolver;
pub mod alias;
pub mod backup_manager;

// Taxonomy modules
pub mod taxonomy_manager;
pub mod taxonomy_prerequisites;
pub mod phylogenetic_clusterer;
pub mod clustering_rules;

// Trait modules
pub mod storage_traits;
pub mod processing_traits;
pub mod report_traits;
pub mod tool_traits;
pub mod selection_traits;

// Utility modules
pub mod memory_estimator;

// Re-export commonly used types
