# talaria-bio

## Overview

`talaria-bio` is the core bioinformatics library for the Talaria sequence database reduction system. It provides fundamental biological sequence processing capabilities including FASTA I/O, sequence alignment, taxonomy management, delta encoding for compression, and integration with external biological databases.

This module serves as the biological foundation layer that other Talaria components build upon, particularly `talaria-sequoia` (content-addressed storage), `talaria-cli` (command-line interface), and `talaria-tools` (external tool integration).

## Architecture

### Design Principles

1. **Separation of Concerns**: Each submodule handles a distinct aspect of biological data processing
2. **Zero-Copy Operations**: Where possible, operations avoid copying large sequence data
3. **Streaming Support**: Large FASTA files can be processed without loading entirely into memory
4. **Parallel Processing**: Built-in support for parallel sequence processing using Rayon
5. **Format Agnostic**: Core types (`Sequence`) are independent of file formats
6. **Taxonomy-Aware**: Deep integration with NCBI taxonomy for sequence classification

### Module Structure

```
talaria-bio/
├── src/
│   ├── lib.rs                     # Module declarations and re-exports
│   ├── alignment/                 # Sequence alignment algorithms
│   │   ├── mod.rs                 # Public alignment API
│   │   ├── nw_aligner.rs          # Needleman-Wunsch implementation
│   │   └── scoring.rs             # Scoring matrices (BLOSUM62, nucleotide)
│   ├── clustering/                # Sequence clustering algorithms
│   │   ├── mod.rs                 # Clustering API
│   │   └── phylogenetic.rs        # Phylogenetic clustering implementation
│   ├── compression/               # Delta encoding and compression
│   │   ├── mod.rs                 # Compression API
│   │   └── delta.rs               # Delta encoder/decoder for sequences
│   ├── formats/                   # File format I/O
│   │   ├── mod.rs                 # Format API
│   │   └── fasta.rs               # FASTA parser/writer (supports .gz)
│   ├── providers/                 # External database integration
│   │   ├── mod.rs                 # Provider traits
│   │   └── uniprot.rs             # UniProt API client
│   ├── sequence/                  # Core sequence types
│   │   ├── mod.rs                 # Sequence API
│   │   ├── types.rs               # Sequence struct and operations
│   │   └── stats.rs               # Sequence statistics computation
│   └── taxonomy/                  # Taxonomy management
│       ├── mod.rs                 # Taxonomy API
│       ├── core.rs                # TaxonomyDB, resolution, NCBI parser
│       ├── formatter.rs           # Header formatting with TaxID
│       ├── stats.rs               # Taxonomic coverage analysis
│       └── prerequisites.rs       # Taxonomy prerequisites checking
├── tests/
│   ├── alignment_integration.rs   # Comprehensive alignment tests
│   └── fasta_integration.rs       # FASTA I/O integration tests
└── benches/
    ├── alignment_bench.rs         # Alignment performance benchmarks
    └── fasta_bench.rs             # FASTA parsing benchmarks
```

## Core Components

### 1. Sequence Types (`sequence/`)

The fundamental data structure representing biological sequences:

```rust
pub struct Sequence {
    pub id: String,                        // Sequence identifier
    pub description: Option<String>,       // Optional description/header
    pub sequence: Vec<u8>,                 // Raw sequence data (amino acids/nucleotides)
    pub quality: Option<Vec<u8>>,          // Optional quality scores
    pub taxon_id: Option<u32>,             // NCBI taxonomy ID (authoritative)
    pub taxonomy_sources: TaxonomySources, // Multi-source taxonomy tracking
    pub metadata: HashMap<String, String>, // Additional metadata
}

// Multi-source taxonomy tracking for conflict resolution
pub struct TaxonomySources {
    pub api_provided: Option<u32>,   // From external API
    pub user_specified: Option<u32>, // From user input
    pub mapping_lookup: Option<u32>, // From accession2taxid
    pub header_parsed: Option<u32>,  // Parsed from FASTA header
    pub chunk_context: Option<u32>,  // From SEQUOIA chunk context
}

// SequenceType is imported from talaria_core::types
use talaria_core::types::SequenceType;
// Variants: Protein, DNA, RNA, Nucleotide, Unknown
```

**Key Features:**
- Automatic sequence type detection (proteins detected by EFILPQXZ residues)
- Built-in sanitization removes ambiguous residues: B, J, O, U, Z, X (and lowercase)
- Multi-source taxonomy resolution with conflict detection
- Comprehensive statistics: N50/N90, Shannon entropy, Simpson diversity, GC content
- Case normalization for consistent processing

