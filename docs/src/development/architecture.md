# Architecture

Comprehensive overview of Talaria's system architecture, design patterns, and internal structure.

## System Overview

```mermaid
graph TB
    subgraph "CLI Interface"
        A1[reduce]
        A2[stats]
        A3[download]
        A4[interactive]
    end
    
    subgraph "Core Engine"
        B1[Reduction Engine]
        B2[Alignment Manager]
        B3[Delta Encoder]
    end
    
    subgraph "Aligner Abstraction"
        C1[BLAST]
        C2[LAMBDA]
        C3[Kraken]
        C4[Diamond]
        C5[MMseqs2]
    end
    
    subgraph "I/O Layer"
        D1[FASTA Parser]
        D2[Memory Map]
        D3[Compression]
    end
    
    A1 --> B1
    A2 --> B1
    A2 --> B2
    A3 --> D1
    A4 --> B1
    
    B1 --> C1
    B1 --> C2
    B1 --> C3
    B1 --> C4
    B1 --> C5
    
    B2 --> C1
    B2 --> C2
    B2 --> C3
    B2 --> C4
    B2 --> C5
    
    B3 --> D1
    B3 --> D2
    
    C1 --> D1
    C2 --> D1
    C3 --> D1
    C4 --> D1
    C5 --> D1
```

## Module Structure

### Core Modules

```
src/
├── main.rs              # Entry point and CLI setup
├── lib.rs               # Library exports
│
├── bio/                 # Biological data structures
│   ├── mod.rs           # Module exports
│   ├── sequence.rs      # Sequence representation
│   ├── alignment.rs     # Alignment algorithms
│   ├── scoring.rs       # Scoring matrices
│   ├── delta.rs         # Delta encoding
│   └── stats.rs         # Statistics calculation
│
├── core/                # Core reduction logic
│   ├── mod.rs           # Module exports
│   ├── reducer.rs       # Main reduction engine
│   ├── selector.rs      # Reference selection
│   ├── clustering.rs    # Sequence clustering
│   ├── taxonomy.rs      # Taxonomy-aware reduction
│   └── config.rs        # Configuration management
│
├── aligners/            # Aligner implementations
│   ├── mod.rs           # Aligner trait and registry
│   ├── blast.rs         # BLAST integration
│   ├── lambda.rs        # LAMBDA integration
│   ├── kraken.rs        # Kraken optimization
│   ├── diamond.rs       # Diamond integration
│   └── mmseqs2.rs       # MMseqs2 integration
│
├── io/                  # Input/Output handling
│   ├── mod.rs           # Module exports
│   ├── fasta.rs         # FASTA parser and writer
│   ├── compression.rs   # Compression utilities
│   ├── mmap.rs          # Memory-mapped I/O
│   └── streaming.rs     # Stream processing
│
├── cli/                 # Command-line interface
│   ├── mod.rs           # CLI setup
│   ├── commands/        # Command implementations
│   │   ├── reduce.rs    # Reduce command
│   │   ├── stats.rs     # Statistics command
│   │   ├── download.rs  # Download command
│   │   └── expand.rs    # Expand command
│   ├── interactive/     # Interactive TUI
│   │   ├── mod.rs       # TUI framework
│   │   ├── reduce.rs    # Reduction wizard
│   │   ├── stats.rs     # Statistics viewer
│   │   └── config.rs    # Configuration editor
│   └── visualize.rs     # Visualization utilities
│
├── download/            # Database download
│   ├── mod.rs           # Download manager
│   ├── uniprot.rs       # UniProt downloader
│   ├── ncbi.rs          # NCBI downloader
│   └── pdb.rs           # PDB downloader
│
└── utils/               # Utility functions
    ├── mod.rs           # Module exports
    ├── parallel.rs      # Parallel processing
    ├── progress.rs      # Progress reporting
    └── error.rs         # Error handling
```

## Design Patterns

### 1. Strategy Pattern for Aligners

