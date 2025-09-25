# Talaria Tools Module

## Overview

The `talaria-tools` module provides a comprehensive framework for managing, installing, and interfacing with external bioinformatics alignment tools. This module serves as the bridge between Talaria's sequence reduction algorithms and high-performance alignment tools like LAMBDA, BLAST, DIAMOND, and MMseqs2.

### Key Features

- **Tool Management**: Automated download, installation, and version management
- **Aligner Abstraction**: Unified interface for different alignment tools
- **Parser Framework**: Comprehensive accession parsing for various sequence databases
- **Batch Processing**: Efficient handling of large sequence datasets
- **Error Recovery**: Robust error handling with workspace preservation
- **Mock Testing**: Testing framework with mock aligners

## Architecture

```
talaria-tools/
├── src/
│   ├── aligners/           # Aligner implementations
│   │   ├── lambda/         # LAMBDA aligner integration
│   │   │   ├── mod.rs      # Main LAMBDA aligner implementation
│   │   │   ├── parser.rs   # Accession parsing utilities
│   │   │   └── utils.rs    # LAMBDA-specific utilities
│   │   └── mod.rs          # Aligner module exports
│   ├── manager/            # Tool management
│   │   ├── mod.rs          # Tool manager implementation
│   │   └── installer.rs    # Tool installation logic
│   ├── testing/            # Testing utilities
│   │   ├── mod.rs          # Testing module exports
│   │   └── mock.rs         # Mock aligner for testing
│   ├── traits/             # Core trait definitions
│   │   ├── mod.rs          # Trait module exports
│   │   └── aligner.rs      # Aligner trait definitions
│   ├── types.rs            # Common type definitions
│   └── lib.rs              # Module exports
├── tests/                  # Integration tests
└── Cargo.toml             # Package configuration
```

## Core Components

### 1. Aligner Trait System (`traits/`)

The foundation of the module's abstraction layer, providing a unified interface for all alignment tools.

#### Core Traits

```rust
pub trait Aligner: Send + Sync {
    /// Perform sequence alignment search
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentSummary>>;

    /// Get aligner version information
    fn version(&self) -> Result<String>;

    /// Check if the aligner is available
    fn is_available(&self) -> bool;
}

pub trait ConfigurableAligner: Aligner {
    /// Get current configuration
    fn config(&self) -> &AlignmentConfig;

    /// Update configuration
    fn set_config(&mut self, config: AlignmentConfig);
}
```

#### Alignment Results

```rust
pub struct AlignmentSummary {
    pub query_id: String,
    pub reference_id: String,
    pub score: f64,
    pub e_value: f64,
    pub bit_score: f64,
    pub percent_identity: f64,
    pub alignment_length: usize,
    pub query_start: usize,
    pub query_end: usize,
    pub reference_start: usize,
    pub reference_end: usize,
    pub gaps: usize,
    pub mismatches: usize,
}

pub struct AlignmentConfig {
    pub threads: usize,
    pub e_value_threshold: f64,
    pub max_targets: usize,
    pub sensitivity: SensitivityMode,
    pub output_format: OutputFormat,
    pub batch_size: usize,
    pub preserve_on_failure: bool,
}
```

### 2. LAMBDA Aligner (`aligners/lambda/`)

The primary aligner implementation, optimized for protein sequence alignment with SEQUOIA integration.

#### Key Features

- **High Performance**: Utilizes LAMBDA's optimized indexing for fast searches
- **Taxonomy Integration**: Supports NCBI taxonomy for taxonomic filtering
- **Batch Processing**: Handles large datasets through intelligent batching
- **Memory Management**: Configurable memory limits and batch sizing
- **Progress Tracking**: Real-time progress monitoring during alignment

#### Implementation Details

```rust
pub struct LambdaAligner {
    workspace: PathBuf,
    config: AlignmentConfig,
    acc_tax_map: Option<PathBuf>,
    tax_dump_dir: Option<PathBuf>,
    batch_enabled: bool,
    batch_size: usize,
    preserve_on_failure: bool,
    failed: AtomicBool,
}
```

