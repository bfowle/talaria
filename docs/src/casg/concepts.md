# Core CASG Concepts

This guide explains the fundamental concepts behind CASG in plain language. No PhD required!

## Content Addressing

### Traditional Approach: Names Point to Data
```
database_v1.fasta → [data that can change]
database_v2.fasta → [completely different data]
```

Problems:
- Same name might have different content
- No way to verify if data is correct
- Must trust the source completely

### CASG Approach: Content Defines the Name
```
SHA256(data) = abc123... → [data that never changes]
```

Benefits:
- Content creates its own unique ID
- Any change creates a new ID
- Can verify data independently
- Perfect deduplication

**Simple Analogy**: It's like using fingerprints instead of names. A fingerprint uniquely identifies a person and can't be faked.

## Chunks

Instead of treating a database as one giant file, CASG breaks it into manageable pieces called chunks.

### What's in a Chunk?
- Related sequences (often from the same organism or family)
- Typically 50-500 MB in size
- Each has its own unique hash ID

### Why Chunks?
- **Efficient Updates**: Only download changed chunks
- **Parallel Processing**: Work on multiple chunks simultaneously
- **Better Caching**: Keep frequently used chunks in memory
- **Fault Tolerance**: One corrupted chunk doesn't affect others

```mermaid
graph LR
    DB[Giant Database<br/>500 GB] --> C1[Chunk 1<br/>Human<br/>100 MB]
    DB --> C2[Chunk 2<br/>Mouse<br/>150 MB]
    DB --> C3[Chunk 3<br/>E.coli<br/>50 MB]
    DB --> C4[... more chunks]

    style DB stroke:#ff6b6b,stroke-width:2px
    style C1 stroke:#4ecdc4,stroke-width:2px
    style C2 stroke:#4ecdc4,stroke-width:2px
    style C3 stroke:#4ecdc4,stroke-width:2px
```

## Manifests

A manifest is like a recipe or blueprint that tells CASG how to reconstruct a complete database from chunks.

### What's in a Manifest?
```yaml
database: uniprot_swissprot
version: 2024-03-15
total_sequences: 571282
chunks:
  - hash: abc123def456...
    taxon: 9606  # Human
    size: 104857600
  - hash: 789ghi012jkl...
    taxon: 10090  # Mouse
    size: 157286400
```

### Manifest Benefits
- **Version Tracking**: Know exactly what's in each version
- **Quick Updates**: Compare manifests to find changes
- **Verification**: Confirm all chunks are present and correct
- **Reproducibility**: Recreate exact database state anytime

## Merkle Trees

Merkle trees provide cryptographic proof that data is correct without checking every single piece.

### How It Works

```mermaid
graph TD
    Root[Root Hash<br/>Proves Everything]
    L1[Left Branch<br/>Hash]
    L2[Right Branch<br/>Hash]
    C1[Chunk 1]
    C2[Chunk 2]
    C3[Chunk 3]
    C4[Chunk 4]

    Root --> L1
    Root --> L2
    L1 --> C1
    L1 --> C2
    L2 --> C3
    L2 --> C4

    style Root stroke:#ff6b6b,stroke-width:3px
    style L1 stroke:#4ecdc4,stroke-width:2px
    style L2 stroke:#4ecdc4,stroke-width:2px
```

**Benefits**:
- Verify any chunk belongs to the database
- Detect tampering immediately
- Prove database integrity with just the root hash

## Bi-Temporal Versioning

Biological databases change in two independent ways:

### 1. Sequence Time
When new sequences are added or existing ones updated:
- New protein discovered
- Sequence correction
- Additional annotations

### 2. Taxonomy Time
When our understanding of relationships changes:
- Species reclassification
- New evolutionary insights
- Taxonomic corrections

### Why It Matters
```mermaid
graph LR
    S1[Sequences<br/>Jan 2024] --> S2[Sequences<br/>Feb 2024]
    T1[Taxonomy<br/>v2024.1] --> T2[Taxonomy<br/>v2024.2]

    S1 -.-> T1
    S1 -.-> T2
    S2 -.-> T1
    S2 -.-> T2

    style S1 stroke:#4ecdc4,stroke-width:2px
    style S2 stroke:#4ecdc4,stroke-width:2px
    style T1 stroke:#95e1d3,stroke-width:2px
    style T2 stroke:#95e1d3,stroke-width:2px
```

You can:
- Use January sequences with February taxonomy
- Apply current taxonomy to historical sequences
- Track how classifications changed over time

## Delta Compression

Instead of storing similar sequences multiple times, CASG stores one reference and the differences (deltas) for similar sequences.

### Example
```
Reference: MKTAYIAKQRQISFVKSHFSRQ...  (Human insulin)
Delta 1:   ----------E---------...     (Mouse: position 11 K→E)
Delta 2:   ---S----------------...     (Rat: position 4 T→S)
```

**Storage Savings**:
- Full storage: 3 complete sequences
- Delta storage: 1 sequence + 2 small changes
- Savings: ~70% for similar sequences

## Summary

These concepts work together to create a storage system that's:
- **Efficient**: Minimal storage and bandwidth usage
- **Verifiable**: Cryptographic proof of correctness
- **Flexible**: Handle updates and versions elegantly
- **Scientific**: Designed for biological data patterns

Ready to see these concepts in action? Continue to [How CASG Works](./how-it-works.md)