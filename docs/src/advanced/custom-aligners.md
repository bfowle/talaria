# Custom Aligners

Guide to implementing and integrating custom alignment algorithms and third-party aligners with Talaria.

## Aligner Interface

### Core Trait Definition

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Core trait that all aligners must implement
#[async_trait]
pub trait Aligner: Send + Sync {
    /// Unique identifier for the aligner
    fn name(&self) -> &str;
    
    /// Version information
    fn version(&self) -> &str;
    
    /// Check if aligner is available on system
    async fn is_available(&self) -> bool;
    
    /// Initialize the aligner
    async fn initialize(&mut self, config: AlignerConfig) -> Result<()>;
    
    /// Perform alignment
    async fn align(
        &self,
        query: &Sequence,
        reference: &Sequence,
        params: AlignmentParams,
    ) -> Result<Alignment>;
    
    /// Batch alignment for efficiency
    async fn align_batch(
        &self,
        queries: &[Sequence],
        references: &[Sequence],
        params: AlignmentParams,
    ) -> Result<Vec<Alignment>>;
    
    /// Get optimization hints for reduction
    fn optimization_hints(&self) -> OptimizationHints;
}
```

### Configuration Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignerConfig {
    /// Path to aligner executable (if external)
    pub executable_path: Option<PathBuf>,
    
    /// Number of threads to use
    pub threads: usize,
    
    /// Memory limit in MB
    pub memory_limit: Option<usize>,
    
    /// Temporary directory for intermediate files
    pub temp_dir: PathBuf,
    
    /// Custom parameters
    pub custom_params: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct OptimizationHints {
    /// Preferred k-mer size
    pub kmer_size: Option<usize>,
    
    /// Minimum sequence length
    pub min_sequence_length: usize,
    
    /// Whether aligner benefits from sorted input
    pub prefers_sorted: bool,
    
    /// Whether aligner can use indexed references
    pub supports_indexing: bool,
    
    /// Optimal chunk size for batch processing
    pub optimal_batch_size: usize,
}
```

## Implementing Custom Aligners

### Basic Implementation

```rust
pub struct MyCustomAligner {
    name: String,
    config: AlignerConfig,
    initialized: bool,
}

#[async_trait]
impl Aligner for MyCustomAligner {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn version(&self) -> &str {
        "1.0.0"
    }
    
    async fn is_available(&self) -> bool {
        // Check if required dependencies are available
        if let Some(ref exe) = self.config.executable_path {
            exe.exists()
        } else {
            true // Built-in aligner
        }
    }
    
    async fn initialize(&mut self, config: AlignerConfig) -> Result<()> {
        self.config = config;
        
        // Perform any initialization steps
        self.setup_working_directory()?;
        self.validate_parameters()?;
        
        self.initialized = true;
        Ok(())
    }
    
    async fn align(
        &self,
        query: &Sequence,
        reference: &Sequence,
        params: AlignmentParams,
    ) -> Result<Alignment> {
        if !self.initialized {
            return Err(anyhow!("Aligner not initialized"));
        }
        
        // Implement alignment logic
        let score = self.calculate_alignment_score(query, reference, &params)?;
        
        Ok(Alignment {
            query_id: query.id.clone(),
            reference_id: reference.id.clone(),
            score,
            identity: self.calculate_identity(query, reference),
            alignment_length: query.len().max(reference.len()),
            gaps: self.count_gaps(query, reference),
        })
    }
    
    async fn align_batch(
        &self,
        queries: &[Sequence],
        references: &[Sequence],
        params: AlignmentParams,
    ) -> Result<Vec<Alignment>> {
        // Parallel batch processing
        use rayon::prelude::*;
        
        queries.par_iter()
            .flat_map(|query| {
                references.par_iter()
                    .map(|reference| {
                        futures::executor::block_on(
                            self.align(query, reference, params.clone())
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Result<Vec<_>>>()
    }
    
    fn optimization_hints(&self) -> OptimizationHints {
        OptimizationHints {
            kmer_size: Some(21),
            min_sequence_length: 50,
            prefers_sorted: false,
            supports_indexing: true,
            optimal_batch_size: 1000,
        }
    }
}
```

### External Tool Integration

