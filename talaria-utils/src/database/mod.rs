//! Database-related utilities

pub mod alias;
pub mod database_ref;
pub mod reference;
pub mod resolver;
pub mod version;
pub mod version_detector;
pub mod version_store;

// Re-export main types
pub use reference::DatabaseReference;
pub use version::{
    DatabaseVersion, VersionAliases, VersionDetector, VersionManager
};