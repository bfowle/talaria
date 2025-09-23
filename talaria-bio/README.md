# talaria-bio

Bioinformatics library for sequence manipulation and analysis.

## Overview

This crate provides core bioinformatics functionality:

- **Sequence Handling**: Efficient sequence representation and manipulation
- **FASTA I/O**: High-performance FASTA parsing and writing
- **Alignment**: Needleman-Wunsch and other alignment algorithms
- **Taxonomy**: Taxonomic classification and management
- **Statistics**: Sequence composition and quality metrics

## Features

### Sequence Manipulation
```rust
use talaria_bio::{Sequence, SequenceType};

let seq = Sequence::new("seq1".to_string(), b"MVALPRWFDK".to_vec());
assert_eq!(seq.detect_type(), SequenceType::Protein);

// With taxonomy
let mut seq = Sequence::new("protein".to_string(), b"MVAL".to_vec());
seq.taxon_id = Some(9606); // Human
```

### FASTA I/O
```rust
use talaria_bio::{FastaReader, FastaWriter};

// Read FASTA (supports gzip)
let reader = FastaReader::new("sequences.fasta")?;
let sequences = reader.read_all()?;

// Write FASTA
let writer = FastaWriter::new("output.fasta")?;
writer.write_sequences(&sequences)?;
```

### Alignment
```rust
use talaria_bio::alignment::{NeedlemanWunsch, ScoringMatrix};

let aligner = NeedlemanWunsch::new(ScoringMatrix::blosum62());
let alignment = aligner.align(seq1, seq2);
println!("Identity: {:.2}%", alignment.identity * 100.0);
```

### Taxonomy Management
```rust
use talaria_bio::{TaxonomyManager, TaxonId};

let manager = TaxonomyManager::new(path)?;
let ancestors = manager.get_ancestors(TaxonId(9606))?; // Human ancestors
let lca = manager.find_lca(vec![TaxonId(9606), TaxonId(10090)])?; // Human-Mouse LCA
```

## Key Types

- `Sequence`: Core sequence representation with metadata
- `SequenceType`: Protein or Nucleotide classification
- `TaxonId`: NCBI taxonomy identifier wrapper
- `TaxonomyNode`: Taxonomic hierarchy node
- `AlignmentResult`: Alignment scores and operations

## Performance

- Memory-mapped FASTA reading for large files
- Parallel sequence processing with rayon
- Optimized alignment algorithms
- Efficient taxonomy lookups with caching

## Usage

Add to your `Cargo.toml`:
```toml
[dependencies]
talaria-bio = { path = "../talaria-bio" }
```

## License

MIT