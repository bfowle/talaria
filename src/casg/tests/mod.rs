mod manifest_test;
mod reconstruction_test;
mod resume_test;
mod update_test;
mod version_identification_test;

// Re-export for use in integration tests
pub use version_identification_test::{VersionIdentifier, VersionInfo};
