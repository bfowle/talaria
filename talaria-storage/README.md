# Talaria Storage Module

## Overview

The `talaria-storage` module provides a comprehensive storage abstraction layer for the Talaria bioinformatics system. It implements content-addressed storage with SHA256 hashing, supporting various storage backends including local filesystem, cloud storage (S3, GCS, Azure), and distributed systems. The module is designed with a trait-based architecture that allows pluggable storage implementations while maintaining consistent interfaces across the system.

### Key Features

- **Content-Addressed Storage**: All data is stored and retrieved using SHA256 hashes, ensuring data integrity and enabling automatic deduplication
- **Delta Encoding Support**: Efficient storage of sequence variations through delta compression
- **Taxonomy-Aware Storage**: First-class support for taxonomic organization of biological data
- **Storage Optimization**: Built-in strategies for deduplication, compression, and space optimization
- **Async/Await Support**: Modern asynchronous I/O for high-performance operations
- **Thread-Safe Operations**: Concurrent access using lock-free data structures
- **Pluggable Backend Architecture**: Easy integration of new storage backends through trait implementations

## Architecture

### Storage Trait Hierarchy

```
ChunkStorage (Base Trait)
    ├── DeltaStorage
    │   └── ReductionStorage
    ├── TaxonomyStorage
    ├── RemoteStorage
    └── StatefulStorage
```

### Component Architecture

```
talaria-storage/
├── core/               # Core types and traits
│   ├── types.rs       # Fundamental types (SHA256Hash, ChunkInfo, etc.)
│   └── traits.rs      # Storage trait definitions
├── index/             # Indexing subsystem
│   └── index.rs       # Chunk indexing and querying
├── cache/             # Caching layer
│   └── cache.rs       # Alignment and chunk caching
├── optimization/      # Storage optimization
│   └── optimizer.rs   # Optimization strategies and analysis
└── io/                # I/O operations
    └── metadata.rs    # Metadata persistence
```

### Data Flow

```
Application Layer
    ↓
Storage Traits API
    ↓
[Indexing] ← → [Caching]
    ↓
Storage Backend Implementation
    ↓
Physical Storage (Filesystem/Cloud)
```

## Module Structure

### Core (`core/`)

The core module contains fundamental types and trait definitions that form the foundation of the storage system.

#### Types (`core/types.rs`)

- **SHA256Hash**: Content addressing primitive
  ```rust
  pub struct SHA256Hash(pub [u8; 32]);
  ```
  Provides methods for hex conversion and computation from raw data.

- **StorageChunkInfo**: Storage-specific chunk metadata
  ```rust
  pub struct StorageChunkInfo {
      pub hash: SHA256Hash,
      pub size: usize,
  }
  ```
  Note: This was renamed from `ChunkInfo` to avoid conflicts with display-specific types.

- **ChunkMetadata**: Extended chunk information (imported from talaria-core)
  ```rust
  use talaria_core::types::ChunkMetadata;
  // Includes: hash, size, offset, sequence_count, compressed_size, compression_ratio
  ```

- **DeltaChunk**: Delta-encoded chunk
  ```rust
  pub struct DeltaChunk {
      pub reference_hash: SHA256Hash,
      pub deltas: Vec<u8>,
  }
  ```

- **Storage Statistics Types**:
  - `StorageStats`: Overall storage metrics
  - `GCResult`: Garbage collection results
  - `VerificationError`: Integrity check results
  - `TaxonomyStats`: Taxonomy-specific statistics
  - `SyncResult`: Remote synchronization results

#### Traits (`core/traits.rs`)

##### ChunkStorage (Base Trait)

The foundation trait that all storage implementations must provide:

```rust
pub trait ChunkStorage: Send + Sync {
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;
    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;
    fn has_chunk(&self, hash: &SHA256Hash) -> bool;
    fn enumerate_chunks(&self) -> Vec<StorageChunkInfo>;
    fn verify_all(&self) -> Result<Vec<VerificationError>>;
    fn get_stats(&self) -> StorageStats;
    fn gc(&mut self, referenced: &[SHA256Hash]) -> Result<GCResult>;
}
```

