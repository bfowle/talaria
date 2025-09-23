# talaria-storage

Storage backend abstractions and implementations for Talaria.

## Overview

This crate provides a unified storage layer with multiple backend support:

- **Trait-based Architecture**: Flexible storage abstractions
- **Chunk Storage**: Content-addressed chunk management
- **Delta Storage**: Efficient delta-encoded sequence storage
- **Index Management**: Fast chunk lookups and queries
- **Optimization**: Storage optimization strategies
- **Caching**: Multi-level caching with various eviction policies

## Features

### Storage Traits
```rust
use talaria_storage::{ChunkStorage, StorageStats};

// Any storage backend implementing ChunkStorage
fn store_data<S: ChunkStorage>(storage: &S, data: &[u8]) -> Result<SHA256Hash> {
    let hash = storage.store_chunk(data, true)?; // with compression
    println!("Stats: {:?}", storage.get_stats());
    Ok(hash)
}
```

### Chunk Index
```rust
use talaria_storage::{InMemoryChunkIndex, ChunkQuery, ChunkMetadata};

let mut index = InMemoryChunkIndex::new();
index.insert(metadata).await?;

// Query chunks
let query = ChunkQuery::by_taxon(TaxonId(9606));
let chunks = index.query(query).await?;
```

### Storage Optimization
```rust
use talaria_storage::{StandardStorageOptimizer, StorageStrategy};

let optimizer = StandardStorageOptimizer::new();
let analysis = optimizer.analyze(storage_path).await?;

if analysis.fragmentation_ratio > 0.3 {
    let result = optimizer.optimize(storage_path, StorageStrategy::Compact).await?;
    println!("Reclaimed: {} bytes", result.space_reclaimed);
}
```

### Caching
```rust
use talaria_storage::cache::{AlignmentCache};

let cache = AlignmentCache::new(1000); // max 1000 entries
cache.insert("ref1", "query1", alignment_result);

if let Some(result) = cache.get("ref1", "query1") {
    // Cache hit - avoid recomputation
}
```

## Storage Backends

- **Local Filesystem**: Default local storage
- **S3 Compatible**: AWS S3 and compatible services
- **Memory**: In-memory storage for testing
- **Hybrid**: Multi-tier storage with hot/cold data

## Key Traits

- `ChunkStorage`: Basic chunk operations
- `DeltaStorage`: Delta-specific operations
- `RemoteStorage`: Cloud storage operations
- `StatefulStorage`: Transactional storage
- `ChunkIndex`: Indexing and querying

## Performance Features

- Parallel chunk verification
- Batch operations for efficiency
- Compression support (zstd, lz4, gzip)
- Memory-mapped I/O where applicable
- Connection pooling for remote storage

## Usage

Add to your `Cargo.toml`:
```toml
[dependencies]
talaria-storage = { path = "../talaria-storage" }
```

## License

MIT