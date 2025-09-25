//! Verification, validation, and integrity checking

pub mod merkle;
pub mod verifier;
pub mod validator;

// Re-export main types
pub use merkle::MerkleDAG;
pub use verifier::{SEQUOIAVerifier as Verifier, VerificationResult};
pub use validator::{ValidationResult, ValidationError, ValidationOptions, StandardTemporalManifestValidator as Validator};