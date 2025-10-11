# Talaria Codebase Learning Guide

> **Deep Dive Study Plan**: A comprehensive, bottom-up approach to mastering the Talaria bioinformatics sequence database system

## Codebase Overview

- **Total Size**: ~101,642 lines of Rust code
- **Files**: 310 Rust files across 8 crates
- **Architecture**: Bottom-up dependencies (Foundation → Domain → Application)
- **Complexity Distribution**:
  - **talaria-sequoia**: 42k lines (largest, most complex)
  - **talaria-cli**: 15k lines
  - **talaria-storage**: 12k lines
  - **talaria-utils**: 8k lines
  - **talaria-tools**: 8k lines
  - **talaria-bio**: 6k lines
  - **talaria-core**: 2.7k lines (smallest, start here!)
  - **talaria-test**: 2k lines

## Why This Learning Path?

**Bottom-up approach** ensures you:

1. Understand fundamentals before complexity
2. Build mental models incrementally
3. Avoid confusion from forward references
4. See how components compose into larger systems

---

## Phase 1: Foundation Layer (Days 1-2)

### Week 1, Day 1-2: Core Types & Configuration

#### 1.1 `talaria-core` (~2,700 lines) * **START HERE**

**Purpose**: Foundation types, errors, configuration, and paths

**Learning Order**:

1. **`src/types/`** - Core data types
   ```rust
   // Key types you'll see everywhere:
   - SHA256Hash     // Content addressing
   - TaxonId        // Taxonomy identifiers
   - DatabaseSource // Where sequences come from
   - ChunkMetadata  // Chunk organization
   ```

   **Goal**: Understand how sequences are identified (SHA256) and organized

2. **`src/system/paths.rs`** - Path configuration
   ```bash
   # Environment variables that control everything:
   TALARIA_HOME           # Base directory (~/.talaria)
   TALARIA_DATA_DIR       # Data storage
   TALARIA_DATABASES_DIR  # Database location
   TALARIA_CACHE_DIR      # Cache location
   ```

   **Goal**: Understand where data lives and how paths are resolved

3. **`src/error/`** - Error handling
   ```rust
   // Error types:
   - TalariaError          // Main error type
   - VerificationError     // Merkle/hash verification
   - StorageError          // Storage backend errors

   ```
   **Goal**: Learn error handling patterns used throughout codebase

4. **`src/config/`** - Configuration system
   **Goal**: Understand how configuration is loaded and used

**Deliverable**: Create a diagram showing:

- Core type hierarchy
- Path resolution flow
- Error propagation pattern

**Key Questions to Answer**:

- Why use SHA256 for sequence identification?
- How does path configuration work across different environments?
- What's the error handling philosophy?

---

#### 1.2 `talaria-utils` (~8,000 lines)

**Purpose**: Cross-cutting utilities, display formatting, progress tracking

**Learning Order**:

1. **`src/display/`** - Output formatting
   - How CLI output is formatted
   - Color codes and Unicode symbols
   - Table formatting

2. **`src/progress/`** - Progress bar system
   - Multi-progress bar management
   - Progress tracking strategies
   - Visual feedback patterns

3. **`src/parallel.rs`** - Parallelization utilities
   - Rayon integration
   - Batch processing patterns
   - Thread pool management

4. **`src/database/`** - Database utilities
   - Version detection
   - Reference database handling

**Deliverable**: Understand how to:

- Display progress for long-running operations
- Format output for terminal display
- Parallelize batch operations

**Key Questions**:

- How does the progress system work?
- What parallelization patterns are used?
- How are database versions detected?

---

## Phase 2: Domain Layer (Days 3-5)

### Week 1, Day 3-4: Bioinformatics Primitives

#### 2.1 `talaria-bio` (~6,000 lines)

**Purpose**: Bioinformatics core - FASTA, alignment, taxonomy, compression

**Learning Order**:

1. **`src/sequence/types.rs`** - Sequence representation
   ```rust
   pub struct Sequence {
       pub id: String,           // Accession
       pub sequence: Vec<u8>,    // Actual sequence data
       pub description: String,  // Header info
   }

   pub enum SequenceType {
       DNA,
       RNA,
       Protein,
   }
   ```

   **Goal**: Understand how biological sequences are represented in memory

