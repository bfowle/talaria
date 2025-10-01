//! Verification, validation, and integrity checking

pub mod merkle;
pub mod validator;
pub mod verifier;

// Re-export main types
pub use merkle::MerkleDAG;
pub use validator::{
    StandardTemporalManifestValidator as Validator, ValidationError, ValidationOptions,
    ValidationResult,
};
pub use verifier::{SequoiaVerifier as Verifier, VerificationResult};
