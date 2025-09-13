# Delta Encoding

Delta encoding is a core technique in Talaria for compressing similar sequences by storing only the differences from reference sequences.

## Overview

Instead of storing complete sequences, delta encoding stores:
- A reference sequence in full
- Differences (deltas) from the reference for similar sequences

This approach can achieve significant compression ratios for highly similar sequences, such as those from the same species or protein family.

## Algorithm

### Delta Structure

Each delta-encoded sequence contains:

```rust
struct Delta {
    reference_id: String,      // ID of the reference sequence
    operations: Vec<DeltaOp>,  // List of edit operations
    metadata: DeltaMetadata,   // Original sequence metadata
}

enum DeltaOp {
    Match(usize),              // Match n bases from reference
    Insert(Vec<u8>),           // Insert these bases
    Delete(usize),             // Delete n bases from reference
    Substitute(Vec<u8>),      // Replace with these bases
}
```

### Encoding Process

1. **Alignment**: Align query sequence with reference using Needleman-Wunsch
2. **Operation Generation**: Convert alignment to delta operations
3. **Optimization**: Merge consecutive operations of the same type
4. **Compression**: Apply additional compression to operation stream

### Example

Given:
- Reference: `ATCGATCGATCG`
- Query: `ATCGATGGATCG`

Delta encoding produces:
```
Match(6)        # ATCGAT
Substitute(GG)  # CG -> GG
Match(4)        # ATCG
```

## Compression Efficiency

### Space Complexity

For a sequence of length n with k differences from reference:
- Original: O(n) space
- Delta: O(k) space
- Compression ratio: n/k

### Typical Compression Ratios

| Sequence Similarity | Compression Ratio |
|-------------------|------------------|
| >95% identity     | 10-20x          |
| 90-95% identity   | 5-10x           |
| 80-90% identity   | 2-5x            |
| <80% identity     | <2x (not recommended) |

## Implementation Details

### Encoding Algorithm

```rust
fn encode_delta(reference: &[u8], query: &[u8]) -> Vec<DeltaOp> {
    let alignment = align_sequences(reference, query);
    let mut ops = Vec::new();
    let mut ref_pos = 0;
    let mut query_pos = 0;
    
    for (ref_base, query_base) in alignment {
        match (ref_base, query_base) {
            (Some(r), Some(q)) if r == q => {
                // Match
                ops.push(DeltaOp::Match(1));
                ref_pos += 1;
                query_pos += 1;
            }
            (Some(_), Some(q)) => {
                // Substitution
                ops.push(DeltaOp::Substitute(vec![q]));
                ref_pos += 1;
                query_pos += 1;
            }
            (Some(_), None) => {
                // Deletion
                ops.push(DeltaOp::Delete(1));
                ref_pos += 1;
            }
            (None, Some(q)) => {
                // Insertion
                ops.push(DeltaOp::Insert(vec![q]));
                query_pos += 1;
            }
            _ => unreachable!()
        }
    }
    
    merge_consecutive_ops(ops)
}
```

### Decoding Algorithm

```rust
fn decode_delta(reference: &[u8], delta: &[DeltaOp]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut ref_pos = 0;
    
    for op in delta {
        match op {
            DeltaOp::Match(n) => {
                result.extend_from_slice(&reference[ref_pos..ref_pos + n]);
                ref_pos += n;
            }
            DeltaOp::Insert(bases) => {
                result.extend_from_slice(bases);
            }
            DeltaOp::Delete(n) => {
                ref_pos += n;
            }
            DeltaOp::Substitute(bases) => {
                result.extend_from_slice(bases);
                ref_pos += bases.len();
            }
        }
    }
    
    result
}
```

## Optimization Strategies

### 1. Operation Merging

Consecutive operations of the same type are merged:
```
Match(3) + Match(4) → Match(7)
Insert(A) + Insert(T) → Insert(AT)
```

### 2. Run-Length Encoding

For repetitive operations:
```
Delete(1) × 10 → DeleteRun(1, 10)
```

### 3. Bit-Packed Encoding

Operations are encoded using variable-length integers:
- Small matches (1-127): 1 byte
- Medium matches (128-16383): 2 bytes
- Large matches: 3+ bytes

### 4. Reference Selection

Choosing optimal references is crucial:
- References should be representative of their cluster
- Longer sequences often make better references
- Consider taxonomy when selecting references

## Quality Preservation

### Lossless Encoding

Delta encoding in Talaria is completely lossless:
- Original sequences can be perfectly reconstructed
- All metadata is preserved
- Quality scores (if present) are maintained

### Validation

Each delta-encoded sequence includes:
- Checksum of original sequence
- Length of original sequence
- Number of differences from reference

## Performance Characteristics

### Encoding Performance

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| Alignment | O(n×m)         | O(n×m)          |
| Delta generation | O(n) | O(k)             |
| Optimization | O(k)      | O(k)             |
| Total | O(n×m)          | O(n×m)          |

Where:
- n = reference length
- m = query length
- k = number of differences

### Decoding Performance

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| Delta parsing | O(k)     | O(k)             |
| Reconstruction | O(n)   | O(n)             |
| Total | O(n)            | O(n)             |

## Use Cases

### Ideal Scenarios

1. **Strain Variation**: Multiple strains of the same species
2. **Protein Families**: Homologous proteins with conserved domains
3. **Amplicon Sequencing**: Sequences from the same genomic region
4. **Time Series**: Evolutionary or experimental time series data

### Poor Fit Scenarios

1. **Highly Divergent Sequences**: <70% identity
2. **Random Sequences**: No biological relationship
3. **Short Sequences**: Overhead exceeds benefits for sequences <50bp

## Integration with Aligners

### BLAST Compatibility

Delta-encoded databases can be expanded for BLAST:
```bash
talaria expand -i reduced.fasta -d deltas.tal -o full.fasta
makeblastdb -in full.fasta -dbtype nucl
```

### Direct Delta Support

Some aligners can work directly with delta-encoded databases:
- LAMBDA: Native delta support
- Diamond: Partial delta support via plugins
- MMseqs2: Delta-aware clustering

## File Formats

### Delta File Structure

```
Header:
  Magic: TAL∆
  Version: 1.0
  Reference count: N
  Delta count: M

References:
  [ID, Length, Sequence, Checksum]...

Deltas:
  [RefID, OrigID, OpCount, Operations, Checksum]...
```

### Compression

Additional compression is applied:
- Gzip compression for text formats
- Binary encoding for operations
- Dictionary compression for repeated patterns

## Best Practices

1. **Reference Selection**
   - Use longest sequences as references
   - Ensure references are high quality
   - Distribute references across taxonomic groups

2. **Threshold Selection**
   - Use 90% identity threshold for nucleotides
   - Use 70% identity threshold for proteins
   - Adjust based on sequence diversity

3. **Validation**
   - Always verify reconstruction accuracy
   - Check compression ratios
   - Monitor encoding/decoding performance

4. **Storage**
   - Keep delta files with their references
   - Include metadata for reconstruction
   - Maintain checksums for validation

## See Also

- [Reference Selection](reference-selection.md) - Choosing optimal references
- [Alignment](alignment.md) - Sequence alignment algorithms
- [File Formats](../api/formats.md) - Detailed format specifications