2. **`src/formats/fasta.rs`** - FASTA parsing * **CRITICAL**
   ```rust
   // FASTA format:
   // >sequence_id description
   // ACGTACGTACGT...

   pub struct FastaParser {
       // Streaming parser - processes files incrementally
   }
   ```

   **Goal**: Trace how a FASTA file becomes `Sequence` objects

3. **`src/taxonomy/`** - Taxonomy operations
   ```rust
   pub struct TaxonomyTree {
       // Hierarchical taxonomy structure
       // Species → Genus → Family → ...
   }
   ```

   **Goal**: Understand taxonomy filtering and tree traversal

4. **`src/alignment/nw_aligner.rs`** - Needleman-Wunsch alignment
   ```rust
   pub fn align(seq1: &[u8], seq2: &[u8]) -> Alignment {
       // Dynamic programming alignment algorithm
   }
   ```

   **Goal**: Understand how sequence similarity is computed

5. **`src/compression/delta.rs`** - Delta encoding
   ```rust
   pub struct DeltaCompressor {
       // Encodes sequences as deltas from references
   }
   ```

   **Goal**: Learn how similar sequences are compressed

**Deliverable**:

- Trace a FASTA file through: parsing → Sequence → alignment → delta encoding
- Document the bioinformatics algorithms used

**Key Questions**:

- How does streaming FASTA parsing work?
- How is sequence similarity calculated?
- What compression strategies optimize for biological sequences?

---

### Week 1, Day 5: Storage Backend

#### 2.2 `talaria-storage` (~12,000 lines)

**Purpose**: Low-level storage abstraction over RocksDB

**Learning Order**:

1. **`src/types.rs`** - Storage traits
   ```rust
   pub trait SequenceStorageBackend {
       fn sequence_exists(&self, hash: &SHA256Hash) -> Result<bool>;
       fn store_canonical(&self, seq: &CanonicalSequence) -> Result<()>;
       fn load_canonical(&self, hash: &SHA256Hash) -> Result<CanonicalSequence>;
       // ... more methods
   }
   ```

   **Goal**: Understand the storage abstraction

2. **`src/backend/rocksdb_backend.rs`** - RocksDB implementation * **CRITICAL**
   ```rust
   pub struct RocksDBBackend {
       db: Arc<DBWithThreadMode<MultiThreaded>>,
       config: RocksDBConfig,
       write_opts: WriteOptions,
   }

   // Column families:
   // - sequences: Canonical sequence data
   // - representations: Headers/metadata from different DBs
   // - manifests: Chunk manifests
   // - indices: Secondary indices (accession → hash)
   // - merkle: Merkle DAG nodes
   // - temporal: Bi-temporal version tracking
   ```

   **Goal**: Master the RocksDB implementation

3. **`src/compression.rs`** - Compression layer
   ```rust
   pub struct ChunkCompressor {
       // Zstandard compression for chunks
   }
   ```

4. **`src/core/traits.rs`** - Storage abstractions
   **Goal**: Understand trait-based design

**Deliverable**: Create diagrams showing:

1. RocksDB column family structure
2. Data flow: write → column family → disk
3. Read path: query → bloom filter → column family → value

**Key Questions**:

- Why RocksDB over filesystem storage?
- How do column families optimize different access patterns?
- What role do bloom filters play?

---

## Phase 3: The Heart - SEQUOIA (Days 6-14)

### Week 2: SEQUOIA Core Storage

**The largest crate** (~42,000 lines) - Take your time!

#### 3.1 Storage System (Days 6-7)

**Start with the index system - it's the gateway to everything else**

1. **`storage/indices.rs`** * **START HERE FOR SEQUOIA**
   ```rust
   pub struct SequenceIndices {
       backend: Arc<RocksDBBackend>,
       sequence_bloom: Arc<RwLock<BloomFilter>>,
       streaming_mode: Arc<AtomicBool>,
   }

   pub struct BloomFilter {
       bits: Vec<bool>,
       size: usize,
       hash_count: usize,
   }
   ```

   **Goal**: Understand how sequences are indexed and how bloom filters accelerate lookups