### 2. FASTA I/O (`formats/fasta.rs`)

High-performance FASTA parsing with multiple strategies:

```rust
// Standard parsing (entire file into memory)
let sequences = parse_fasta("input.fasta")?;

// Parallel parsing for large files
let sequences = parse_fasta_parallel("huge.fasta", chunk_size)?;  // chunk_size in bytes

// Streaming parse from bytes
let sequences = parse_fasta_from_bytes(&data)?;

// Writing with compression detection
write_fasta("output.fasta.gz", &sequences)?;
```

**Performance Optimizations:**
- Memory-mapped I/O for large files
- Automatic gzip compression detection
- Parallel chunk processing
- Zero-copy parsing where possible

### 3. Delta Encoding (`compression/delta.rs`)

Efficient compression through delta encoding:

```rust
pub struct DeltaRecord {
    pub delta_id: String,            // ID of delta-encoded sequence
    pub reference_id: String,        // Reference sequence ID
    pub deltas: Vec<DeltaRange>,     // List of differences
    pub header_change: HeaderChange, // Metadata changes
}

pub struct DeltaRange {
    pub reference_start: usize,  // Start position in reference
    pub reference_end: usize,    // End position in reference
    pub delta_sequence: Vec<u8>, // Replacement sequence
}
```

**Compression Strategy:**
- Stores only differences from reference sequences
- Handles insertions, deletions, and substitutions
- Preserves header/metadata changes
- Typical compression ratios: 10-100x for similar sequences

### 4. Sequence Alignment (`alignment/`)

Needleman-Wunsch global alignment with case normalization:

```rust
// Simple API that auto-selects scoring matrix
let alignment = Alignment::global(&ref_seq, &query_seq);

// Or use specific aligner
let aligner = NeedlemanWunsch::new(NucleotideMatrix::new());
let alignment = aligner.align(&ref_seq, &query_seq);

// DetailedAlignment contains:
pub struct DetailedAlignment {
    pub score: i32,                // Total alignment score
    pub ref_aligned: Vec<u8>,      // Reference with gaps
    pub query_aligned: Vec<u8>,    // Query with gaps
    pub alignment_string: Vec<u8>, // '|' for match, 'X' for mismatch, ' ' for gap
    pub deltas: Vec<Delta>,        // All differences including gaps
    pub identity: f64,             // Sequence identity (0.0 to 1.0)
}
```

**Features:**
- Automatic case normalization (sequences converted to uppercase)
- BLOSUM62 for proteins, custom matrix for nucleotides
- Delta extraction includes insertions, deletions, and substitutions
- Configurable gap penalties (affine gap model)

### 5. Taxonomy Management (`taxonomy/`)

NCBI taxonomy integration and resolution:

```rust
pub struct TaxonomyDB {
    nodes: HashMap<u32, TaxonomyNode>,   // TaxID -> Node mapping
    name_to_taxid: HashMap<String, u32>, // Scientific name lookup
    merged: HashMap<u32, u32>,           // Merged TaxID tracking
}

// Multi-source taxonomy resolution
pub trait TaxonomyResolver {
    fn resolve_taxonomy(&self) -> TaxonomyResolution;
    fn get_taxonomy_sources(&self) -> Vec<TaxonomyDataSource>;
}

// TaxonomyDataSource is imported from talaria_core::types
use talaria_core::types::TaxonomyDataSource;
// Variants: Api, User, Accession2Taxid, Header, Inherited, Unknown
```

**Features:**
- NCBI taxonomy dump file parsing
- Multiple taxonomy source reconciliation
- Discrepancy detection and reporting
- Hierarchical rank traversal (species → kingdom)
- Coverage statistics and tree visualization

### 6. External Providers (`providers/`)

Integration with biological databases:

```rust
pub trait SequenceProvider {
    async fn fetch_by_taxid(&self, taxid: u32) -> Result<Vec<Sequence>>;
    async fn fetch_by_accession(&self, acc: &str) -> Result<Sequence>;
}

// UniProt implementation
let client = UniProtClient::new();
let sequences = client.fetch_proteome(taxid).await?;
```

**Supported Providers:**
- UniProt (Swiss-Prot, TrEMBL)
- Custom database providers via trait implementation

## Integration with Talaria System

### 1. Used by `talaria-sequoia`

