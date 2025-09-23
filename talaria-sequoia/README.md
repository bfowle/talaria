# talaria-sequoia

Sequence Query Optimization with Indexed Architecture (SEQUOIA) - Advanced storage system for biological sequences.

## Overview

SEQUOIA is a revolutionary approach to biological sequence storage that combines:

- **Content Addressing**: SHA256-based deduplication
- **Merkle DAGs**: Cryptographic verification and integrity
- **Bi-Temporal Versioning**: Track both sequence and taxonomy evolution
- **Taxonomy-Aware Chunking**: Intelligent sequence organization
- **Evolution-Aware Delta Encoding**: Phylogenetic distance optimization

## Features

### SEQUOIA Repository
```rust
use talaria_sequoia::SEQUOIARepository;

// Initialize or open a repository
let repo = SEQUOIARepository::init(path)?;
let repo = SEQUOIARepository::open(path)?;

// Store sequences with automatic chunking
let chunks = repo.store_sequences(sequences)?;

// Extract by taxonomy
let ecoli_seqs = repo.extract_taxon("Escherichia coli")?;

// Verify integrity
let result = repo.verify()?;
assert!(result.is_valid());
```

### Advanced Chunking
```rust
use talaria_sequoia::{TaxonomicChunker, ChunkingStrategy, SpecialTaxon};

let strategy = ChunkingStrategy {
    target_chunk_size: 5 * 1024 * 1024, // 5MB
    taxonomic_coherence: 0.95, // 95% same taxonomy
    special_taxa: vec![
        SpecialTaxon::new(TaxonId(9606), "Human"),
        SpecialTaxon::new(TaxonId(562), "E. coli"),
    ],
    ..Default::default()
};

let chunker = TaxonomicChunker::new(strategy);
let chunks = chunker.chunk_sequences(sequences)?;
```

### Dual Merkle DAGs
```rust
use talaria_sequoia::{DualMerkleDAG, MerkleVerifiable};

let dual_dag = DualMerkleDAG::new();
dual_dag.add_sequence_chunk(chunk)?;
dual_dag.add_taxonomy_version(version)?;

// Generate bi-temporal proof
let proof = dual_dag.generate_proof(sequence_hash, taxonomy_version)?;
assert!(dual_dag.verify_proof(&proof));
```

### Bi-Temporal Versioning
```rust
use talaria_sequoia::{BiTemporalRepository, BiTemporalCoordinate};

let coord = BiTemporalCoordinate {
    sequence_time: "2024-01-01T00:00:00Z".parse()?,
    taxonomy_time: "2024-01-01T00:00:00Z".parse()?,
};

// Store with temporal metadata
repo.store_temporal(sequences, coord)?;

// Query at specific point in time
let historical = repo.query_at(coord)?;

// Track taxonomy evolution
let evolution = repo.track_taxon_evolution(TaxonId(562))?;
```

### Evolution-Aware Delta Encoding
```rust
use talaria_sequoia::{EvolutionAwareDeltaGenerator, PhylogeneticDistance};

let generator = EvolutionAwareDeltaGenerator::new(taxonomy_manager);
let distance_calc = PhylogeneticDistance::new(taxonomy_manager);

// Encode with phylogenetic optimization
let delta = generator.encode_with_evolution(reference, child)?;

// Distance-based reference selection
let best_ref = generator.select_optimal_reference(sequence, candidates)?;
```

## Architecture

### Multi-Objective Optimization
The chunking algorithm optimizes for:
1. **Size uniformity** - Balanced chunk sizes
2. **Taxonomic coherence** - Related sequences together
3. **Deduplication potential** - Maximum content reuse
4. **Access patterns** - Optimized for common queries
5. **Compression ratio** - Better compression within chunks

### Merkle DAG Structure
```
    Cross-Reference Root
         /            \
   Sequence Root    Taxonomy Root
       /    \          /    \
   Chunk1  Chunk2  TaxV1  TaxV2
```

## Key Components

- `SEQUOIARepository`: Main repository interface
- `SEQUOIAStorage`: Content-addressed storage layer
- `TaxonomicChunker`: Intelligent chunking algorithm
- `MerkleDAG`: Cryptographic verification
- `TemporalIndex`: Time-based queries
- `EvolutionTracker`: Taxonomy change tracking
- `FastaAssembler`: Sequence reconstruction

## Performance

- **50-70% size reduction** without information loss
- **Sub-second verification** of TB-scale databases
- **Parallel chunking** with rayon
- **Memory-efficient streaming** for large datasets
- **O(log n) proof generation** and verification

## Usage

Add to your `Cargo.toml`:
```toml
[dependencies]
talaria-sequoia = { path = "../talaria-sequoia" }
```

### As a Library
```rust
use talaria_sequoia::{SEQUOIARepository, ChunkingStrategy};

fn main() -> Result<()> {
    let repo = SEQUOIARepository::init("./data")?;
    let sequences = load_sequences()?;

    let chunks = repo.store_sequences(sequences)?;
    println!("Stored {} chunks", chunks.len());

    Ok(())
}
```

## License

MIT