```rust
pub trait Aligner: Send + Sync {
    fn align(&self, seq1: &[u8], seq2: &[u8]) -> AlignmentResult;
    fn optimization_hints(&self) -> OptimizationHints;
}

pub struct AlignerRegistry {
    aligners: HashMap<String, Box<dyn Aligner>>,
}

impl AlignerRegistry {
    pub fn get_aligner(&self, name: &str) -> Option<&dyn Aligner> {
        self.aligners.get(name).map(|b| b.as_ref())
    }
}
```

### 2. Builder Pattern for Configuration

```rust
pub struct ReductionBuilder {
    config: ReductionConfig,
}

impl ReductionBuilder {
    pub fn new() -> Self {
        Self {
            config: ReductionConfig::default(),
        }
    }
    
    pub fn threshold(mut self, threshold: f64) -> Self {
        self.config.threshold = threshold;
        self
    }
    
    pub fn aligner(mut self, aligner: String) -> Self {
        self.config.aligner = aligner;
        self
    }
    
    pub fn build(self) -> Result<ReductionEngine> {
        ReductionEngine::new(self.config)
    }
}
```

### 3. Iterator Pattern for Streaming

```rust
pub struct FastaIterator<R: BufRead> {
    reader: R,
    buffer: String,
}

impl<R: BufRead> Iterator for FastaIterator<R> {
    type Item = Result<Sequence>;
    
    fn next(&mut self) -> Option<Self::Item> {
        // Parse next sequence
    }
}

pub trait StreamProcessor {
    fn process_stream<I>(&self, iter: I) -> Result<()>
    where
        I: Iterator<Item = Result<Sequence>>;
}
```

### 4. Observer Pattern for Progress

```rust
pub trait ProgressObserver: Send + Sync {
    fn on_progress(&self, current: usize, total: usize);
    fn on_complete(&self);
    fn on_error(&self, error: &Error);
}

pub struct ProgressManager {
    observers: Vec<Box<dyn ProgressObserver>>,
}

impl ProgressManager {
    pub fn notify_progress(&self, current: usize, total: usize) {
        for observer in &self.observers {
            observer.on_progress(current, total);
        }
    }
}
```

## Data Flow

### Reduction Pipeline

```mermaid
flowchart TD
    A[Input FASTA] --> B[Parse & Load]
    B -->|Memory-mapped for large files| C[Pre-filtering]
    C -->|Length, complexity filters| D[Clustering]
    D -->|Group similar sequences| E[Reference Select]
    E -->|Choose representatives| F{Skip Deltas?}
    F -->|No| G[Delta Encoding]
    F -->|Yes --no-deltas| H[Write Output]
    G -->|Encode non-references| H[Write Output]
    H --> I[FASTA + Delta files]
    
    style A stroke:#1976d2,stroke-width:2px,fill:#bbdefb
    style B stroke:#00796b,stroke-width:2px
    style C stroke:#00796b,stroke-width:2px
    style D stroke:#00796b,stroke-width:2px
    style E stroke:#512da8,stroke-width:2px,fill:#d1c4e9
    style F stroke:#f57c00,stroke-width:2px,fill:#ffe0b2
    style G stroke:#0288d1,stroke-width:2px,fill:#b3e5fc
    style H stroke:#388e3c,stroke-width:2px,fill:#c8e6c9
    style I stroke:#388e3c,stroke-width:3px,fill:#a5d6a7
```

### Alignment Processing

```mermaid
flowchart TD
    A[Query Sequences] --> B[Batch Manager]
    B --> C1[Thread 1]
    B --> C2[Thread 2]
    B --> CN[Thread N]
    
    C1 --> D1[Aligner]
    C2 --> D2[Aligner]
    CN --> DN[Aligner]
    
    D1 --> E[Cache]
    D2 --> E
    DN --> E
    
    E --> F[Results]
    
    style A stroke:#1976d2,stroke-width:2px,fill:#bbdefb
    style B stroke:#7b1fa2,stroke-width:2px,fill:#e1bee7
    style E stroke:#388e3c,stroke-width:2px,fill:#c8e6c9
    style F stroke:#388e3c,stroke-width:3px,fill:#a5d6a7
    style C1 stroke:#00796b,stroke-width:2px
    style C2 stroke:#00796b,stroke-width:2px
    style CN stroke:#00796b,stroke-width:2px
    style D1 stroke:#0288d1,stroke-width:2px
    style D2 stroke:#0288d1,stroke-width:2px
    style DN stroke:#0288d1,stroke-width:2px
```