SEQUOIA (content-addressed storage) uses talaria-bio for:
- **Sequence parsing**: Reading FASTA files for chunking
- **Delta generation**: Creating delta records for compression
- **Taxonomy formatting**: Enriching headers with TaxID
- **Retroactive analysis**: Time-travel queries on sequences

```rust
// Example from talaria-sequoia/src/delta_generator.rs
use talaria_bio::compression::{DeltaEncoder, DeltaRecord};
use talaria_bio::sequence::Sequence;

let encoder = DeltaEncoder::new();
let delta = encoder.encode(&reference_seq, &target_seq)?;
```

### 2. Used by `talaria-cli`

The CLI uses talaria-bio for:
- **Database operations**: Adding/exporting sequences
- **Statistics computation**: Analyzing sequence datasets
- **Taxonomy coverage**: Reporting taxonomic distribution
- **Reference selection**: Choosing optimal references for delta encoding

```rust
// Example from talaria-cli/src/cli/commands/stats.rs
use talaria_bio::sequence::stats::SequenceStats;
let stats = SequenceStats::compute(&sequences);
stats.print_summary();
```

### 3. Used by `talaria-tools`

External tool integration uses talaria-bio for:
- **FASTA I/O**: Reading/writing for LAMBDA aligner
- **Sequence manipulation**: Preparing data for external tools

```rust
// Example from talaria-tools/src/lambda.rs
use talaria_bio::formats::fasta::{FastaReadable, FastaFile};
let sequences = FastaFile::read_sequences(&input_path)?;
```

### 4. Used by `talaria-storage`

Storage layer uses talaria-bio for:
- **Delta metadata**: Storing/retrieving delta records
- **FASTA reconstruction**: Rebuilding sequences from deltas

```rust
// Example from talaria-storage/src/metadata.rs
use talaria_bio::compression::delta::DeltaRecord;
pub fn write_metadata(path: &Path, deltas: &[DeltaRecord])?;
```

## API Documentation

### Core Functions

#### FASTA Operations
```rust
// Parse FASTA file (auto-detects compression)
pub fn parse_fasta<P: AsRef<Path>>(path: P) -> Result<Vec<Sequence>>

// Parse with parallel processing (memory-mapped)
pub fn parse_fasta_parallel<P: AsRef<Path>>(
    path: P,
    chunk_size: usize // Size in bytes for each chunk
) -> Result<Vec<Sequence>>

// Write sequences to FASTA
pub fn write_fasta<P: AsRef<Path>>(
    path: P,
    sequences: &[Sequence]
) -> Result<()>

// Parse from byte buffer
pub fn parse_fasta_from_bytes(data: &[u8]) -> Result<Vec<Sequence>>
```

#### Sequence Operations
```rust
// Sanitize sequences (remove ambiguous residues)
pub fn sanitize_sequences(
    sequences: Vec<Sequence>
) -> (Vec<Sequence>, usize)

// Compute sequence statistics
impl Sequence {
    pub fn compute_stats(&self) -> SequenceStats
    pub fn detect_type(&self) -> SequenceType
    pub fn gc_content(&self) -> f32 // For DNA/RNA
}
```

#### Delta Encoding
```rust
// Encode sequence as delta from reference
impl DeltaEncoder {
    pub fn encode(
        &self,
        reference: &Sequence,
        target: &Sequence
    ) -> Result<DeltaRecord>
}

// Reconstruct sequence from delta
impl DeltaReconstructor {
    pub fn reconstruct(
        &self,
        reference: &Sequence,
        delta: &DeltaRecord
    ) -> Result<Sequence>
}
```

#### Taxonomy Operations
```rust
// Load NCBI taxonomy database
impl TaxonomyDB {
    pub fn from_ncbi_dump(nodes_path: &Path, names_path: &Path) -> Result<Self>
    pub fn get_lineage(&self, taxid: u32) -> Vec<(TaxonomicRank, String)>
    pub fn get_rank(&self, taxid: u32, rank: TaxonomicRank) -> Option<String>
}

// Resolve taxonomy from sequence
impl Sequence {
    pub fn resolve_taxonomy(&self) -> TaxonomyResolution
}
```

## Usage Examples

### 1. Basic FASTA Processing
```rust
use talaria_bio::{parse_fasta, write_fasta};
use talaria_bio::sequence::sanitize_sequences;

// Read and clean sequences
let sequences = parse_fasta("input.fasta")?;
let (clean_seqs, removed) = sanitize_sequences(sequences);
println!("Removed {} sequences with ambiguous residues", removed);

// Write cleaned sequences
write_fasta("clean.fasta", &clean_seqs)?;
```

