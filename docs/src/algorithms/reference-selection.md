# Reference Selection

Reference selection is a critical step in Talaria's reduction pipeline that determines which sequences will be stored in full and which will be delta-encoded.

## Overview

The reference selection algorithm identifies a minimal set of representative sequences that can effectively serve as references for delta encoding the remaining sequences in the dataset.

## Selection Strategies

### 1. Simple Greedy Selection (Default)

The default strategy uses a simple greedy algorithm based on sequence length:

```rust
fn select_references_simple(sequences: Vec<Sequence>, target_ratio: f64) -> SelectionResult {
    // Sort by length (descending)
    let mut sorted = sequences.clone();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
    
    // Select top N% as references
    let target_count = (sequences.len() as f64 * target_ratio) as usize;
    let references = sorted.into_iter().take(target_count).collect();
    
    // Assign remaining to closest reference by length
    assign_to_closest_reference(references, sequences)
}
```

This matches the original db-reduce behavior and requires no similarity calculations.

### 2. Similarity-Based Selection (Optional)

**Enable with**: `--similarity-threshold <value>`

Groups sequences into clusters and selects centroids:

1. **Cluster Formation**: Group sequences by k-mer similarity
2. **Centroid Selection**: Choose most representative sequence per cluster
3. **Refinement**: Adjust references based on cluster sizes

This is an optional feature not present in the original db-reduce.

### 3. Taxonomy-Aware Selection (Optional)

**Enable with**: `--taxonomy-aware`

**Note**: Currently uses simple taxon ID proximity, not true taxonomic distance.

Considers taxonomic IDs when selecting references:

```rust
fn select_with_taxonomy(sequences: Vec<Sequence>) -> SelectionResult {
    // Currently implemented as simple ID difference check:
    // if (taxon_a - taxon_b).abs() > 1000 { skip }
    
    // Full taxonomic tree support would require:
    // - NCBI taxonomy files (names.dmp, nodes.dmp)
    // - Building taxonomy tree
    // - Calculating true taxonomic distance
    
    // This is an optional feature not in original db-reduce
}
```

## Selection Criteria

### Primary Criteria

1. **Sequence Length**
   - Longer sequences preferred as references
   - Better coverage of sequence space
   - More reliable alignments