2. **`storage/sequence.rs`** - Sequence storage
   ```rust
   pub struct SequenceStorage {
       backend: Arc<RocksDBBackend>,
       streaming_mode: Arc<AtomicBool>,
   }

   pub struct CanonicalSequence {
       sequence_hash: SHA256Hash,    // Content-based ID
       sequence: Vec<u8>,              // Actual sequence
       length: usize,
       sequence_type: SequenceType,
       // ... metadata
   }

   pub struct SequenceRepresentations {
       canonical_hash: SHA256Hash,
       representations: Vec<SequenceRepresentation>,  // Different headers
   }
   ```

   **Goal**: Understand canonical storage and multi-representation support

3. **`storage/core.rs`** - Main SEQUOIA storage * **THE ORCHESTRATOR**
   ```rust
   pub struct SequoiaStorage {
       base_path: PathBuf,
       sequence_storage: Arc<SequenceStorage>,
       indices: Arc<SequenceIndices>,
       chunk_storage: Arc<RocksDBBackend>,
       state_manager: Arc<Mutex<ProcessingStateManager>>,
       compressor: Arc<Mutex<ChunkCompressor>>,
   }
   ```

   **Key method to trace**:
   ```rust
   pub fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash> {
       let hash = SHA256Hash::compute(data);

       // Tier 1: In-memory bloom filter (O(1), 99.9% accurate)
       if self.indices.sequence_exists(&hash) {
           // Tier 2 & 3: RocksDB native bloom + actual lookup
           if self.chunk_storage.chunk_exists(&hash)? {
               return Ok(hash);  // Already stored
           }
       }

       // Store new chunk...
   }
   ```

   **Goal**: Understand how all components work together

**Deliverable**:

- Data flow diagram: Input → Bloom filter → RocksDB → Storage
- Trace one sequence from store to retrieve

---

#### 3.2 Chunking System (Days 8-9)

1. **`chunker/mod.rs`** - Chunking traits
   ```rust
   pub trait ChunkingStrategy {
       fn chunk_sequences(&self, sequences: Vec<Sequence>) -> Result<Vec<Chunk>>;
   }
   ```

2. **`chunker/canonical_taxonomic.rs`** - Taxonomy-aware chunking
   ```rust
   pub struct CanonicalTaxonomicChunker {
       // Groups sequences by taxonomy for efficient updates
       target_size: usize,
       // ... config
   }
   ```

   **Goal**: Understand how taxonomy guides chunk organization

3. **`chunker/hierarchical_taxonomic.rs`** - Hierarchical strategy
   **Goal**: Learn advanced chunking strategies

**Key Concept**: Chunks are **manifests** that reference sequences, not containers

**Deliverable**:

- Diagram showing chunk manifest structure
- Example of how taxonomy affects chunking

---

#### 3.3 Delta Encoding System (Days 10-11)

1. **`delta/traits.rs`** - Delta abstractions
   ```rust
   pub trait DeltaEncoder {
       fn encode(&self, reference: &[u8], target: &[u8]) -> Result<Delta>;
       fn decode(&self, reference: &[u8], delta: &Delta) -> Result<Vec<u8>>;
   }
   ```

2. **`delta/canonical.rs`** - Canonical delta format
   ```rust
   pub struct CanonicalDelta {
       reference_hash: SHA256Hash,
       target_hash: SHA256Hash,
       operations: Vec<DeltaOp>,
   }

   pub enum DeltaOp {
       Copy { offset: usize, length: usize },
       Insert { data: Vec<u8> },
       Skip { length: usize },
   }
   ```

   **Goal**: Understand delta encoding operations

3. **`delta/generator.rs`** - Delta generation
4. **`delta/reconstructor.rs`** - Delta reconstruction

**Key Insight**: Deltas are computed once between canonical sequences and reused across all databases!

**Deliverable**:

- Trace delta encoding: reference → target → operations
- Trace delta decoding: reference + delta → target
- Calculate compression ratios

---

#### 3.4 Temporal System (Day 12)

