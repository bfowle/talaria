pub mod metadata;

// Re-export commonly used types and functions
pub use metadata::{
    write_metadata, load_metadata,
    write_ref2children, load_ref2children,
    DeltaRecord,
};