### 2. Delta Encoding for Compression
```rust
use talaria_bio::compression::{DeltaEncoder, DeltaReconstructor};

let encoder = DeltaEncoder::new();
let references = select_references(&sequences); // Your selection logic
let mut deltas = Vec::new();

for seq in &sequences {
    if let Some(best_ref) = find_best_reference(seq, &references) {
        let delta = encoder.encode(best_ref, seq)?;
        deltas.push(delta);
    }
}

// Later: reconstruct
let reconstructor = DeltaReconstructor::new();
for delta in &deltas {
    let original = reconstructor.reconstruct(&reference, &delta)?;
}
```

### 3. Taxonomy Analysis
```rust
use talaria_bio::taxonomy::{TaxonomyDB, TaxonomyCoverage};

// Load NCBI taxonomy
let taxonomy = TaxonomyDB::from_ncbi_dump("nodes.dmp", "names.dmp")?;

// Analyze coverage
let coverage = TaxonomyCoverage::from_sequences(&sequences, &taxonomy)?;
coverage.print_summary();
coverage.print_tree(max_depth);
```

### 4. Parallel FASTA Processing
```rust
use talaria_bio::formats::fasta::parse_fasta_parallel;
use rayon::prelude::*;

// Parse large file in parallel with 1MB chunks
let sequences = parse_fasta_parallel("huge_database.fasta", 1024 * 1024)?;

// Process in parallel
let results: Vec<_> = sequences
    .par_iter()
    .map(|seq| seq.compute_stats())
    .collect();
```

### 5. UniProt Integration
```rust
use talaria_bio::providers::uniprot::UniProtClient;

let client = UniProtClient::new();

// Fetch all proteins for a species
let human_proteins = client.fetch_by_taxid(9606).await?;

// Fetch specific proteome
let proteome = client.fetch_proteome("UP000005640").await?;
```

## Performance Considerations

### Memory Usage

1. **Sequence Storage**: ~1 byte per residue + metadata overhead
2. **Large Files**: Use `parse_fasta_parallel()` or streaming for files >1GB
3. **Delta Records**: Typically 1-10% of original sequence size
4. **Taxonomy DB**: ~50MB for full NCBI taxonomy in memory

### Optimization Strategies

1. **Parallel Processing**
   ```rust
   // Set thread pool size
   rayon::ThreadPoolBuilder::new()
       .num_threads(8)
       .build_global()
       .unwrap();
   ```

2. **Memory-Mapped I/O**
   ```rust
   // Automatically used for large files in parse_fasta_parallel
   ```

3. **Compression**
   ```rust
   // Write compressed output
   write_fasta("output.fasta.gz", &sequences)?;
   ```

4. **Batch Processing**
   ```rust
   // Process sequences in batches to control memory
   for chunk in sequences.chunks(1000) {
       process_batch(chunk)?;
   }
   ```

### Benchmarks

| Operation              | Performance | Notes                      |
| ---------------------- | ----------- | -------------------------- |
| `parse_fasta`          | 2.3s/GB     | Single-threaded            |
| `parse_fasta_parallel` | 0.6s/GB     | 8 threads, memory-mapped   |
| `write_fasta`          | 1.8s/GB     | Uncompressed               |
| `write_fasta` (gz)     | 8.5s/GB     | Gzip compression level 6   |
| Memory-mapped parse    | 0.5s/GB     | Direct mmap access         |
| NW alignment (DNA)     | 1ms/100bp   | Needleman-Wunsch           |
| NW alignment (protein) | 2ms/100aa   | BLOSUM62 scoring           |
| Delta encoding         | 0.1ms/seq   | Per sequence pair          |
| Sequence stats         | 50μs/seq    | All statistics computed    |
| GC content calc        | 10μs/seq    | Optimized counting         |

## Testing

### Comprehensive Test Suite
- **53 Unit Tests**: Core functionality in `src/` modules
- **29 Integration Tests**: Cross-module functionality
  - 19 alignment tests (`tests/alignment_integration.rs`)
  - 10 FASTA I/O tests (`tests/fasta_integration.rs`)
- **Performance Benchmarks**: Using Criterion.rs
  - FASTA parsing benchmarks (`benches/fasta_bench.rs`)
  - Alignment benchmarks (`benches/alignment_bench.rs`)

