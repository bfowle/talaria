pub mod metadata;

// Re-export commonly used types and functions
pub use metadata::{
    load_metadata, load_ref2children, write_metadata, write_ref2children, DeltaRecord,
};
