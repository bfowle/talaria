pub mod cache;
pub mod metadata;
pub mod traits;

pub use traits::{
    ChunkStorage, DeltaStorage, ReductionStorage, TaxonomyStorage,
    RemoteStorage, StatefulStorage, StorageStats, GCResult,
    VerificationError, VerificationErrorType,
};