#### Workspace Structure

```
${TALARIA_WORKSPACE_DIR}/lambda_{id}/
├── indices/            # LAMBDA index files
│   ├── db.fasta        # Reference sequences
│   ├── db.idx.*        # LAMBDA index files
│   └── acc_tax.tsv     # Accession-taxonomy mapping
├── temp/               # Temporary processing files
│   ├── batch_*.fasta   # Batch query files
│   └── results_*.m8    # Batch results
├── iterations/         # Per-iteration results
│   ├── iter_0/         # First iteration
│   │   ├── query.fasta
│   │   ├── results.m8
│   │   └── stats.json
│   └── iter_N/         # Nth iteration
└── logs/               # Execution logs
    ├── lambda.log
    └── error.log
```

#### Accession Parser Framework

The parser framework handles various sequence database formats:

```rust
pub trait AccessionParser: Send + Sync {
    fn parse_header(&self, header: &str) -> Vec<String>;
    fn parse_accession(&self, text: &str) -> Option<String>;
    fn extract_all_forms(&self, accession: &str) -> Vec<String>;
}

// Implementations
pub struct UniProtParser; // sp|P12345|PROT_HUMAN
pub struct NCBIParser;    // NP_001234.1, XP_567890.2
pub struct PDBParser;     // pdb|1ABC|A
pub struct GenericParser; // Generic patterns

pub struct ComprehensiveAccessionParser {
    parsers: Vec<Box<dyn AccessionParser>>,
}
```

### 3. Tool Manager (`manager/`)

Handles the lifecycle of external bioinformatics tools.

#### Core Functionality

```rust
pub struct ToolManager {
    tools_dir: PathBuf,
    tools: HashMap<Tool, ToolInfo>,
}

pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub installed: bool,
    pub last_updated: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

impl ToolManager {
    /// Install a specific tool
    pub async fn install(&mut self, tool: Tool) -> Result<()>;

    /// Update an installed tool
    pub async fn update(&mut self, tool: Tool) -> Result<()>;

    /// Remove an installed tool
    pub fn remove(&mut self, tool: Tool) -> Result<()>;

    /// Check if a tool is installed
    pub fn is_installed(&self, tool: Tool) -> bool;

    /// Get tool executable path
    pub fn get_path(&self, tool: Tool) -> Option<PathBuf>;

    /// List all managed tools
    pub fn list(&self) -> Vec<ToolInfo>;
}
```

#### Installation Process

1. **Version Detection**: Query GitHub API for latest release
2. **Download**: Fetch appropriate binary for platform
3. **Verification**: Checksum validation
4. **Installation**: Extract and set permissions
5. **Registration**: Update tool registry

#### Platform Support

```rust
#[cfg(target_os = "linux")]
fn get_platform_suffix() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "linux-x86_64",
        "aarch64" => "linux-aarch64",
        _ => "linux-generic",
    }
}

#[cfg(target_os = "macos")]
fn get_platform_suffix() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "darwin-x86_64",
        "aarch64" => "darwin-aarch64",
        _ => "darwin-universal",
    }
}
```

### 4. Tool Types (`types.rs`)

Defines the supported tools and their metadata.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Lambda,  // LAMBDA aligner
    Blast,   // NCBI BLAST+
    Diamond, // DIAMOND aligner
    Mmseqs2, // MMseqs2 aligner
}

impl Tool {
    pub fn name(&self) -> &'static str;
    pub fn display_name(&self) -> &'static str;
    pub fn github_repo(&self) -> &'static str;
    pub fn binary_name(&self) -> &'static str;
    pub fn download_url(&self, version: &str) -> String;
    pub fn minimum_version(&self) -> &'static str;
}
```

### 5. Testing Framework (`testing/`)

Provides mock implementations for testing without external dependencies.

```rust
pub struct MockAligner;

