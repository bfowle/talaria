# Talaria Architecture Documentation

## Overview

Talaria is a high-performance bioinformatics toolkit designed for efficient sequence database management, reduction, and analysis. The system is built on a modular, trait-based architecture that provides flexibility, extensibility, and type safety.

## Core Design Principles

1. **Trait-Based Abstraction**: All major components are defined through traits, enabling polymorphism and easy extension
2. **Content-Addressed Storage**: Using CASG (Content-Addressed Sequence Graph) for efficient deduplication
3. **Taxonomy-Aware Processing**: Deep integration of taxonomic information throughout the pipeline
4. **Delta Encoding**: Efficient storage through delta compression relative to reference sequences
5. **Parallel Processing**: Leveraging Rust's concurrency features for high performance

## Trait Architecture

The trait system is the foundation of Talaria's architecture, providing clean interfaces between components:

### 1. Aligner Traits (`src/tools/traits.rs`)

```rust
pub trait Aligner: Send + Sync {
    fn search(&self, query: &[Sequence], reference: &[Sequence]) -> Result<Vec<AlignmentResult>>;
    fn search_batched(&self, query: &[Sequence], reference: &[Sequence], batch_size: usize) -> Result<Vec<AlignmentResult>>;
    fn build_index(&self, reference_path: &Path, index_path: &Path) -> Result<()>;
    fn verify_installation(&self) -> Result<()>;
    fn supports_taxonomy(&self) -> bool;
    fn name(&self) -> &str;
}
```

**Purpose**: Provides a unified interface for different sequence alignment tools (LAMBDA, BLAST, DIAMOND, etc.)

**Implementations**:
- `LambdaAligner`: High-performance protein aligner with taxonomy support
- Future: `BlastAligner`, `DiamondAligner`, `MMseqs2Aligner`

### 2. Storage Trait Hierarchy (`src/storage/traits.rs`)

```rust
pub trait ChunkStorage: Send + Sync {
    fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;
    fn get_chunk(&self, hash: &SHA256Hash) -> Result<Vec<u8>>;
    fn has_chunk(&self, hash: &SHA256Hash) -> bool;
    fn enumerate_chunks(&self) -> Vec<ChunkInfo>;
}

pub trait DeltaStorage: ChunkStorage { ... }
pub trait ReductionStorage: DeltaStorage { ... }
pub trait TaxonomyStorage: ChunkStorage { ... }
pub trait RemoteStorage: ChunkStorage { ... }
pub trait StatefulStorage: ChunkStorage { ... }
```

**Purpose**: Abstracts storage backends for content-addressed chunks, enabling local, cloud, and hybrid storage

**Implementations**:
- `CASGStorage`: Primary implementation with all storage traits
- Future: `S3Storage`, `AzureStorage`, `GCSStorage`

### 3. Chunker Traits (`src/casg/chunker/traits.rs`)

```rust
pub trait Chunker: Send + Sync {
    fn chunk_sequences(&self, sequences: Vec<Sequence>) -> Result<Vec<TaxonomyAwareChunk>>;
    fn merge_chunks(&self, chunks: Vec<TaxonomyAwareChunk>) -> Result<TaxonomyAwareChunk>;
    fn split_chunk(&self, chunk: &TaxonomyAwareChunk, max_size: usize) -> Result<Vec<TaxonomyAwareChunk>>;
}

pub trait TaxonomyAwareChunker: Chunker { ... }
pub trait DeltaAwareChunker: Chunker { ... }
pub trait AdaptiveChunker: Chunker { ... }
```

**Purpose**: Defines how sequences are grouped into chunks for storage

**Implementations**:
- `TaxonomicChunker`: Groups sequences by taxonomy for better compression
- Future: `SizeBasedChunker`, `RandomChunker`, `ClusterBasedChunker`

### 4. Delta Generator Traits (`src/casg/delta/traits.rs`)

```rust
pub trait DeltaGenerator: Send + Sync {
    fn generate_delta_chunks(&mut self, sequences: &[Sequence], references: &[Sequence], reference_hash: SHA256Hash) -> Result<Vec<DeltaChunk>>;
    fn find_best_reference<'a>(&self, seq: &Sequence, references: &'a [Sequence]) -> Result<(&'a Sequence, f32)>;
    fn calculate_similarity(&self, seq1: &Sequence, seq2: &Sequence) -> f32;
}

pub trait DeltaReconstructor: Send + Sync { ... }
```

