# How SEQUOIA Works

This guide walks through exactly how SEQUOIA operates, step by step, with visual examples.

## The Complete SEQUOIA Workflow

```mermaid
graph TD
    Start[New Database Version Available]
    Check[Check Manifest]
    Compare[Compare with Local]
    Download[Download Only Changes]
    Store[Store in Chunks]
    Verify[Verify Integrity]
    Ready[Database Ready]

    Start --> Check
    Check --> Compare
    Compare --> Download
    Download --> Store
    Store --> Verify
    Verify --> Ready

    style Start stroke:#ff6b6b,stroke-width:2px
    style Ready stroke:#51cf66,stroke-width:2px
```

## Step-by-Step Breakdown

### Step 1: Initial Database Download

When you first download a database, here's what happens behind the scenes:

```mermaid
sequenceDiagram
    participant User
    participant Talaria
    participant Remote
    participant SEQUOIA
    participant Disk

    User->>Talaria: talaria database download uniprot/swissprot
    Talaria->>Remote: Request manifest
    Remote-->>Talaria: Return manifest (1 KB)
    Talaria->>Talaria: Parse manifest

    loop For each chunk in manifest
        Talaria->>SEQUOIA: Check if chunk exists locally
        SEQUOIA-->>Talaria: Not found
        Talaria->>Remote: Download chunk
        Remote-->>Talaria: Stream chunk data
        Talaria->>SEQUOIA: Store chunk with hash
        SEQUOIA->>Disk: Write chunk file
    end

    Talaria-->>User: Download complete!
```

**What's Really Happening:**

1. **Manifest First**: Always download the tiny manifest (1-10 KB) first
2. **Smart Checking**: Check which chunks you already have
3. **Parallel Downloads**: Download multiple chunks simultaneously
4. **Verification**: Each chunk is verified against its hash
5. **Storage**: Chunks stored by their content hash

### Step 2: Checking for Updates

The magic happens when checking for updates:

```bash
talaria database update uniprot/swissprot
```

```mermaid
graph LR
    subgraph Local
        LM[Local Manifest<br/>v2024-03-01]
        LC1[Chunk abc123]
        LC2[Chunk def456]
        LC3[Chunk ghi789]
    end

    subgraph Remote
        RM[Remote Manifest<br/>v2024-03-15]
        RC1[Chunk abc123]
        RC2[Chunk def456]
        RC3[Chunk xyz999]
    end

    LM -.->|Compare| RM
    LC1 -->|Match ✓| RC1
    LC2 -->|Match ✓| RC2
    LC3 -->|Different ✗| RC3

    style LC1 stroke:#51cf66,stroke-width:2px
    style LC2 stroke:#51cf66,stroke-width:2px
    style LC3 stroke:#ff6b6b,stroke-width:2px
    style RC3 stroke:#ffd43b,stroke-width:2px
```

**Result**: Only download chunk xyz999 (the new one)!

### Step 3: Chunking Process

How does SEQUOIA decide what goes in each chunk?

```mermaid
graph TD
    DB[Complete Database]
    Tax[Group by Taxonomy]

    subgraph Taxonomic Groups
        Human[Human Proteins<br/>50,000 sequences]
        Mouse[Mouse Proteins<br/>45,000 sequences]
        Ecoli[E.coli Proteins<br/>4,500 sequences]
        Other[Other Species<br/>...]
    end

    subgraph Smart Chunks
        CH1[Chunk: Human-1<br/>200 MB]
        CH2[Chunk: Human-2<br/>200 MB]
        CM1[Chunk: Mouse-1<br/>180 MB]
        CE1[Chunk: Ecoli-All<br/>45 MB]
    end

    DB --> Tax
    Tax --> Human
    Tax --> Mouse
    Tax --> Ecoli
    Tax --> Other

    Human --> CH1
    Human --> CH2
    Mouse --> CM1
    Ecoli --> CE1

    style Tax stroke:#4ecdc4,stroke-width:2px
    style CH1 stroke:#51cf66,stroke-width:2px
    style CH2 stroke:#51cf66,stroke-width:2px
    style CM1 stroke:#51cf66,stroke-width:2px
    style CE1 stroke:#51cf66,stroke-width:2px
```

**Why Taxonomic Chunking?**
- Similar organisms have similar proteins
- Better compression ratios
- Researchers often query specific species
- Updates often affect specific taxonomic groups

### Step 4: Delta Compression in Action

For similar sequences, SEQUOIA uses delta compression:

```mermaid
graph TD
    subgraph Input Sequences
        S1[Sequence 1: MKTAYIAKQRQ...]
        S2[Sequence 2: MKTAYIAKQEQ...]
        S3[Sequence 3: MKTAYIAKQRQ...]
    end

    subgraph Processing
        Ref[Reference Selection]
        Delta[Delta Computation]
    end

    subgraph Storage
        R[Reference: MKTAYIAKQRQ...]
        D1[Delta: pos 10 R→E]
        D2[Delta: identical]
    end

    S1 --> Ref
    S2 --> Ref
    S3 --> Ref

    Ref --> R
    Ref --> Delta
    Delta --> D1
    Delta --> D2

    style R stroke:#51cf66,stroke-width:3px
    style D1 stroke:#4ecdc4,stroke-width:2px
    style D2 stroke:#4ecdc4,stroke-width:2px
```

**Storage Savings**: 3 sequences → 1 reference + 2 tiny deltas

### Step 5: Verification with Merkle Trees

How SEQUOIA ensures data integrity:

```mermaid
graph TD
    subgraph Verification Process
        Root[Root Hash: 5a9b3c...]

        Branch1[Branch 1: 8f2d1a...]
        Branch2[Branch 2: 3c9e7b...]

        C1[Chunk 1: abc123...]
        C2[Chunk 2: def456...]
        C3[Chunk 3: ghi789...]
        C4[Chunk 4: jkl012...]
    end

    Root --> Branch1
    Root --> Branch2
    Branch1 --> C1
    Branch1 --> C2
    Branch2 --> C3
    Branch2 --> C4

    subgraph Verify Chunk 3
        V1[1. Hash Chunk 3 → ghi789...]
        V2[2. Hash with C4 → 3c9e7b...]
        V3[3. Hash with Branch1 → 5a9b3c...]
        V4[4. Compare with Root ✓]
    end

    C3 -.->|Verify| V1
    V1 --> V2
    V2 --> V3
    V3 --> V4

    style Root stroke:#ff6b6b,stroke-width:3px
    style V4 stroke:#51cf66,stroke-width:2px
```

## Real-World Example: Daily UniProt Update

Let's walk through an actual update scenario:

### Day 1: Initial Download
```bash
$ talaria database download uniprot/swissprot
```
- Downloads 571,282 sequences
- Creates 127 chunks (grouped by taxonomy)
- Total size: 204 MB compressed
- Time: ~5 minutes

### Day 7: Weekly Update
```bash
$ talaria database update uniprot/swissprot
```

What happens:
1. **Check Manifest** (0.1 seconds)
   - Remote: 571,419 sequences
   - Local: 571,282 sequences
   - Difference: 137 new, 12 updated

2. **Compare Chunks** (0.2 seconds)
   - 124 chunks unchanged ✓
   - 3 chunks modified ✗

3. **Download Changes** (3 seconds)
   - Only 3 chunks needed
   - ~2.4 MB download (not 204 MB!)

4. **Update Complete** (5 seconds total)
   - 99% bandwidth saved
   - Perfect integrity verified

## Performance Comparison

```mermaid
graph LR
    subgraph Traditional
        T1[Download: 204 MB]
        T2[Every Update: 204 MB]
        T3[Bandwidth/Month: 6.1 GB]
    end

    subgraph With SEQUOIA
        C1[Initial: 204 MB]
        C2[Updates: ~2 MB each]
        C3[Bandwidth/Month: 264 MB]
    end

    style T3 stroke:#ff6b6b,stroke-width:2px
    style C3 stroke:#51cf66,stroke-width:2px
```

**Savings**: 96% reduction in bandwidth!

## Under the Hood: File Structure

Here's how SEQUOIA organizes files on disk:

```
~/.talaria/
├── databases/
│   ├── manifests/
│   │   ├── uniprot_swissprot_2024-03-15.manifest
│   │   └── ncbi_nr_2024-03-14.manifest
│   └── chunks/
│       ├── ab/
│       │   └── abc123def456.chunk  # Human proteins
│       ├── de/
│       │   └── def789ghi012.chunk  # Mouse proteins
│       └── ... (more chunks)
└── cache/
    └── indices/  # Optional index cache
```

Each chunk is stored in RocksDB column family with hash-based keys for efficient access.

## Summary

SEQUOIA works by:

1. **Breaking databases into smart chunks** based on biological relationships
2. **Identifying each chunk uniquely** with cryptographic hashes
3. **Downloading only what changed** during updates
4. **Verifying everything** with Merkle tree proofs
5. **Storing efficiently** with delta compression

The result? Faster updates, less storage, perfect verification, and better science.

Ready to try it yourself? Continue to [Getting Started](./getting-started.md)