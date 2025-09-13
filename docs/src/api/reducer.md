# Reducer API Reference

The `Reducer` struct is the core component of Talaria's sequence reduction engine. This document provides reference for the public API of the Reducer struct and its builder methods.

## Struct Definition

```rust
pub struct Reducer {
    config: Config,
    progress_callback: Option<Box<dyn Fn(&str, f64) + Send + Sync>>,
    use_similarity: bool,
    use_alignment: bool,
    silent: bool,
    no_deltas: bool,
    max_align_length: usize,
}
```

## Constructor

### `new(config: Config) -> Self`

Creates a new Reducer instance with the provided configuration.

```rust
use talaria::core::reducer::Reducer;
use talaria::core::config::Config;

let config = Config::default();
let reducer = Reducer::new(config);
```

## Builder Methods

The Reducer uses the builder pattern for configuration. All builder methods consume `self` and return `Self` for method chaining.

### `with_selection_mode(use_similarity: bool, use_alignment: bool) -> Self`

Configures the reference selection algorithm.

**Parameters:**
- `use_similarity`: Enable k-mer similarity-based selection
- `use_alignment`: Enable full alignment-based selection (slower but more accurate)

```rust
let reducer = Reducer::new(config)
    .with_selection_mode(true, false);  // Use similarity but not alignment
```

### `with_silent(silent: bool) -> Self`

Enables or disables silent mode, which suppresses all progress output and statistics.

**Parameters:**
- `silent`: If true, suppress all output (useful for benchmarks and tests)

```rust
let reducer = Reducer::new(config)
    .with_silent(true);  // No progress bars or statistics
```

### `with_no_deltas(no_deltas: bool) -> Self`

Controls whether delta encoding is performed.

**Parameters:**
- `no_deltas`: If true, skip delta encoding entirely (faster but no reconstruction possible)

```rust
let reducer = Reducer::new(config)
    .with_no_deltas(true);  // Skip delta encoding for speed
```

### `with_max_align_length(max_length: usize) -> Self`

Sets the maximum sequence length for alignment during delta encoding.

**Parameters:**
- `max_length`: Maximum sequence length in residues (default: 10000)

```rust
let reducer = Reducer::new(config)
    .with_max_align_length(5000);  // Limit alignment to 5000 residues
```

### `with_progress_callback<F>(callback: F) -> Self`

Sets a progress callback function for custom progress reporting.

**Parameters:**
- `callback`: Function that receives progress messages and percentage (0.0-100.0)

```rust
let reducer = Reducer::new(config)
    .with_progress_callback(|msg, pct| {
        println!("{}: {:.1}%", msg, pct);
    });
```

## Main Method

### `reduce(sequences: Vec<Sequence>, reduction_ratio: f64, target_aligner: TargetAligner) -> Result<(Vec<Sequence>, Vec<DeltaRecord>), TalariaError>`

Performs the main reduction operation on a set of sequences.

**Parameters:**
- `sequences`: Input sequences to reduce
- `reduction_ratio`: Target reduction ratio (0.0-1.0, where 0.3 = 30% of original)
- `target_aligner`: The target aligner to optimize for

**Returns:**
- `Ok((references, deltas))`: Reference sequences and delta records
- `Err(TalariaError)`: Error if reduction fails

```rust
use talaria::cli::TargetAligner;

let (references, deltas) = reducer.reduce(
    sequences,
    0.3,  // Reduce to 30%
    TargetAligner::Generic
)?;
```

## Usage Examples

### Basic Reduction

```rust
use talaria::core::reducer::Reducer;
use talaria::core::config::Config;
use talaria::cli::TargetAligner;

let config = Config::default();
let reducer = Reducer::new(config);

let (refs, deltas) = reducer.reduce(sequences, 0.3, TargetAligner::Generic)?;
```

### Fast Reduction Without Deltas

```rust
let reducer = Reducer::new(config)
    .with_no_deltas(true)
    .with_silent(true);

let (refs, _) = reducer.reduce(sequences, 0.3, TargetAligner::Blast)?;
// deltas will be empty due to with_no_deltas(true)
```

### High-Quality Reduction

```rust
let reducer = Reducer::new(config)
    .with_selection_mode(true, true)  // Use both similarity and alignment
    .with_max_align_length(20000);    // Allow longer alignments

let (refs, deltas) = reducer.reduce(sequences, 0.5, TargetAligner::Diamond)?;
```

### Benchmark Mode

```rust
let reducer = Reducer::new(config)
    .with_silent(true)  // No output for clean benchmark results
    .with_no_deltas(true);  // Skip deltas for speed

let (refs, _) = reducer.reduce(sequences, 0.3, TargetAligner::Generic)?;
```

## Performance Considerations

1. **Delta Encoding**: The most time-consuming step. Use `with_no_deltas(true)` for faster processing when reconstruction is not needed.

2. **Alignment Length**: Long sequences significantly slow down delta encoding. Use `with_max_align_length()` to limit alignment computation.

3. **Selection Mode**: 
   - Default (no flags): Fast, simple length-based selection
   - Similarity-based: Moderate speed, better clustering
   - Alignment-based: Slowest but highest quality

4. **Silent Mode**: Use `with_silent(true)` in automated pipelines and benchmarks to avoid output overhead.

## Error Handling

The `reduce()` method returns a `Result` with possible errors:

- `TalariaError::InvalidInput`: Invalid reduction ratio or empty sequences
- `TalariaError::AllocationError`: Insufficient memory
- `TalariaError::AlignmentError`: Alignment computation failed

## Thread Safety

The Reducer is `Send` but not `Sync`. Create separate instances for parallel processing or use within a single thread.

## See Also

- [Configuration API](configuration.md) - Config struct documentation
- [CLI Reference](cli.md) - Command-line interface
- [Algorithms](../algorithms/reduction.md) - Reduction algorithm details