**Purpose**: Handles delta encoding and reconstruction for efficient storage

**Implementations**:
- `DeltaGenerator`: NW-based delta encoding
- Future: `FastDeltaGenerator`, `CompressedDeltaGenerator`

### 5. Manager Traits (`src/core/managers/traits.rs`)

```rust
pub trait Manager: Send + Sync {
    fn initialize(&mut self) -> Result<()>;
    fn verify(&self) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
    fn status(&self) -> Result<String>;
}

pub trait DatabaseManager: Manager { ... }
pub trait ToolManager: Manager { ... }
pub trait TaxonomyManager: Manager { ... }
```

**Purpose**: Manages lifecycle and operations of various system components

**Implementations**:
- `DatabaseManager`: CASG-based database management
- `ToolManager`: External tool installation and management
- Future: `TaxonomyManager`, `ConfigManager`, `CacheManager`

### 6. Selector Traits (`src/core/selection/traits.rs`)

```rust
pub trait ReferenceSelector: Send + Sync {
    fn select_references(&self, sequences: Vec<Sequence>, target_ratio: f64) -> Result<SelectionResult>;
    fn calculate_coverage(&self, references: &[Sequence], all_sequences: &[Sequence]) -> f64;
}

pub trait AlignmentBasedSelector: ReferenceSelector { ... }
pub trait TaxonomyAwareSelector: ReferenceSelector { ... }
```

**Purpose**: Algorithms for selecting representative sequences

**Implementations**:
- `ReferenceSelector`: Simple greedy selection
- Future: `AlignmentBasedSelector`, `ClusteringSelector`, `MLBasedSelector`

### 7. Validator Traits (`src/core/validation/traits.rs`)

```rust
pub trait Validator: Send + Sync {
    fn validate(&self, target: &Path) -> Result<ValidationResult>;
    fn can_validate(&self, path: &Path) -> bool;
}

pub trait SequenceValidator: Validator { ... }
pub trait ChunkValidator: Validator { ... }
pub trait FastaValidator: Validator { ... }
```

**Purpose**: Data validation and integrity checking

**Implementations**:
- Future: `FastaValidator`, `ChunkValidator`, `DeltaValidator`

### 8. Processor Traits (`src/processing/traits.rs`)

```rust
pub trait SequenceProcessor: Send + Sync {
    fn process(&self, sequences: &mut [Sequence]) -> Result<ProcessingResult>;
    fn supports_type(&self, seq_type: SequenceType) -> bool;
}

pub trait BatchProcessor: SequenceProcessor { ... }
pub trait FilterProcessor: SequenceProcessor { ... }
```

**Purpose**: Sequence processing pipelines

**Implementations**:
- Future: `QualityFilter`, `TaxonomyEnricher`, `LowComplexityFilter`

### 9. Reporter Traits (`src/report/traits.rs`)

```rust
pub trait Reporter: Send + Sync {
    fn generate(&self, data: &ReportData) -> Result<String>;
    fn format(&self) -> ReportFormat;
    fn export(&self, content: &str, output: &Path) -> Result<()>;
}

pub trait InteractiveReporter: Reporter { ... }
pub trait StreamingReporter: Reporter { ... }
```

**Purpose**: Report generation in various formats

**Implementations**:
- `HtmlReporter`, `JsonReporter`, `TextReporter`
- Future: `MarkdownReporter`, `PdfReporter`

## Component Architecture

### CASG (Content-Addressed Sequence Graph)

The heart of Talaria's storage system:

```
┌─────────────────────────────────────────┐
│           CASG Repository               │
├─────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐    │
│  │   Storage    │  │   Manifest   │    │
│  │  (Chunks)    │  │  (Metadata)  │    │
│  └──────────────┘  └──────────────┘    │
│  ┌──────────────┐  ┌──────────────┐    │
│  │   Chunker    │  │  Assembler   │    │
│  │ (Taxonomic)  │  │   (FASTA)    │    │
│  └──────────────┘  └──────────────┘    │
│  ┌──────────────┐  ┌──────────────┐    │
│  │    Delta     │  │   Verifier   │    │
│  │  Generator   │  │  (Merkle)    │    │
│  └──────────────┘  └──────────────┘    │
└─────────────────────────────────────────┘
```

