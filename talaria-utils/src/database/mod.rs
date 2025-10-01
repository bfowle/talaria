//! Database-related utilities

pub mod database_ref;
pub mod version_detector;
pub mod version_store;

// Re-export main types
pub use database_ref::DatabaseReference;
pub use version_detector::{DatabaseVersion, VersionAliases, VersionDetector, VersionManager};