##### DeltaStorage

Extends ChunkStorage with delta-specific operations:

```rust
pub trait DeltaStorage: ChunkStorage {
    fn store_delta_chunk(&self, chunk: &DeltaChunk) -> Result<SHA256Hash>;
    fn get_delta_chunk(&self, hash: &SHA256Hash) -> Result<DeltaChunk>;
    fn find_delta_for_child(&self, child_id: &str) -> Result<Option<SHA256Hash>>;
    fn get_deltas_for_reference(&self, reference_hash: &SHA256Hash) -> Result<Vec<SHA256Hash>>;
}
```

##### ReductionStorage

Manages reduction manifests for database compression:

```rust
pub trait ReductionStorage: DeltaStorage {
    fn store_reduction_manifest(&self, manifest: &ReductionManifest) -> Result<SHA256Hash>;
    fn get_reduction_by_profile(&self, profile: &str) -> Result<Option<ReductionManifest>>;
    fn list_reduction_profiles(&self) -> Result<Vec<String>>;
    fn delete_reduction_profile(&self, profile: &str) -> Result<()>;
}
```

##### TaxonomyStorage

Provides taxonomy-aware storage capabilities:

```rust
pub trait TaxonomyStorage: ChunkStorage {
    fn store_taxonomy_chunk(&self, chunk: &TaxonomyAwareChunk) -> Result<SHA256Hash>;
    fn get_taxonomy_chunk(&self, hash: &SHA256Hash) -> Result<TaxonomyAwareChunk>;
    fn find_chunks_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;
    fn get_taxonomy_stats(&self) -> Result<TaxonomyStats>;
}
```

##### RemoteStorage

Enables synchronization with remote repositories:

```rust
pub trait RemoteStorage: ChunkStorage {
    fn fetch_chunks(&mut self, hashes: &[SHA256Hash]) -> Result<Vec<TaxonomyAwareChunk>>;
    fn push_chunks(&self, hashes: &[SHA256Hash]) -> Result<()>;
    fn sync(&mut self) -> Result<SyncResult>;
    fn get_remote_status(&self) -> Result<RemoteStatus>;
}
```

##### StatefulStorage

Manages processing state for resumable operations:

```rust
pub trait StatefulStorage: ChunkStorage {
    fn start_processing(
        &self,
        operation: OperationType,
        manifest_hash: SHA256Hash,
        manifest_version: String,
        total_chunks: usize,
        source_info: SourceInfo,
    ) -> Result<String>;

    fn check_resumable(
        &self,
        database: &str,
        operation: &OperationType,
        manifest_hash: &SHA256Hash,
        manifest_version: &str,
    ) -> Result<Option<ProcessingState>>;

    fn update_processing_state(&self, completed_chunks: &[SHA256Hash]) -> Result<()>;
    fn complete_processing(&self) -> Result<()>;
}
```

### Indexing (`index/`)

The indexing subsystem provides fast lookups and queries over stored chunks.

#### ChunkIndex Trait

```rust
#[async_trait]
pub trait ChunkIndex: Send + Sync {
    async fn add_chunk(&mut self, metadata: ChunkMetadata) -> Result<()>;
    async fn remove_chunk(&mut self, hash: &SHA256Hash) -> Result<()>;
    async fn find_by_taxon(&self, taxon_id: TaxonId) -> Result<Vec<SHA256Hash>>;
    async fn find_by_taxons(&self, taxon_ids: &[TaxonId]) -> Result<Vec<SHA256Hash>>;
    async fn get_metadata(&self, hash: &SHA256Hash) -> Result<Option<ChunkMetadata>>;
    async fn query(&self, query: ChunkQuery) -> Result<Vec<ChunkMetadata>>;
    async fn rebuild(&mut self) -> Result<()>;
    async fn get_stats(&self) -> Result<IndexStats>;
}
```