## Memory Management

### Memory Layout

```mermaid
graph TB
    subgraph "Application Memory"
        A["Stack (per thread)<br/>• Function calls<br/>• Local variables"]
        B["Heap<br/>• Sequence buffers<br/>• Alignment matrices<br/>• Cache structures"]
        C["Memory-Mapped Regions<br/>• Large FASTA files<br/>• Read-only mapping<br/>• Page-aligned access"]
        D["Shared Memory<br/>• Inter-process communication<br/>• Alignment cache<br/>• Progress tracking"]
    end
    
    A -.->|Thread-local| B
    B -.->|Dynamic allocation| C
    C -.->|Shared access| D
    
    style A stroke:#1976d2,stroke-width:2px,fill:#bbdefb
    style B stroke:#00796b,stroke-width:2px,fill:#b2dfdb
    style C stroke:#512da8,stroke-width:2px,fill:#d1c4e9
    style D stroke:#7b1fa2,stroke-width:2px,fill:#e1bee7
```

### Object Pooling

```rust
pub struct AlignmentMatrixPool {
    available: Vec<AlignmentMatrix>,
    in_use: HashSet<usize>,
}

impl AlignmentMatrixPool {
    pub fn acquire(&mut self, rows: usize, cols: usize) -> PooledMatrix {
        let matrix = self.available.pop()
            .unwrap_or_else(|| AlignmentMatrix::new(rows, cols));
        PooledMatrix::new(matrix, self)
    }
    
    pub fn release(&mut self, matrix: AlignmentMatrix) {
        if self.available.len() < MAX_POOL_SIZE {
            self.available.push(matrix);
        }
    }
}
```

## Concurrency Model

### Thread Pool Architecture

```mermaid
flowchart TD
    A["Main Thread<br/>CLI parsing, coordination"] --> B[Producer Queue]
    A --> C[Consumer Queue]
    
    B --> D1[Worker 1]
    B --> D2[Worker 2]
    B --> D3[Worker 3]
    B --> DN[Worker N]
    
    D1 --> C
    D2 --> C
    D3 --> C
    DN --> C
    
    style A stroke:#7b1fa2,stroke-width:3px,fill:#e1bee7
    style B stroke:#1976d2,stroke-width:2px,fill:#bbdefb
    style C stroke:#388e3c,stroke-width:2px,fill:#c8e6c9
    style D1 stroke:#00796b,stroke-width:2px
    style D2 stroke:#00796b,stroke-width:2px
    style D3 stroke:#00796b,stroke-width:2px
    style DN stroke:#00796b,stroke-width:2px
```

### Synchronization Primitives

```rust
pub struct SharedState {
    // Read-heavy data
    config: RwLock<Config>,
    
    // Write-heavy data
    progress: Mutex<Progress>,
    
    // Lock-free structures
    stats: AtomicU64,
    
    // Channel communication
    results: mpsc::Sender<Result>,
}
```

## Error Handling

### Error Hierarchy

```rust
#[derive(Debug, thiserror::Error)]
pub enum TalariaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Alignment error: {0}")]
    Alignment(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Download error: {0}")]
    Download(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, TalariaError>;
```

### Error Recovery

```rust
pub trait ErrorRecovery {
    fn recover(&self, error: &TalariaError) -> RecoveryAction;
}

pub enum RecoveryAction {
    Retry,
    Skip,
    Abort,
    Fallback(Box<dyn Fn() -> Result<()>>),
}
```

## Plugin System

