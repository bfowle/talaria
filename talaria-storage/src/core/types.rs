#![allow(dead_code)]

// Storage-specific types (core types are now in talaria-core)

use serde::{Deserialize, Serialize};

// Re-export core types from talaria-core
pub use talaria_core::{
    ChunkInfo, ChunkMetadata, ChunkType as CoreChunkType, DeltaChunk, GCResult, RemoteStatus,
    SHA256Hash, StorageStats, SyncResult, TaxonId, TaxonomyStats, VerificationError,
    VerificationErrorType,
};

// Placeholder types - these would come from talaria-sequoia if not for circular dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyAwareChunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo;