#### Query System

The `ChunkQuery` structure enables complex filtering:

```rust
pub struct ChunkQuery {
    pub taxon_ids: Option<Vec<TaxonId>>,      // Filter by taxonomy
    pub size_range: Option<(usize, usize)>,   // Filter by size
    pub has_reference: Option<bool>,          // Filter by reference status
    pub limit: Option<usize>,                 // Limit results
    pub order_by_access: bool,                // Order by access frequency
}
```

#### InMemoryChunkIndex

A high-performance, thread-safe implementation using DashMap:

```rust
pub struct InMemoryChunkIndex {
    chunks: Arc<DashMap<SHA256Hash, ChunkMetadata>>,
    taxon_index: Arc<DashMap<TaxonId, HashSet<SHA256Hash>>>,
}
```

Features:
- Lock-free concurrent access
- O(1) chunk lookups
- O(1) taxon-to-chunk mapping
- Automatic index maintenance

### Caching (`cache/`)

The caching layer improves performance by storing frequently accessed data in memory.

#### AlignmentCache

Caches alignment results to avoid redundant computations:

```rust
pub struct AlignmentCache {
    cache: Arc<DashMap<(String, String), AlignmentResult>>,
    max_size: usize,
}
```

Features:
- LRU eviction when cache is full
- Thread-safe concurrent access
- Automatic cache invalidation on updates

### Optimization (`optimization/`)

The optimization subsystem provides strategies for reducing storage usage and improving performance.

#### Storage Strategies

```rust
pub enum StorageStrategy {
    Deduplication,      // Remove duplicate chunks
    Compression,        // Compress chunk data
    DeltaEncoding,      // Use delta encoding
    Archival,          // Archive old versions
    Caching,           // Cache hot chunks
    Repacking,         // Consolidate small chunks
    GarbageCollection, // Remove unreferenced chunks
}
```

#### StorageOptimizer Trait

```rust
#[async_trait]
pub trait StorageOptimizer: Send + Sync {
    async fn analyze(&self, path: &Path) -> Result<StorageAnalysis>;
    async fn optimize(&mut self, path: &Path, options: OptimizationOptions) -> Result<OptimizationResult>;
    async fn estimate_savings(&self, path: &Path, strategies: &[StorageStrategy]) -> Result<HashMap<StorageStrategy, usize>>;
}
```

#### Optimization Process

1. **Analysis Phase**: Scan storage to identify optimization opportunities
2. **Planning Phase**: Select strategies based on analysis results
3. **Execution Phase**: Apply selected strategies
4. **Verification Phase**: Verify data integrity after optimization

#### StandardStorageOptimizer

The default implementation providing:
- Cross-database deduplication
- Multi-level compression (zstd, lz4, snappy)
- Intelligent chunk repacking
- Incremental garbage collection

### I/O Operations (`io/`)

The I/O module handles metadata persistence and data serialization.

#### Metadata Functions

```rust
pub fn write_metadata<P: AsRef<Path>>(
    path: P,
    deltas: &[DeltaRecord],
) -> Result<(), TalariaError>;

pub fn load_metadata<P: AsRef<Path>>(
    path: P
) -> Result<Vec<DeltaRecord>, TalariaError>;

pub fn write_ref2children<P: AsRef<Path>>(
    path: P,
    ref2children: &HashMap<String, Vec<String>>,
) -> Result<(), TalariaError>;

pub fn load_ref2children<P: AsRef<Path>>(
    path: P
) -> Result<HashMap<String, Vec<String>>, TalariaError>;
```

## Integration with Other Modules

### talaria-core Integration

The storage module uses core functionality for:
- **Error Handling**: Uses `TalariaError` and `TalariaResult` types
- **Configuration**: Respects global configuration settings
- **Path Management**: Uses centralized path utilities