### Plugin Interface

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn initialize(&mut self, config: &Config) -> Result<()>;
    fn execute(&self, context: &mut Context) -> Result<()>;
}

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
    hooks: HashMap<String, Vec<PluginHook>>,
}
```

### Hook Points

```mermaid
flowchart LR
    A[Application Start] --> B[pre_init]
    B --> C[post_init]
    C --> D[pre_reduction]
    D --> E[Reduction Process]
    E --> F[post_reduction]
    F --> G[pre_alignment]
    G --> H[Alignment Process]
    H --> I[post_alignment]
    I --> J[pre_output]
    J --> K[Write Output]
    K --> L[post_output]
    L --> M[Application End]
    
    style A stroke:#1976d2,stroke-width:2px,fill:#bbdefb
    style E stroke:#512da8,stroke-width:3px,fill:#d1c4e9
    style H stroke:#512da8,stroke-width:3px,fill:#d1c4e9
    style K stroke:#388e3c,stroke-width:2px,fill:#c8e6c9
    style M stroke:#388e3c,stroke-width:3px,fill:#a5d6a7
```

## Testing Architecture

### Test Structure

```
tests/
├── unit/               # Unit tests
│   ├── alignment_test.rs
│   ├── delta_test.rs
│   └── parser_test.rs
│
├── integration/        # Integration tests
│   ├── reduce_test.rs
│   ├── download_test.rs
│   └── cli_test.rs
│
├── fixtures/           # Test data
│   ├── small.fasta
│   ├── large.fasta
│   └── edge_cases.fasta
│
└── benchmarks/         # Performance tests
    ├── alignment_bench.rs
    ├── parsing_bench.rs
    └── reduction_bench.rs
```

### Testing Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    
    // Property-based testing
    proptest! {
        #[test]
        fn test_alignment_symmetry(seq1 in sequence_strategy(),
                                   seq2 in sequence_strategy()) {
            let score1 = align(&seq1, &seq2);
            let score2 = align(&seq2, &seq1);
            prop_assert_eq!(score1, score2);
        }
    }
    
    // Fuzz testing
    #[test]
    fn fuzz_parser() {
        let data = include_bytes!("../fuzz/corpus/parser/crash-1");
        let _ = parse_fasta(data);
    }
}
```

## Performance Considerations

### Hot Paths

1. **Alignment Inner Loop**: SIMD-optimized
2. **FASTA Parsing**: Zero-copy parsing
3. **Delta Encoding**: Bit-packed representation
4. **Cache Lookup**: Lock-free hash maps

### Optimization Techniques

```rust
// Branch prediction hints
#[inline(always)]
#[cold]
fn handle_error(e: Error) { /* ... */ }

// Cache-friendly data layout
#[repr(C, align(64))]
struct CacheAligned {
    data: [u8; 64],
}

// SIMD operations
#[target_feature(enable = "avx2")]
unsafe fn simd_compare(a: &[u8], b: &[u8]) -> u32 {
    // AVX2 implementation
}
```

## Security Considerations

### Input Validation

```rust
pub struct InputValidator {
    max_sequence_length: usize,
    max_file_size: usize,
    allowed_characters: HashSet<u8>,
}

impl InputValidator {
    pub fn validate(&self, input: &[u8]) -> Result<()> {
        if input.len() > self.max_file_size {
            return Err(TalariaError::InvalidInput("File too large"));
        }
        // Additional validation
        Ok(())
    }
}
```

### Sandboxing

```rust
#[cfg(target_os = "linux")]
pub fn setup_sandbox() -> Result<()> {
    use syscallz::{Context, Syscall, Action};
    
    let mut ctx = Context::init()?;
    ctx.allow_syscall(Syscall::read)?;
    ctx.allow_syscall(Syscall::write)?;
    ctx.allow_syscall(Syscall::mmap)?;
    // Restrict other syscalls
    ctx.load()?;
    
    Ok(())
}
```

## Future Architecture

### Planned Enhancements

1. **Distributed Processing**: MPI support
2. **Cloud Integration**: S3/GCS backends
3. **GPU Acceleration**: CUDA/OpenCL kernels
4. **Web Assembly**: Browser-based version
5. **gRPC API**: Remote procedure calls

### Extensibility Points

```rust
pub trait Extension {
    fn extend_cli(&self, app: App) -> App;
    fn extend_config(&self, config: &mut Config);
    fn extend_pipeline(&self, pipeline: &mut Pipeline);
}
```

## See Also

- [Building](building.md) - Build instructions
- [Contributing](contributing.md) - Development guidelines
- [API Reference](../api/lib.md) - Library documentation
- [Performance](../advanced/performance.md) - Optimization guide