### Running Tests
```bash
# Run all tests
cargo test -p talaria-bio

# Run unit tests only
cargo test -p talaria-bio --lib

# Run integration tests only
cargo test -p talaria-bio --tests

# Run specific test file
cargo test -p talaria-bio --test alignment_integration

# Run benchmarks
cargo bench -p talaria-bio

# Run with output for debugging
cargo test -p talaria-bio -- --nocapture
```

### Test Coverage
```bash
# Generate coverage report
cargo tarpaulin -p talaria-bio --out html
open tarpaulin-report.html
```

### Test Categories

**Unit Tests** (`src/*/tests`):
- Sequence type detection and manipulation
- Sanitization of ambiguous residues
- Statistics computation (N50, entropy, diversity)
- Taxonomy source tracking and resolution
- FASTA parsing edge cases

**Integration Tests** (`tests/`):
- FASTA round-trip I/O with various formats
- Alignment with different sequence types
- Compressed file handling (.gz)
- Parallel processing validation
- Large sequence performance

**Benchmarks** (`benches/`):
- FASTA parsing: serial vs parallel vs memory-mapped
- Alignment: varying sequence lengths and similarity
- Real-world sequence sizes (Illumina, Sanger, genes)
- Worst-case scenarios (no matches, all gaps)

## Error Handling

All operations return `Result<T, anyhow::Error>` for consistent error handling:

```rust
use anyhow::{Context, Result};

fn process_sequences(path: &Path) -> Result<()> {
    let sequences = parse_fasta(path)
        .context("Failed to parse FASTA file")?;

    let stats = sequences.compute_stats()
        .context("Failed to compute statistics")?;

    Ok(())
}
```

Common error scenarios:
- **Invalid FASTA format**: Missing '>' headers, malformed sequences
- **Ambiguous residues**: Sequences with X, B, Z (protein) or N (nucleotide)
- **Memory limits**: Files too large for available RAM
- **Taxonomy mismatches**: Conflicting TaxIDs from different sources
- **Network errors**: UniProt API timeouts or rate limits

## Dependencies

### Core Dependencies
- `bio` - Bioinformatics algorithms and data structures
- `noodles` - High-performance FASTA/FASTQ parsing
- `needletail` - Alternative FASTA parser with SIMD optimizations
- `rayon` - Data parallelism for sequence processing
- `flate2` - Gzip compression support

### Utility Dependencies
- `serde` - Serialization for delta records and metadata
- `indicatif` - Progress bars for long operations
- `regex` - Pattern matching in sequence headers
- `chrono` - Timestamp handling for temporal features
- `sha2` - Hashing for content addressing

### External Integration
- `reqwest` - HTTP client for UniProt API
- `nom` - Parser combinators for complex formats

## Future Improvements

### Planned Features

1. **Additional Formats**
   - FASTQ support with quality scores
   - GenBank/EMBL format parsing
   - GFF/GTF annotation integration

2. **Enhanced Algorithms**
   - Local alignment (Smith-Waterman)
   - Multiple sequence alignment
   - K-mer based similarity metrics
   - Suffix array construction

3. **Database Integration**
   - NCBI Entrez API client
   - PDB structure fetching
   - Pfam domain annotations
   - RefSeq integration

4. **Performance Optimizations**
   - SIMD acceleration for alignment
   - GPU support via CUDA/OpenCL
   - Lazy sequence loading
   - Bloom filters for membership testing

5. **Advanced Compression**
   - Reference graph compression
   - Homopolymer run-length encoding
   - Context-aware encoding

### API Stability

The public API is considered stable for:
- Core types (`Sequence`, `SequenceType`)
- FASTA I/O functions
- Delta encoding/decoding
- Taxonomy resolution traits

Experimental/unstable APIs:
- Provider traits (may change for async support)
- Parallel processing internals
- Compression strategies

## Contributing

### Code Style
- Follow Rust standard naming conventions
- Document public APIs with examples
- Add unit tests for new functionality
- Run `cargo fmt` and `cargo clippy` before commits

### Testing Requirements
- All new features must include tests
- Maintain >80% code coverage
- Include integration tests for cross-module functionality
- Add benchmarks for performance-critical code

### Documentation
- Update this README for significant changes
- Add inline documentation for complex algorithms
- Include usage examples in doc comments
- Document performance characteristics

## License

This module is part of the Talaria project and follows the same license terms as the parent project.

## Support

For issues, questions, or contributions related to talaria-bio:
- Open an issue on the Talaria GitHub repository
- Check existing documentation in `/docs`
- Review test cases for usage examples