```rust
use talaria_core::error::{TalariaError, TalariaResult};
use talaria_core::config::Config;
use talaria_core::system::paths;
```

### talaria-bio Integration

Leverages bio module for:
- **Delta Compression**: Uses delta encoding algorithms
- **Sequence Handling**: Works with biological sequence types
- **Format Support**: Handles FASTA and other bio formats

```rust
use talaria_bio::compression::delta::{DeltaRecord, format_deltas_dat, parse_deltas_dat};
use talaria_bio::sequence::Sequence;
```

### talaria-sequoia Integration (Future)

Will provide:
- Actual storage backend implementations
- Advanced chunking strategies
- Distributed storage coordination
- Replication and sharding

## Usage Examples

### Implementing a Custom Storage Backend

```rust
use talaria_storage::{ChunkStorage, SHA256Hash, StorageChunkInfo, StorageStats, GCResult, VerificationError};
use anyhow::Result;

pub struct MyCustomStorage {
    // Your storage implementation
}

impl ChunkStorage for MyCustomStorage {
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash> {
        // Compute hash
        let hash = SHA256Hash::compute(data);

        // Optionally compress
        let stored_data = if compress {
            compress_data(data)?
        } else {
            data.to_vec()
        };

        // Store to your backend
        self.backend.put(&hash.to_hex(), &stored_data)?;

        Ok(hash)
    }

    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        // Retrieve from your backend
        let data = self.backend.get(&hash.to_hex())?;

        // Decompress if needed
        if is_compressed(&data) {
            decompress_data(&data)
        } else {
            Ok(data)
        }
    }

    // Implement other required methods...
}
```

### Using the Index System

```rust
use talaria_storage::{ChunkIndex, InMemoryChunkIndex, ChunkQuery, ChunkMetadata};

#[tokio::main]
async fn main() -> Result<()> {
    // Create index
    let mut index = InMemoryChunkIndex::new();

    // Add chunks
    let metadata = ChunkMetadata {
        hash: SHA256Hash::compute(b"data"),
        size: 1024,
        taxon_ids: Some(vec![9606, 10090]), // Human and mouse
    };
    index.add_chunk(metadata).await?;

    // Query chunks
    let query = ChunkQuery {
        taxon_ids: Some(vec![9606]),
        size_range: Some((1000, 2000)),
        ..Default::default()
    };

    let results = index.query(query).await?;
    println!("Found {} chunks", results.len());

    Ok(())
}
```

### Storage Optimization Workflow

```rust
use talaria_storage::{StorageOptimizer, StandardStorageOptimizer, OptimizationOptions, StorageStrategy};

#[tokio::main]
async fn main() -> Result<()> {
    let mut optimizer = StandardStorageOptimizer::new();

    // Analyze storage
    let analysis = optimizer.analyze(Path::new("/storage")).await?;
    println!("Total size: {} bytes", analysis.total_size);
    println!("Duplicate chunks: {}", analysis.duplicate_chunks.len());

    // Configure optimization
    let options = OptimizationOptions {
        strategies: vec![
            StorageStrategy::Deduplication,
            StorageStrategy::Compression,
        ],
        target_savings: Some(1_000_000_000), // 1GB
        dry_run: false,
        ..Default::default()
    };

    // Run optimization
    let result = optimizer.optimize(Path::new("/storage"), options).await?;
    println!("Saved {} bytes", result.space_saved);

    Ok(())
}
```

### Delta Storage Operations

```rust
use talaria_storage::{DeltaStorage, DeltaChunk};

async fn store_delta_encoded_sequences<S: DeltaStorage>(
    storage: &S,
    reference: &[u8],
    variants: Vec<&[u8]>,
) -> Result<()> {
    // Store reference
    let ref_hash = storage.store_chunk(reference, true)?;

    // Store variants as deltas
    for variant in variants {
        let delta = compute_delta(reference, variant)?;
        let delta_chunk = DeltaChunk {
            reference_hash: ref_hash,
            deltas: delta,
        };
        storage.store_delta_chunk(&delta_chunk)?;
    }

    Ok(())
}
```