impl Aligner for MockAligner {
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>> {
        // Return mock results for testing
        Ok(generate_mock_results(query, reference))
    }
}
```

## Integration with Talaria

### 1. Reference Selection Pipeline

The tools module integrates with Talaria's reference selection algorithm:

```rust
// In talaria-cli/src/core/reference_selector.rs
use talaria_tools::{LambdaAligner, Aligner};

pub struct ReferenceSelector {
    aligner: Box<dyn Aligner>,
    config: SelectionConfig,
}

impl ReferenceSelector {
    pub fn select_references(
        &mut self,
        sequences: &[Sequence],
    ) -> Result<Vec<usize>> {
        // Iterative selection using aligner
        let mut selected = Vec::new();
        let mut remaining = sequences.to_vec();

        while !remaining.is_empty() {
            // Align remaining against selected
            let results = self.aligner.search(&remaining, &selected)?;

            // Select best reference
            let best_ref = self.find_best_reference(&results);
            selected.push(best_ref);

            // Update remaining sequences
            remaining = self.filter_covered(&remaining, &results);
        }

        Ok(selected)
    }
}
```

### 2. Reduction Workflow

```mermaid
graph TD
    A[Input Sequences] --> B[Initialize Aligner]
    B --> C{Aligner Available?}
    C -->|Yes| D[Create Index]
    C -->|No| E[Download/Install]
    E --> D
    D --> F[Iterative Selection]
    F --> G[Align Sequences]
    G --> H[Compute Coverage]
    H --> I{Coverage Sufficient?}
    I -->|No| J[Select New Reference]
    J --> F
    I -->|Yes| K[Output References]
    K --> L[Delta Encoding]
```

### 3. Configuration

Tools are configured through environment variables and configuration files:

```toml
# ~/.talaria/config.toml
[tools]
directory = "/home/user/.talaria/tools"
auto_update = true
update_check_interval = 86400  # seconds

[tools.lambda]
version = "3.0.0"
threads = 8
memory_limit = "16G"
batch_size = 100000
preserve_workspace = false

[tools.blast]
version = "2.14.0"
threads = 4
e_value = 0.001
max_targets = 500

[tools.diamond]
version = "2.1.8"
sensitivity = "sensitive"
threads = 8
block_size = 2.0
```

## Usage Examples

### Basic Tool Installation

```rust
use talaria_tools::{ToolManager, Tool};

#[tokio::main]
async fn main() -> Result<()> {
    let mut manager = ToolManager::new()?;

    // Install LAMBDA
    manager.install(Tool::Lambda).await?;

    // Check installation
    if manager.is_installed(Tool::Lambda) {
        let path = manager.get_path(Tool::Lambda).unwrap();
        println!("LAMBDA installed at: {:?}", path);
    }

    Ok(())
}
```

### Sequence Alignment

```rust
use talaria_tools::{LambdaAligner, Aligner, AlignmentConfig};
use talaria_bio::sequence::Sequence;

fn align_sequences(
    queries: Vec<Sequence>,
    references: Vec<Sequence>,
) -> Result<Vec<AlignmentResult>> {
    let config = AlignmentConfig {
        threads: 8,
        e_value_threshold: 0.001,
        max_targets: 100,
        batch_size: 50000,
        ..Default::default()
    };

    let mut aligner = LambdaAligner::new(config)?;
    aligner.search(&queries, &references)
}
```

### Custom Aligner Implementation

```rust
use talaria_tools::traits::{Aligner, AlignmentResult};

struct CustomAligner {
    // Custom fields
}

impl Aligner for CustomAligner {
    fn search(
        &mut self,
        query: &[Sequence],
        reference: &[Sequence],
    ) -> Result<Vec<AlignmentResult>> {
        // Custom alignment logic
        todo!()
    }

    fn version(&self) -> Result<String> {
        Ok("Custom Aligner v1.0.0".to_string())
    }

