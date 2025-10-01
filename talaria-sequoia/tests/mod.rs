mod manifest_test;
// mod reconstruction_test;  // TODO: Re-enable when reconstruction is implemented
// mod resume_test;  // This is a standalone integration test file
mod update_test;
mod version_identification_test;

// Re-export for use in integration tests
pub use talaria_sequoia::temporal::VersionInfo;
pub use version_identification_test::VersionIdentifier;