```rust
use tokio::process::Command;

pub struct ExternalAligner {
    executable: PathBuf,
    work_dir: PathBuf,
    config: AlignerConfig,
}

impl ExternalAligner {
    async fn run_external_command(
        &self,
        query_file: &Path,
        reference_file: &Path,
        output_file: &Path,
        params: &AlignmentParams,
    ) -> Result<()> {
        let mut cmd = Command::new(&self.executable);
        
        // Add standard arguments
        cmd.arg("-query").arg(query_file)
           .arg("-subject").arg(reference_file)
           .arg("-out").arg(output_file)
           .arg("-num_threads").arg(self.config.threads.to_string());
        
        // Add custom parameters
        for (key, value) in &params.custom_params {
            cmd.arg(format!("-{}", key)).arg(value);
        }
        
        // Execute command
        let output = cmd.output().await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("External aligner failed: {}", stderr));
        }
        
        Ok(())
    }
    
    async fn parse_output(&self, output_file: &Path) -> Result<Vec<Alignment>> {
        let content = tokio::fs::read_to_string(output_file).await?;
        
        // Parse aligner-specific output format
        let alignments = content.lines()
            .filter_map(|line| self.parse_alignment_line(line).ok())
            .collect();
        
        Ok(alignments)
    }
}
```

## Plugin System

### Plugin Architecture

```rust
use libloading::{Library, Symbol};

pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Aligner>>,
    libraries: Vec<Library>,
}

impl PluginManager {
    pub fn load_plugin(&mut self, path: &Path) -> Result<()> {
        unsafe {
            let lib = Library::new(path)?;
            
            // Get plugin metadata
            let get_metadata: Symbol<fn() -> PluginMetadata> = 
                lib.get(b"get_plugin_metadata")?;
            let metadata = get_metadata();
            
            // Create aligner instance
            let create_aligner: Symbol<fn() -> Box<dyn Aligner>> = 
                lib.get(b"create_aligner")?;
            let aligner = create_aligner();
            
            // Register plugin
            self.plugins.insert(metadata.name.clone(), aligner);
            self.libraries.push(lib);
            
            Ok(())
        }
    }
    
    pub fn get_aligner(&self, name: &str) -> Option<&dyn Aligner> {
        self.plugins.get(name).map(|b| b.as_ref())
    }
}

#[derive(Debug, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
}
```

### Writing Plugins

```rust
// my_plugin/src/lib.rs

use talaria_plugin_api::*;

pub struct MyAligner {
    // Implementation
}

impl Aligner for MyAligner {
    // Implement trait methods
}

#[no_mangle]
pub extern "C" fn get_plugin_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "my_aligner".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        author: "Your Name".to_string(),
        description: "Custom alignment algorithm".to_string(),
    }
}

#[no_mangle]
pub extern "C" fn create_aligner() -> Box<dyn Aligner> {
    Box::new(MyAligner::new())
}
```

## Advanced Features

### GPU Acceleration

```rust
pub struct GpuAligner {
    device: GpuDevice,
    kernels: HashMap<String, GpuKernel>,
}

impl GpuAligner {
    pub async fn align_gpu(
        &self,
        queries: &[Sequence],
        references: &[Sequence],
    ) -> Result<Vec<Alignment>> {
        // Transfer data to GPU
        let d_queries = self.device.upload(queries)?;
        let d_references = self.device.upload(references)?;
        
        // Allocate output buffer
        let d_output = self.device.allocate::<Alignment>(
            queries.len() * references.len()
        )?;
        
        // Launch kernel
        let kernel = &self.kernels["alignment"];
        kernel.launch(
            &[&d_queries, &d_references, &d_output],
            queries.len() as u32,
            references.len() as u32,
        )?;
        
        // Download results
        self.device.download(&d_output)
    }
}
```

### Adaptive Algorithm Selection

```rust
pub struct AdaptiveAligner {
    aligners: Vec<Box<dyn Aligner>>,
    selector: AlgorithmSelector,
}

impl AdaptiveAligner {
    pub async fn select_best_aligner(
        &self,
        sequences: &[Sequence],
    ) -> &dyn Aligner {
        let features = self.extract_features(sequences);
        let aligner_idx = self.selector.predict(&features);
        &*self.aligners[aligner_idx]
    }
    
    fn extract_features(&self, sequences: &[Sequence]) -> Features {
        Features {
            avg_length: sequences.iter().map(|s| s.len()).sum::<usize>() 
                / sequences.len(),
            gc_content: self.calculate_gc_content(sequences),
            complexity: self.calculate_complexity(sequences),
            similarity: self.estimate_similarity(sequences),
        }
    }
}
```