1. **`temporal/core.rs`** - Temporal core
   ```rust
   pub struct TemporalVersion {
       sequence_time: DateTime<Utc>,    // When sequence added
       taxonomy_time: DateTime<Utc>,    // When taxonomy valid
       manifest_hash: SHA256Hash,
   }
   ```

2. **`temporal/bi_temporal.rs`** - Bi-temporal tracking
   ```rust
   // Two independent timelines:
   // 1. Sequence evolution (additions/modifications)
   // 2. Taxonomy evolution (reclassifications)
   ```

   **Goal**: Understand time-travel queries

3. **`temporal/retroactive.rs`** - Retroactive updates
   **Goal**: Learn how historical data is updated

**Key Concept**: Bi-temporal = (sequence_time, taxonomy_time) coordinates

**Deliverable**:

- Timeline diagram showing both temporal dimensions
- Example of retroactive taxonomy update

---

#### 3.5 Database Manager (Day 13) ! **LARGE FILE**

1. **`database/manager.rs`** - Main database manager
   - **Warning**: This file is massive!
   - Handles all database operations
   - Coordinates downloads, processing, storage

   **Strategy**: Don't read linearly - trace specific operations:
   - How does download work?
   - How does update differ from fresh download?
   - How are manifests compared?

2. **`database/manager_resume.rs`** - Resume capability
   **Goal**: Understand checkpoint and resume logic

3. **`database/diff.rs`** - Database comparison
   **Goal**: Learn manifest diffing

**Deliverable**:

- Sequence diagram for database download
- Sequence diagram for database update

---

#### 3.6 Download System (Day 14)

1. **`download/mod.rs`** - Download abstractions
2. **`download/manager.rs`** - Download orchestration
3. **`download/resumable_downloader.rs`** - Resumable downloads
4. **`download/workspace.rs`** - Workspace management

**Goal**: Understand streaming download → process → store pipeline

**Deliverable**:

- Download workflow diagram
- Error recovery flowchart

---

## Phase 4: Tool Integration (Day 15)

### Week 3, Day 1: External Tools

#### 4.1 `talaria-tools` (~8,000 lines)

**Purpose**: Integration with external bioinformatics tools (LAMBDA, BLAST, etc.)

**Learning Order**:

1. **`src/traits/aligner.rs`** - Aligner trait
   ```rust
   pub trait Aligner {
       fn align(&self, query: &Path, database: &Path) -> Result<AlignmentResults>;
       fn create_index(&self, sequences: &Path) -> Result<()>;
   }
   ```

2. **`src/aligners/lambda/`** - LAMBDA integration
   - How LAMBDA is invoked
   - How results are parsed
   - Error handling for external processes

3. **`src/manager/installer.rs`** - Tool management
   - How tools are downloaded/installed
   - Version management

**Deliverable**:

- Diagram of tool invocation flow
- Understanding of LAMBDA integration

---

## Phase 5: Application Layer (Days 16-17)

### Week 3, Day 2-3: Command-Line Interface

#### 5.1 `talaria-cli` (~15,000 lines)

**Purpose**: User-facing command-line interface

**Learning Order**:

1. **`src/main.rs`** - Entry point
   ```rust
   // CLI structure using clap
   #[derive(Parser)]
   struct Cli {
       #[command(subcommand)]
       command: Commands,
   }
   ```

2. **`src/cli/commands/database/`** - Database commands
   - `download.rs` - Download databases
   - `info.rs` - Show database information
   - `list.rs` - List available databases

3. **`src/cli/commands/reduce.rs`** - Main reduction command
   **Goal**: Understand end-to-end reduction workflow

4. **`src/cli/interactive/`** - Interactive mode
   **Goal**: Learn TUI implementation

**Deliverable**:

- Map CLI commands to underlying crate operations
- Understand command execution flow

---

## Phase 6: Testing (Day 18)

### Week 3, Day 4: Test Infrastructure

#### 6.1 `talaria-test` (~2,000 lines)

**Purpose**: Test utilities, fixtures, and mocks

**Learning Order**:

1. **`src/fixtures.rs`** - Test data generation
2. **`src/mock/`** - Mock implementations
3. **`src/assertions.rs`** - Custom test assertions

**Goal**: Learn how to write tests for the system

**Deliverable**:

- Understanding of testing patterns
- Ability to add new tests

---

## Study Methods & Best Practices

### For Each Module:

#### 1. **Read Order**
```
mod.rs/lib.rs → Tests → Main files → Supporting files
```

#### 2. **Active Reading**
- Take notes in comments
- Draw diagrams
- Ask questions

#### 3. **Trace Execution**
Pick one operation and trace it completely:
```
User Command → CLI → Manager → Storage → RocksDB → Disk
```

#### 4. **Run Examples**
```bash
# Watch it in action
TALARIA_LOG=debug talaria database download uniprot/swissprot

# Use as a library
cargo run --example sequence_storage
```

#### 5. **Use Tools**

```bash
# Browse documentation
cargo doc --open --package talaria-sequoia

# Understand dependencies
cargo tree --package talaria-sequoia

# Find definitions
rg "pub struct SequenceIndices" --type rust

# Check complexity
tokei talaria-sequoia/src
```

---

## Documentation Artifacts to Create

### 1. **Architecture Diagrams**

#### Overall System
```
+-------------+
| talaria-cli |
+------+------+
       |
       v
+-----------------+
| talaria-sequoia | <-- The Heart
+-------+---------+
        |
        v
+-----------------+
| talaria-storage | <-- RocksDB Backend
+-------+---------+
        |
        v
+----------------+
|  talaria-bio   | <-- Bioinformatics
+-------+--------+
        |
        v
+----------------+
|  talaria-core  | <-- Foundation
+----------------+
```

#### Storage Architecture
```
SequoiaStorage
|-- SequenceStorage (Canonical sequences in RocksDB)
|-- SequenceIndices (Bloom filter + RocksDB indices)
|-- ChunkStorage (Manifests in RocksDB)
 +-- StateManager (Processing state)
```

#### Bloom Filter Optimization
```
store_chunk(data)
  |
  |--> Tier 1: In-memory bloom filter (1us)
  |      +--> If "definitely not exists" -> continue
  |
  |--> Tier 2: RocksDB native bloom (block-level)
  |      +--> Reduces disk I/O
  |
   +--> Tier 3: Actual RocksDB lookup
         +--> Definitive answer
```

### 2. **Data Flow Diagrams**

#### FASTA to Storage
```
FASTA file
  → FastaParser (streaming)
  → Sequence objects
  → SHA256Hash computation
  → Bloom filter check
  → RocksDB existence check
  → Store in RocksDB
  → Update indices
```

#### Query and Retrieval
```
Query (accession)
  → Index lookup (RocksDB INDICES column family)
  → Get hash
  → Load canonical (RocksDB SEQUENCES column family)
  → Load representations (RocksDB REPRESENTATIONS column family)
  → Return complete sequence data
```

### 3. **API Surface Maps**

Document the public API for each crate:
```rust
// talaria-sequoia public API
pub struct SequoiaStorage { ... }
impl SequoiaStorage {
    pub fn new(path: &Path) -> Result<Self>;
    pub fn store_chunk(&self, data: &[u8], compress: bool) -> Result<SHA256Hash>;
    pub fn chunk_exists_fast(&self, hash: &SHA256Hash) -> Result<bool>;
    // ... more methods
}
```

### 4. **Decision Logs**

**Why RocksDB?**

- LSM-tree architecture optimized for writes
- Proven at petabyte scale (Meta, Netflix, LinkedIn)
- Built-in bloom filters and compression
- MultiGet for batch operations
- Column families for logical separation

**Why Bloom Filters?**

- 100x faster deduplication checks (100us -> 1us)
- 99.9% accuracy for "not exists" checks
- Minimal memory overhead (~180MB for 100M sequences)
- Three-tier architecture maximizes performance

**Why Bi-Temporal?**

- Sequences and taxonomy evolve independently
- Enables time-travel queries
- Supports retroactive corrections
- Maintains data provenance

---

## Checkpoints & Self-Assessment