### Data Flow

1. **Input**: FASTA sequences from databases (UniProt, NCBI)
2. **Chunking**: Sequences grouped by taxonomy
3. **Delta Encoding**: Non-reference sequences encoded as deltas
4. **Storage**: Content-addressed storage with deduplication
5. **Manifest**: Metadata and Merkle trees for verification
6. **Output**: Reduced FASTA with delta metadata

### Processing Pipeline

```
Input FASTA
    ↓
[SequenceProcessor] → Filter/Transform/Enrich
    ↓
[ReferenceSelector] → Select Representatives
    ↓
[DeltaGenerator] → Encode Non-References
    ↓
[Chunker] → Group by Taxonomy
    ↓
[Storage] → Content-Addressed Storage
    ↓
[Reporter] → Generate Reports
    ↓
Output (Reduced FASTA + Deltas)
```

## Key Design Patterns

### 1. Trait Objects for Runtime Polymorphism

```rust
let aligners: Vec<Box<dyn Aligner>> = vec![
    Box::new(LambdaAligner::new()),
    Box::new(BlastAligner::new()),
];

for aligner in aligners {
    let results = aligner.search(&query, &reference)?;
}
```

### 2. Builder Pattern for Complex Objects

```rust
let reducer = Reducer::new(config)
    .with_selection_mode(true, true)
    .with_no_deltas(false)
    .with_taxonomy_weights(true)
    .with_manifest_acc2taxid(Some(path));
```

### 3. Strategy Pattern via Traits

```rust
impl ReferenceSelector for GreedySelector { ... }
impl ReferenceSelector for AlignmentBasedSelector { ... }
impl ReferenceSelector for MLBasedSelector { ... }
```

### 4. Chain of Responsibility for Processing

```rust
let pipeline = ProcessingPipeline::new()
    .add_processor(Box::new(QualityFilter::new()))
    .add_processor(Box::new(TaxonomyEnricher::new()))
    .add_processor(Box::new(LowComplexityFilter::new()));
```

## Performance Considerations

1. **Zero-Cost Abstractions**: Traits compile to static dispatch where possible
2. **Parallel Processing**: Using Rayon for data parallelism
3. **Streaming**: Large datasets processed in chunks
4. **Caching**: Content-addressed storage provides automatic deduplication
5. **Compression**: Delta encoding + gzip for storage efficiency

## Extensibility

Adding new functionality is straightforward:

### Adding a New Aligner

1. Implement the `Aligner` trait
2. Add to `Tool` enum if external
3. Update `ToolManager` for installation

### Adding a New Storage Backend

1. Implement `ChunkStorage` and related traits
2. Update `CloudConfig` enum if cloud-based
3. Add to storage factory function

### Adding a New Processor

1. Implement `SequenceProcessor` trait
2. Optional: Implement specialized traits (`FilterProcessor`, etc.)
3. Add to processing pipeline options

## Testing Strategy

1. **Unit Tests**: Each trait implementation has dedicated tests
2. **Integration Tests**: `tests/trait_tests.rs` verifies trait interactions
3. **Mock Implementations**: Test traits provide mock implementations
4. **Property-Based Testing**: Using proptest for complex algorithms
5. **Benchmarks**: Performance testing for critical paths

## Future Enhancements

1. **Plugin System**: Dynamic loading of trait implementations
2. **WebAssembly Support**: Run processors in WASM sandbox
3. **Distributed Processing**: Cluster-aware trait implementations
4. **Machine Learning**: ML-based selectors and processors
5. **Real-time Streaming**: Support for continuous data streams

## Conclusion

Talaria's trait-based architecture provides:

- **Flexibility**: Easy to extend and modify
- **Type Safety**: Compile-time guarantees
- **Performance**: Zero-cost abstractions
- **Testability**: Mock implementations for testing
- **Maintainability**: Clear separation of concerns

The system is designed to scale from single-machine analysis to distributed cloud deployments while maintaining a consistent, well-defined API through its trait system.