    fn is_available(&self) -> bool {
        // Check if custom aligner is available
        true
    }
}
```

### Batch Processing

```rust
use talaria_tools::LambdaAligner;

fn process_large_dataset(
    sequences: Vec<Sequence>,
    references: Vec<Sequence>,
) -> Result<()> {
    let mut aligner = LambdaAligner::builder()
        .batch_enabled(true)
        .batch_size(100_000_000)  // 100M amino acids per batch
        .threads(16)
        .preserve_on_failure(true)
        .build()?;

    // Process in batches automatically
    let results = aligner.search(&sequences, &references)?;

    println!("Processed {} alignments", results.len());
    Ok(())
}
```

## Performance Optimization

### 1. Index Caching

LAMBDA indices are cached to avoid rebuilding:

```rust
fn get_or_create_index(
    reference: &[Sequence],
    cache_dir: &Path,
) -> Result<PathBuf> {
    let hash = compute_hash(reference);
    let index_path = cache_dir.join(format!("index_{}.idx", hash));

    if index_path.exists() {
        // Use cached index
        Ok(index_path)
    } else {
        // Build new index
        build_lambda_index(reference, &index_path)?;
        Ok(index_path)
    }
}
```

### 2. Memory Management

```rust
impl LambdaAligner {
    fn estimate_memory_usage(&self, sequences: &[Sequence]) -> usize {
        let seq_memory = sequences.iter()
            .map(|s| s.sequence.len())
            .sum::<usize>();

        let index_memory = self.estimate_index_memory();
        let working_memory = seq_memory * 4; // Estimate

        seq_memory + index_memory + working_memory
    }

    fn adjust_batch_size(&mut self, available_memory: usize) {
        let estimated = self.estimate_memory_usage(&self.current_batch);
        if estimated > available_memory {
            self.config.batch_size /= 2;
        }
    }
}
```

### 3. Parallel Processing

```rust
use rayon::prelude::*;

fn parallel_alignment(
    queries: Vec<Sequence>,
    references: Vec<Sequence>,
    num_threads: usize,
) -> Result<Vec<AlignmentResult>> {
    let chunk_size = queries.len() / num_threads;

    queries
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut aligner = LambdaAligner::new(config)?;
            aligner.search(chunk, &references)
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect()
}
```

## Error Handling

### Workspace Preservation

```rust
impl Drop for LambdaAligner {
    fn drop(&mut self) {
        if self.preserve_on_failure && self.failed.load(Ordering::Relaxed) {
            eprintln!("Preserving workspace at: {:?}", self.workspace);
        } else {
            let _ = std::fs::remove_dir_all(&self.workspace);
        }
    }
}
```

### Error Recovery

```rust
pub enum AlignmentError {
    ToolNotFound(Tool),
    IndexCreationFailed(String),
    AlignmentFailed(String),
    ParseError(String),
    IoError(std::io::Error),
    MemoryExceeded(usize),
}

impl LambdaAligner {
    fn recover_from_error(&mut self, error: &AlignmentError) -> Result<()> {
        match error {
            AlignmentError::MemoryExceeded(required) => {
                // Reduce batch size and retry
                self.config.batch_size /= 2;
                Ok(())
            }
            AlignmentError::IndexCreationFailed(_) => {
                // Clear cache and rebuild
                self.clear_index_cache()?;
                Ok(())
            }
            _ => Err(error.clone().into()),
        }
    }
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use talaria_tools::testing::MockAligner;