### After Phase 1 (Foundation) [x]
- [ ] Can explain SHA256 content addressing
- [ ] Understand path configuration system
- [ ] Know error handling patterns
- [ ] Can navigate core types

### After Phase 2 (Domain) [x]
- [ ] Can parse FASTA files
- [ ] Understand sequence alignment
- [ ] Know taxonomy tree operations
- [ ] Understand RocksDB column families

### After Phase 3 (SEQUOIA) [x]
- [ ] Can trace chunk storage end-to-end
- [ ] Understand bloom filter optimization
- [ ] Know how delta encoding works
- [ ] Understand bi-temporal system
- [ ] Can explain manifest-based updates

### After Phase 4 (Tools) [x]
- [ ] Understand tool integration
- [ ] Know how LAMBDA is invoked

### After Phase 5 (CLI) [x]
- [ ] Can trace CLI command execution
- [ ] Understand command structure

### After Phase 6 (Testing) [x]
- [ ] Can write new tests
- [ ] Understand testing patterns

---

## Timeline Estimates

### Fast Track (Full-Time Study)
- **Foundation**: 2 days
- **Domain**: 3 days
- **SEQUOIA**: 9 days
- **Tools**: 1 day
- **CLI**: 2 days
- **Testing**: 1 day
- **Total**: **18 days (3-4 weeks)**

### Moderate Pace (Part-Time)
- **Foundation**: 1 week
- **Domain**: 1 week
- **SEQUOIA**: 3 weeks
- **Tools**: 3 days
- **CLI**: 1 week
- **Testing**: 2 days
- **Total**: **6-8 weeks**

### Thorough (Deep Research)
- **Foundation**: 2 weeks
- **Domain**: 3 weeks
- **SEQUOIA**: 6 weeks
- **Tools**: 1 week
- **CLI**: 2 weeks
- **Testing**: 1 week
- **Total**: **15 weeks (~3-4 months)**

---

## Success Criteria

You'll know you've mastered the codebase when you can:

### [DONE] Understand the Flow
1. Trace a FASTA file from input to storage in RocksDB
2. Explain the three-tier bloom filter optimization
3. Describe how bi-temporal versioning tracks changes
4. Show how a sequence is retrieved and reconstructed

### [DONE] Explain the Architecture
1. Draw the crate dependency graph from memory
2. Explain why RocksDB was chosen over filesystem
3. Describe the bloom filter mathematics
4. Explain column family organization

### [DONE] Make Changes
1. Add a new database source
2. Implement a new chunking strategy
3. Add a new CLI command
4. Write tests for new functionality

### [DONE] Debug Issues
1. Use logs to trace execution
2. Understand RocksDB metrics
3. Profile performance bottlenecks
4. Fix bugs by understanding data flow

### [DONE] Optimize Performance
1. Tune RocksDB configuration
2. Adjust bloom filter parameters
3. Optimize batch sizes
4. Profile and improve hot paths

---

## Additional Resources

### Internal Documentation
- `/home/brett/repos/talaria/docs/` - Full mdBook documentation
- `/home/brett/repos/talaria/CLAUDE.md` - Development guidelines

### External Resources
- [RocksDB Documentation](https://github.com/facebook/rocksdb/wiki)
- [Rust Documentation](https://doc.rust-lang.org/)
- [Bioinformatics Algorithms](http://bioinformaticsalgorithms.com/)

### Tools
```bash
# Code navigation
cargo doc --open
rg "pattern" --type rust
fd "filename"

# Analysis
cargo tree
cargo bloat --release
tokei

# Performance
cargo bench
cargo flamegraph
```

---

## Next Steps

1. **Start with `talaria-core`** - Build your foundation
2. **Take notes** - Create your own documentation
3. **Draw diagrams** - Visualize the architecture
4. **Ask questions** - Write down what you don't understand
5. **Run code** - See it in action with real data
6. **Write code** - Best way to learn is by doing

Good luck with your deep dive! The codebase is well-structured, and with this guide, you'll master it systematically.

---

*Guide created: September 2024*
*Based on codebase version: 0.1.0*
*Total lines analyzed: 101,642 Rust LOC*