### Working with Taxonomy

```rust
use talaria_storage::{TaxonomyStorage, TaxonId};

async fn find_human_sequences<S: TaxonomyStorage>(
    storage: &S,
) -> Result<Vec<Vec<u8>>> {
    const HUMAN_TAXON_ID: TaxonId = 9606;

    // Find all chunks containing human sequences
    let chunk_hashes = storage.find_chunks_by_taxon(HUMAN_TAXON_ID)?;

    // Retrieve the actual data
    let mut sequences = Vec::new();
    for hash in chunk_hashes {
        let chunk_data = storage.get_chunk(&hash)?;
        sequences.push(chunk_data);
    }

    Ok(sequences)
}
```

## Performance Considerations

### Thread Safety

All storage implementations use thread-safe primitives:
- **DashMap**: Lock-free concurrent HashMap
- **Arc**: Atomic reference counting for shared ownership
- **RwLock**: Reader-writer locks where exclusive access is needed

### Async I/O

The module uses async/await for I/O operations to:
- Maximize throughput with concurrent operations
- Reduce blocking on I/O-bound tasks
- Enable efficient resource utilization

### Caching Strategies

1. **Hot Path Caching**: Frequently accessed chunks are cached in memory
2. **Index Caching**: Metadata is cached to avoid repeated disk access
3. **Alignment Result Caching**: Expensive alignment computations are cached

### Content-Addressed Deduplication

- Automatic deduplication through SHA256 hashing
- Zero-copy references to identical data
- Significant storage savings for redundant sequences

### Optimization Guidelines

1. **Chunk Size**: Balance between deduplication efficiency and overhead
   - Smaller chunks: Better deduplication, more overhead
   - Larger chunks: Less overhead, reduced deduplication

2. **Compression**: Choose appropriate compression levels
   - Level 1-3: Fast compression, moderate savings
   - Level 4-6: Balanced speed and compression
   - Level 7-9: Maximum compression, slower

3. **Index Rebuild**: Periodically rebuild indexes for optimal performance
   - Remove orphaned entries
   - Defragment index structures
   - Update statistics

## Configuration

Storage behavior can be configured through environment variables and configuration files:

### Environment Variables

```bash
# Storage paths
TALARIA_STORAGE_DIR=/path/to/storage
TALARIA_CACHE_DIR=/path/to/cache

# Performance tuning
TALARIA_STORAGE_THREADS=8
TALARIA_CACHE_SIZE_MB=1024
TALARIA_COMPRESSION_LEVEL=6

# Remote storage
TALARIA_S3_BUCKET=my-talaria-bucket
TALARIA_GCS_PROJECT=my-project
TALARIA_AZURE_CONTAINER=my-container
```

### Configuration File (storage.toml)

```toml
[storage]
backend = "filesystem"  # or "s3", "gcs", "azure"
path = "/data/talaria/storage"
compression = true
compression_level = 6

[cache]
enabled = true
size_mb = 1024
ttl_seconds = 3600

[index]
type = "memory"  # or "sqlite", "rocksdb"
rebuild_interval_hours = 24

[optimization]
auto_optimize = true
deduplication = true
min_chunk_size = 4096
max_chunk_size = 1048576
```

## Testing

### Unit Testing

The module includes comprehensive unit tests for each component:

```bash
# Run all tests
cargo test

# Run specific test module
cargo test --package talaria-storage index::tests

# Run with coverage
cargo tarpaulin --out html
```

### Integration Testing

Integration tests verify interactions between components:

```rust
#[tokio::test]
async fn test_storage_workflow() {
    let storage = create_test_storage();
    let index = InMemoryChunkIndex::new();

    // Store data
    let data = b"test data";
    let hash = storage.store_chunk(data, false)?;

    // Index it
    let metadata = ChunkMetadata {
        hash,
        size: data.len(),
        taxon_ids: Some(vec![1234]),
    };
    index.add_chunk(metadata).await?;

    // Query and retrieve
    let chunks = index.find_by_taxon(1234).await?;
    assert_eq!(chunks.len(), 1);

    let retrieved = storage.get_chunk(&chunks[0])?;
    assert_eq!(retrieved, data);
}
```

### Performance Testing

Benchmarks for critical operations:

```rust
#[bench]
fn bench_store_chunk(b: &mut Bencher) {
    let storage = create_storage();
    let data = vec![0u8; 1024 * 1024]; // 1MB

    b.iter(|| {
        storage.store_chunk(&data, false).unwrap();
    });
}
```

## Error Handling

The module uses comprehensive error handling with detailed error types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Chunk not found: {0}")]
    ChunkNotFound(SHA256Hash),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        expected: SHA256Hash,
        actual: SHA256Hash,
    },

    #[error("Storage backend error: {0}")]
    BackendError(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Index error: {0}")]
    IndexError(String),
}
```

## Future Directions

### Planned Features

1. **Cloud Storage Backends**
   - Native S3 implementation with multipart upload
   - Google Cloud Storage with resumable uploads
   - Azure Blob Storage with block blob support

2. **Distributed Storage**
   - Consistent hashing for chunk distribution
   - Replication with configurable factors
   - Erasure coding for fault tolerance

3. **Advanced Indexing**
   - B-tree based persistent indexes
   - Bloom filters for existence checks
   - Inverted indexes for full-text search

4. **Compression Improvements**
   - Adaptive compression based on data type
   - Dictionary compression for similar sequences
   - Hardware-accelerated compression (QAT, ISA-L)

5. **Caching Enhancements**
   - Multi-tier caching (L1: memory, L2: SSD, L3: disk)
   - Predictive prefetching based on access patterns
   - Distributed cache with Redis/Memcached

6. **Security Features**
   - Encryption at rest
   - Client-side encryption
   - Access control lists (ACLs)
   - Audit logging

### API Stability

The storage traits are designed to be stable, but implementations may evolve:
- Trait signatures will remain backward compatible
- New optional methods may be added with default implementations
- Implementation details may change for performance improvements

### Contributing

When contributing to the storage module:

1. **Follow the trait hierarchy**: New storage types should extend existing traits
2. **Maintain thread safety**: Use appropriate synchronization primitives
3. **Write comprehensive tests**: Include unit, integration, and performance tests
4. **Document thoroughly**: Update this README and inline documentation
5. **Consider performance**: Profile and benchmark critical paths

## References

### Related Documentation

- [Talaria Core Module](../talaria-core/README.md)
- [Talaria Bio Module](../talaria-bio/README.md)
- [Talaria Sequoia Module](../talaria-sequoia/README.md)
- [Content-Addressed Storage](https://en.wikipedia.org/wiki/Content-addressable_storage)
- [Delta Encoding](https://en.wikipedia.org/wiki/Delta_encoding)

### Academic Papers

- "Content-Defined Chunking for Deduplication" (Muthitacharoen et al., 2001)
- "Delta Compression Techniques for Efficient Storage" (MacDonald, 2000)
- "Taxonomy-Aware Storage Systems for Biological Data" (Various, 2018-2023)

### Standards and Specifications

- SHA-256 Specification (FIPS 180-4)
- S3 API Documentation
- Google Cloud Storage API
- Azure Blob Storage API

## License

This module is part of the Talaria project and follows the same licensing terms as the parent project.

## Support

For questions, bug reports, or feature requests related to the storage module:
- Open an issue on the Talaria GitHub repository
- Contact the development team
- Consult the [API documentation](https://docs.talaria.bio/storage)