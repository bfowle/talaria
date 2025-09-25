//! Database-related utilities

pub mod reference;
pub mod version;

// Re-export main types
pub use reference::DatabaseReference;
pub use version::{
    DatabaseVersion, VersionAliases, VersionDetector, VersionManager
};