    #[test]
    fn test_mock_aligner() {
        let mut aligner = MockAligner::new();
        let queries = vec![create_test_sequence("Q1", "ACGT")];
        let references = vec![create_test_sequence("R1", "ACGT")];

        let results = aligner.search(&queries, &references).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_accession_parser() {
        let parser = UniProtParser;
        let header = "sp|P12345|PROT_HUMAN Protein description";
        let accessions = parser.parse_header(header);

        assert!(accessions.contains(&"P12345".to_string()));
        assert!(accessions.contains(&"sp|P12345|PROT_HUMAN".to_string()));
    }
}
```

### Integration Tests

```rust
#[test]
#[ignore] // Requires LAMBDA installation
fn test_lambda_alignment() {
    let config = AlignmentConfig::default();
    let mut aligner = LambdaAligner::new(config).unwrap();

    let queries = load_test_sequences("queries.fasta");
    let references = load_test_sequences("references.fasta");

    let results = aligner.search(&queries, &references).unwrap();

    // Verify results
    assert!(results.len() > 0);
    for result in results {
        assert!(result.e_value <= 0.001);
        assert!(result.percent_identity >= 30.0);
    }
}
```

### Benchmarks

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_alignment(c: &mut Criterion) {
    let queries = generate_random_sequences(100, 300);
    let references = generate_random_sequences(1000, 300);

    c.bench_function("lambda_alignment", |b| {
        let mut aligner = LambdaAligner::new(Default::default()).unwrap();
        b.iter(|| {
            aligner.search(
                black_box(&queries),
                black_box(&references),
            )
        });
    });
}

criterion_group!(benches, benchmark_alignment);
criterion_main!(benches);
```

## Troubleshooting

### Common Issues

1. **Tool Not Found**
   ```
   Error: Tool 'lambda' not found
   Solution: Run 'talaria tools install lambda'
   ```

2. **Memory Exceeded**
   ```
   Error: Memory limit exceeded during alignment
   Solution: Reduce batch size or increase memory limit
   ```

3. **Index Creation Failed**
   ```
   Error: Failed to create LAMBDA index
   Solution: Check disk space and permissions
   ```

### Debug Mode

Enable detailed logging:

```bash
export TALARIA_LOG=trace
export TALARIA_LAMBDA_VERBOSE=1
export TALARIA_PRESERVE_ON_FAILURE=1
```

### Performance Tuning

```toml
# Optimize for speed
[tools.lambda]
threads = 32
batch_size = 500_000_000
sensitivity = "fast"

# Optimize for sensitivity
[tools.lambda]
threads = 16
batch_size = 50_000_000
sensitivity = "very-sensitive"
```

## API Reference

### Core Types

- `Tool`: Enumeration of supported tools
- `ToolInfo`: Tool metadata and status
- `AlignmentResult`: Alignment search result
- `AlignmentConfig`: Alignment configuration
- `AlignmentError`: Error types

### Traits

- `Aligner`: Core aligner interface
- `ConfigurableAligner`: Extended configuration interface
- `AccessionParser`: Sequence header parsing

### Implementations

- `LambdaAligner`: LAMBDA aligner implementation
- `MockAligner`: Mock aligner for testing
- `ToolManager`: Tool lifecycle management

## Contributing

### Adding New Aligners

1. Implement the `Aligner` trait
2. Add tool definition to `types.rs`
3. Update `ToolManager` for installation
4. Add tests and documentation

### Parser Extensions

1. Implement `AccessionParser` trait
2. Add to `ComprehensiveAccessionParser`
3. Add test cases for new formats

## Dependencies

- `anyhow`: Error handling
- `serde`: Serialization
- `tokio`: Async runtime
- `reqwest`: HTTP client for downloads
- `tempfile`: Temporary file management
- `rayon`: Parallel processing
- `regex`: Pattern matching
- `chrono`: Date/time handling

## License

Part of the Talaria project. See main repository for license information.

## See Also

- [Talaria CLI Documentation](../talaria-cli/README.md)
- [Talaria Sequoia Documentation](../talaria-sequoia/README.md)
- [Talaria Bio Documentation](../talaria-bio/README.md)
- [LAMBDA Aligner](https://github.com/seqan/lambda)
- [NCBI BLAST+](https://blast.ncbi.nlm.nih.gov/)
- [DIAMOND](https://github.com/bbuchfink/diamond)
- [MMseqs2](https://github.com/soedinglab/MMseqs2)