2. **Sequence Quality**
   - Low ambiguity (few N's)
   - Complete sequences (no gaps)
   - High confidence scores

3. **Representativeness**
   - Central position in sequence space
   - High similarity to cluster members
   - Good coverage of diversity

### Secondary Criteria

1. **Computational Efficiency**
   - Sequences that align quickly
   - Moderate complexity
   - Balanced composition

2. **Storage Efficiency**
   - Sequences that compress well
   - Minimal redundancy
   - Optimal for delta encoding

## Algorithm Details

### Default Behavior

By default, no similarity calculation is performed. References are selected purely by length.

### Optional: Similarity Calculation

**Enable with**: `--similarity-threshold` or `--align-select`

When enabled, similarity between sequences is calculated using:

```rust
fn calculate_similarity(seq1: &[u8], seq2: &[u8]) -> f64 {
    if use_exact_alignment {
        // Full Needleman-Wunsch alignment
        let alignment = align_global(seq1, seq2);
        alignment.identity()
    } else {
        // Fast k-mer based approximation
        let kmers1 = extract_kmers(seq1, k);
        let kmers2 = extract_kmers(seq2, k);
        jaccard_similarity(&kmers1, &kmers2)
    }
}
```

### Coverage Calculation

A reference covers a sequence if their similarity exceeds the threshold:

```rust
fn calculate_coverage(reference: &Sequence, sequences: &[Sequence], threshold: f64) -> Vec<usize> {
    sequences
        .iter()
        .enumerate()
        .filter_map(|(i, seq)| {
            if calculate_similarity(&reference.sequence, &seq.sequence) >= threshold {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}
```

## Optimization Techniques

### 1. K-mer Indexing

Pre-compute k-mer indices for fast similarity estimation:

```rust
struct KmerIndex {
    k: usize,
    index: HashMap<Kmer, Vec<SequenceId>>,
}

impl KmerIndex {
    fn find_similar(&self, sequence: &[u8], min_shared: usize) -> Vec<SequenceId> {
        let query_kmers = extract_kmers(sequence, self.k);
        let mut shared_counts = HashMap::new();
        
        for kmer in query_kmers {
            if let Some(seq_ids) = self.index.get(&kmer) {
                for id in seq_ids {
                    *shared_counts.entry(id).or_insert(0) += 1;
                }
            }
        }
        
        shared_counts
            .into_iter()
            .filter(|(_, count)| *count >= min_shared)
            .map(|(id, _)| id)
            .collect()
    }
}
```

### 2. Parallel Processing

Reference selection can be parallelized:

```rust
use rayon::prelude::*;

fn parallel_selection(sequences: Vec<Sequence>, threshold: f64) -> SelectionResult {
    let chunk_size = sequences.len() / num_cpus::get();
    
    let partial_results: Vec<_> = sequences
        .par_chunks(chunk_size)
        .map(|chunk| select_references_greedy(chunk.to_vec(), threshold))
        .collect();
    
    merge_selection_results(partial_results)
}
```

### 3. Incremental Selection

For large datasets, use incremental selection:

```rust
fn incremental_selection(sequences: impl Iterator<Item = Sequence>, threshold: f64) -> SelectionResult {
    let mut references = Vec::new();
    let mut buffer = Vec::new();
    const BUFFER_SIZE: usize = 10000;
    
    for sequence in sequences {
        buffer.push(sequence);
        
        if buffer.len() >= BUFFER_SIZE {
            let new_refs = select_from_buffer(&buffer, &references, threshold);
            references.extend(new_refs);
            buffer.clear();
        }
    }
    
    // Process remaining
    if !buffer.is_empty() {
        let new_refs = select_from_buffer(&buffer, &references, threshold);
        references.extend(new_refs);
    }
    
    SelectionResult { references, ... }
}
```

## Quality Metrics

### Coverage Metric

Percentage of sequences that can be delta-encoded:

```
Coverage = (Sequences with reference / Total sequences) × 100%
```

### Compression Ratio

Expected compression after delta encoding:

```
Compression Ratio = Original Size / (Reference Size + Delta Size)
```

### Diversity Metric

How well references represent sequence diversity:

```rust
fn calculate_diversity(references: &[Sequence], all_sequences: &[Sequence]) -> f64 {
    let ref_kmers = extract_all_kmers(references);
    let all_kmers = extract_all_kmers(all_sequences);
    
    ref_kmers.intersection(&all_kmers).count() as f64 / all_kmers.len() as f64
}
```

## Configuration Parameters

### Threshold Settings

```toml
[reduction]
# Default configuration (matches original db-reduce)
similarity_threshold = 0.0  # Disabled by default
min_sequence_length = 50    # Minimum length for references
max_delta_distance = 100    # Maximum allowed differences
taxonomy_aware = false      # Disabled by default

# Optional: Enable advanced features
# similarity_threshold = 0.9  # Enable similarity-based selection
# taxonomy_aware = true       # Enable taxonomy consideration
```

### Strategy Selection

```rust
pub enum SelectionStrategy {
    Simple,              // Default: Length-based (matches db-reduce)
    Similarity,          // Optional: K-mer similarity-based
    Alignment,           // Optional: Full alignment-based
    TaxonomyAware,       // Optional: Consider taxon IDs
}
```

### Performance Tuning

```toml
[performance]
use_kmer_approximation = true
kmer_size = 21
parallel_threads = 8
chunk_size = 10000
```

## Practical Examples

### Example 1: Bacterial Genomes (Default)

For a collection of E. coli genomes using default settings:

```bash
talaria reduce \
    --input ecoli_genomes.fasta \
    --output reduced.fasta \
    --reduction-ratio 0.3
```

To enable similarity-based selection (Optional):

```bash
talaria reduce \
    --input ecoli_genomes.fasta \
    --output reduced.fasta \
    --similarity-threshold 0.95 \
    --min-length 1000000
```

Expected results:
- 5-10% selected as references
- 90-95% delta-encoded
- 10-20x compression

### Example 2: Protein Families

For a protein family database using default settings:

```bash
talaria reduce \
    --input protein_family.fasta \
    --output reduced.fasta \
    --reduction-ratio 0.3
```

To enable advanced features (Optional):

```bash
talaria reduce \
    --input protein_family.fasta \
    --output reduced.fasta \
    --similarity-threshold 0.7 \
    --taxonomy-aware
```

Expected results:
- 15-25% selected as references
- 75-85% delta-encoded
- 3-5x compression

### Example 3: Mixed Database

For a diverse sequence database using default settings:

```bash
talaria reduce \
    --input mixed_db.fasta \
    --output reduced.fasta \
    --reduction-ratio 0.3
```

To enable all optional features:

```bash
talaria reduce \
    --input mixed_db.fasta \
    --output reduced.fasta \
    --similarity-threshold 0.8 \
    --taxonomy-aware \
    --align-select
```

Expected results:
- Variable reference percentage by taxonomy
- Optimized per-group compression
- Overall 5-10x compression

## Advanced Topics

### Adaptive Threshold

Dynamically adjust similarity threshold based on sequence characteristics:

```rust
fn adaptive_threshold(sequence: &Sequence) -> f64 {
    let base_threshold = 0.9;
    let length_factor = (sequence.len() as f64).ln() / 10.0;
    let complexity_factor = calculate_complexity(sequence) / 2.0;
    
    (base_threshold - length_factor + complexity_factor).clamp(0.7, 0.95)
}
```

### Multi-Level References

Use hierarchical reference structure:

```
Level 1: Primary references (full sequences)
Level 2: Secondary references (delta from primary)
Level 3: Tertiary sequences (delta from secondary)
```

### Reference Updates

Incrementally update reference set as new sequences arrive:

```rust
fn update_references(
    current_refs: &mut Vec<Sequence>,
    new_sequences: Vec<Sequence>,
    threshold: f64
) {
    let uncovered = find_uncovered_sequences(&new_sequences, current_refs, threshold);
    
    if uncovered.len() > UPDATE_THRESHOLD {
        let new_refs = select_references_greedy(uncovered, threshold);
        current_refs.extend(new_refs.references);
    }
}
```

## Performance Considerations

### Time Complexity

| Strategy | Time Complexity | Space Complexity |
|----------|----------------|------------------|
| Greedy | O(n²) | O(n) |
| Clustering | O(n² log n) | O(n²) |
| K-mer based | O(n × k) | O(n × k) |
| Incremental | O(n × b) | O(b) |

Where:
- n = number of sequences
- k = k-mer size
- b = buffer size

### Memory Usage

Strategies for reducing memory usage:

1. **Streaming Processing**: Process sequences in chunks
2. **K-mer Sampling**: Use sampled k-mers instead of all
3. **Approximate Similarity**: Use MinHash or similar techniques
4. **External Sorting**: Use disk-based sorting for large datasets

## Best Practices

1. **Choose Appropriate Threshold**
   - Higher threshold (>0.9) for closely related sequences
   - Lower threshold (0.7-0.8) for diverse sequences
   - Consider sequence type (nucleotide vs protein)

2. **Validate Selection Quality**
   - Check coverage metrics
   - Verify compression ratios
   - Test reconstruction accuracy

3. **Monitor Performance**
   - Track selection time
   - Monitor memory usage
   - Profile bottlenecks

4. **Optimize for Use Case**
   - Prioritize speed for real-time applications
   - Prioritize quality for archival storage
   - Balance based on requirements

## See Also

- [Delta Encoding](delta-encoding.md) - How selected references are used
- [Reduction Algorithm](reduction.md) - Overall reduction pipeline
- [Performance Optimization](../advanced/performance.md) - Tuning selection performance