### Custom Scoring Matrices

```rust
pub struct CustomScoringMatrix {
    matrix: ndarray::Array2<i32>,
    alphabet: Vec<u8>,
}

impl CustomScoringMatrix {
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut lines = content.lines();
        
        // Parse alphabet
        let alphabet: Vec<u8> = lines.next()
            .ok_or_else(|| anyhow!("Empty scoring matrix file"))?
            .split_whitespace()
            .map(|s| s.as_bytes()[0])
            .collect();
        
        // Parse matrix
        let size = alphabet.len();
        let mut matrix = ndarray::Array2::zeros((size, size));
        
        for (i, line) in lines.enumerate() {
            for (j, value) in line.split_whitespace().enumerate() {
                matrix[[i, j]] = value.parse()?;
            }
        }
        
        Ok(Self { matrix, alphabet })
    }
    
    pub fn score(&self, a: u8, b: u8) -> i32 {
        let i = self.alphabet.iter().position(|&x| x == a).unwrap_or(0);
        let j = self.alphabet.iter().position(|&x| x == b).unwrap_or(0);
        self.matrix[[i, j]]
    }
}
```

## Integration Examples

### MAFFT Integration

```rust
pub struct MafftAligner {
    executable: PathBuf,
    threads: usize,
}

impl MafftAligner {
    pub async fn align_multiple(
        &self,
        sequences: &[Sequence],
    ) -> Result<MultipleAlignment> {
        // Write sequences to temporary file
        let input_file = self.write_temp_fasta(sequences).await?;
        let output_file = self.temp_file("mafft_output.fasta");
        
        // Run MAFFT
        let output = Command::new(&self.executable)
            .arg("--thread").arg(self.threads.to_string())
            .arg("--auto")
            .arg(input_file.path())
            .stdout(Stdio::piped())
            .output()
            .await?;
        
        // Parse aligned sequences
        let aligned = self.parse_fasta(&output.stdout)?;
        
        Ok(MultipleAlignment {
            sequences: aligned,
            score: self.calculate_alignment_score(&aligned),
        })
    }
}
```

### Minimap2 Integration

```rust
pub struct Minimap2Aligner {
    executable: PathBuf,
    preset: String,
}

impl Minimap2Aligner {
    pub async fn align_long_reads(
        &self,
        reads: &[Sequence],
        reference: &Path,
    ) -> Result<Vec<Alignment>> {
        let reads_file = self.write_temp_fastq(reads).await?;
        
        let output = Command::new(&self.executable)
            .arg("-x").arg(&self.preset)
            .arg("-t").arg(self.threads.to_string())
            .arg(reference)
            .arg(reads_file.path())
            .output()
            .await?;
        
        self.parse_paf(&output.stdout)
    }
    
    fn parse_paf(&self, data: &[u8]) -> Result<Vec<Alignment>> {
        let content = std::str::from_utf8(data)?;
        
        content.lines()
            .map(|line| {
                let fields: Vec<&str> = line.split('\t').collect();
                Ok(Alignment {
                    query_id: fields[0].to_string(),
                    reference_id: fields[5].to_string(),
                    score: fields[11].parse()?,
                    identity: fields[9].parse::<f64>()? / fields[10].parse::<f64>()?,
                    alignment_length: fields[10].parse()?,
                    gaps: 0, // PAF doesn't directly report gaps
                })
            })
            .collect()
    }
}
```

## Performance Optimization

### Caching Layer

```rust
pub struct CachedAligner<A: Aligner> {
    inner: A,
    cache: Arc<DashMap<(String, String), Alignment>>,
    max_cache_size: usize,
}

impl<A: Aligner> CachedAligner<A> {
    pub async fn align_with_cache(
        &self,
        query: &Sequence,
        reference: &Sequence,
        params: AlignmentParams,
    ) -> Result<Alignment> {
        let key = (query.id.clone(), reference.id.clone());
        
        // Check cache
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        
        // Compute alignment
        let alignment = self.inner.align(query, reference, params).await?;
        
        // Store in cache if under size limit
        if self.cache.len() < self.max_cache_size {
            self.cache.insert(key, alignment.clone());
        }
        
        Ok(alignment)
    }
}
```

### Parallel Pipeline

