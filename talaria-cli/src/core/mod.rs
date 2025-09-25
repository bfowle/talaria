//! Core functionality for Talaria CLI

// Core submodules
pub mod database;
pub mod execution;
pub mod selection;
pub mod traits;
pub mod versioning;
pub mod workspace;

// Algorithm modules
pub mod reducer;
pub mod reference_selector;
pub mod reference_selector_optimized;
// delta_encoder is now in talaria-bio compression module
pub use talaria_bio::compression::delta as delta_encoder;
pub mod validator;

// Other modules
pub mod migrator;
pub mod resolver;
pub mod alias;
pub mod backup_manager;
pub mod phylogenetic_clusterer;
pub mod clustering_rules;

// Re-export commonly used types
