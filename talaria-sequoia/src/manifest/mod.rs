//! Manifest management for SEQUOIA databases

pub mod core;
pub mod taxonomy;

// Re-export main types
pub use core::{Manifest, ManifestFormat};
pub use taxonomy::TaxonomyManifest;