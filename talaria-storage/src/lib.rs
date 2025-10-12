//! Storage backend implementations for Talaria
//!
//! This crate provides low-level storage backends and utilities for the Talaria project.
//! It focuses on providing efficient storage implementations (RocksDB, filesystem),
//! compression, caching, and I/O operations. Business logic and storage traits are
//! defined in talaria-herald to avoid circular dependencies.

pub mod backend;
pub mod cache;
pub mod compression;
pub mod core;
pub mod format;
pub mod index;
pub mod io;
pub mod optimization;
pub mod types;

// Re-export commonly used types from core
pub use core::types::{
    ChunkInfo, ChunkMetadata, ChunkType, DeltaChunk, GCResult, RemoteStatus, SHA256Hash,
    StorageStats, SyncResult, TaxonId, TaxonomyStats, VerificationError, VerificationErrorType,
};

// Re-export from index module
pub use index::{ChunkIndex, ChunkQuery, InMemoryChunkIndex, IndexStats};

// Re-export from cache module
pub use cache::{AlignmentCache, CachedAlignment};

// Re-export from optimization module
pub use optimization::{
    OptimizationOptions, OptimizationResult, StandardStorageOptimizer, StorageAnalysis,
    StorageOptimizer, StorageStrategy,
};

// Re-export from io module
pub use io::{load_metadata, load_ref2children, write_metadata, write_ref2children, DeltaRecord};