```rust
pub struct PipelinedAligner {
    stages: Vec<Box<dyn AlignmentStage>>,
}

#[async_trait]
trait AlignmentStage: Send + Sync {
    async fn process(
        &self,
        input: AlignmentData,
    ) -> Result<AlignmentData>;
}

impl PipelinedAligner {
    pub async fn align_pipeline(
        &self,
        sequences: Vec<Sequence>,
    ) -> Result<Vec<Alignment>> {
        let (tx, mut rx) = mpsc::channel(100);
        
        // Start pipeline
        let mut data = AlignmentData::new(sequences);
        
        for stage in &self.stages {
            data = stage.process(data).await?;
        }
        
        Ok(data.alignments)
    }
}
```

## Configuration

### Aligner Registry

```toml
[aligners.custom]
# Custom aligner configuration
name = "my_custom_aligner"
type = "plugin"
path = "/usr/local/lib/talaria/plugins/my_aligner.so"

[aligners.custom.params]
kmer_size = 21
min_score = 0.8
use_gpu = true

[aligners.external]
# External tool configuration
name = "blast"
type = "external"
executable = "/usr/bin/blastn"
version_check = "blastn -version"

[aligners.external.defaults]
evalue = "1e-5"
word_size = 11
num_threads = 8
```

### Dynamic Loading

```rust
pub struct AlignerRegistry {
    aligners: HashMap<String, Box<dyn Aligner>>,
    config: RegistryConfig,
}

impl AlignerRegistry {
    pub fn load_from_config(&mut self, config: &Config) -> Result<()> {
        for (name, aligner_config) in &config.aligners {
            let aligner = match aligner_config.aligner_type.as_str() {
                "builtin" => self.load_builtin(name)?,
                "plugin" => self.load_plugin(&aligner_config.path)?,
                "external" => self.load_external(aligner_config)?,
                _ => return Err(anyhow!("Unknown aligner type")),
            };
            
            self.register(name.clone(), aligner);
        }
        
        Ok(())
    }
}
```

## Testing Custom Aligners

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_custom_aligner() {
        let mut aligner = MyCustomAligner::new();
        aligner.initialize(Default::default()).await.unwrap();
        
        let query = Sequence::new("query", b"ACGTACGT");
        let reference = Sequence::new("ref", b"ACGTACGT");
        
        let alignment = aligner.align(
            &query,
            &reference,
            Default::default()
        ).await.unwrap();
        
        assert_eq!(alignment.identity, 1.0);
        assert_eq!(alignment.gaps, 0);
    }
    
    #[tokio::test]
    async fn test_batch_alignment() {
        let aligner = MyCustomAligner::new();
        let queries = vec![
            Sequence::new("q1", b"ACGT"),
            Sequence::new("q2", b"GCTA"),
        ];
        let references = vec![
            Sequence::new("r1", b"ACGT"),
            Sequence::new("r2", b"GCTA"),
        ];
        
        let alignments = aligner.align_batch(
            &queries,
            &references,
            Default::default()
        ).await.unwrap();
        
        assert_eq!(alignments.len(), 4);
    }
}
```

### Benchmarking

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_aligners(c: &mut Criterion) {
    let mut group = c.benchmark_group("aligners");
    
    let sequences = generate_test_sequences(1000);
    
    group.bench_function("custom_aligner", |b| {
        let aligner = MyCustomAligner::new();
        b.iter(|| {
            futures::executor::block_on(
                aligner.align_batch(&sequences, &sequences, Default::default())
            )
        });
    });
    
    group.bench_function("external_aligner", |b| {
        let aligner = ExternalAligner::new();
        b.iter(|| {
            futures::executor::block_on(
                aligner.align_batch(&sequences, &sequences, Default::default())
            )
        });
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_aligners);
criterion_main!(benches);
```

## Best Practices

1. **Interface Compliance**: Always implement the full Aligner trait
2. **Error Handling**: Provide detailed error messages
3. **Resource Management**: Clean up temporary files and memory
4. **Thread Safety**: Ensure aligners are thread-safe
5. **Documentation**: Document parameters and behavior
6. **Testing**: Comprehensive unit and integration tests
7. **Benchmarking**: Compare performance with standard aligners
8. **Compatibility**: Support standard file formats

## See Also

- [API Reference](../api/aligners.md) - Aligner API documentation
- [Performance](performance.md) - Optimization techniques
- [Parallel Processing](parallel.md) - Parallel alignment strategies
- [Configuration](../user-guide/configuration.md) - Aligner configuration