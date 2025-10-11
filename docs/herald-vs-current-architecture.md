# HERALD Architecture vs. Current Implementation Analysis

## Executive Summary

**Critical Finding**: Talaria's current implementation does NOT match the HERALD architecture described in `docs/src/whitepapers/herald-architecture.md`. We are storing sequences twice (full + delta profiles) and missing the core 90% compression benefit.

**Impact**:
- No 90% storage reduction (only deduplicating identicals)
- No cross-database reference sharing
- Separate "reduce" operation duplicates storage
- Not helping users as claimed in paper

## The Fundamental Mismatch

### What HERALD Paper Claims (herald-architecture.md:7-9)

> "By identifying similar sequences and encoding children as deltas from reference sequences, HERALD creates compressed indices containing only the 10% unique references while maintaining full search sensitivity through on-demand reconstruction."

**Key promises:**
- 40-90% storage reduction through reference-based compression
- References shared across all databases
- Aligner indices 10x smaller (built from references only)
- On-demand reconstruction maintains full sensitivity

### What Current Code Actually Does

```
┌─────────────────────────────────────────────────────────────┐
│ Current "Database Add/Download" Flow                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Input FASTA                                                 │
│       ↓                                                      │
│  Parse sequences                                             │
│       ↓                                                      │
│  Compute SHA256(sequence)                                    │
│       ↓                                                      │
│  Check if hash exists in SEQUENCES CF                        │
│       ↓                                                      │
│  If exists: Add representation (header) only                 │
│  If new:    Store FULL sequence in SEQUENCES CF              │
│       ↓                                                      │
│  Create manifest with chunk hashes                           │
│                                                              │
│  Result: Every unique sequence stored in FULL                │
│                                                              │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Current "Reduce" Flow (SEPARATE OPERATION)                   │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Load sequences from SEQUENCES CF                            │
│       ↓                                                      │
│  Run LAMBDA aligner (all-vs-all or taxonomy-aware)           │
│       ↓                                                      │
│  Select references (taxonomy-based clustering)               │
│       ↓                                                      │
│  Encode non-references as deltas                             │
│       ↓                                                      │
│  Store in SEPARATE "profile" structure                       │
│       ↓                                                      │
│  Output: references.fasta + delta metadata                   │
│                                                              │
│  Result: BOTH full sequences AND delta profiles exist!       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**The Duplication Problem:**
- SwissProt imported → 571K sequences stored in full (1.2 GB)
- Run reduce → Creates profile with references + deltas (stored separately)
- **Total storage**: Original 1.2 GB + profile data
- **Expected HERALD**: ~120 MB references + 1.08 GB deltas = same 1.2 GB total, but deltas compressed

## What We Actually Have: Option A (Content-Addressed Storage)

### Current Architecture is "Git for FASTA"

**What it does well:**
- Deduplicates IDENTICAL sequences across all databases
- Content addressing ensures same sequence = same hash
- Merkle DAG for version tracking
- Bi-temporal queries (sequence time vs taxonomy time)
- Reproducibility through cryptographic verification

**Storage characteristics:**
```
SwissProt:  571,609 unique sequences → Store all 571,609 in full
UniRef50:   70,408,371 unique sequences → Store all 70M in full
Sharing:    Only if byte-for-byte identical (rare between different databases)

Example:
  SwissProt: MVALPRWFDK
  UniRef50:  MVALPRWFDKEXTRA (longest in cluster, different sequence)

  SHA256(SwissProt) ≠ SHA256(UniRef50)

  Result: Both stored separately, 0% sharing
```

**Honest capabilities:**
- ✅ Eliminates duplicate storage of IDENTICAL sequences
- ✅ Cryptographic verification of database states
- ✅ Incremental sync (only download changed chunks)
- ✅ Bi-temporal version tracking
- ❌ NO compression of similar sequences
- ❌ NO 90% storage reduction
- ❌ NO cross-database reference sharing
- ❌ NO aligner index optimization

**Reduce is separate tool:**
- Used for creating alignment profiles
- Not integrated into storage layer
- Creates duplicate data structures
- References + deltas stored separately from canonical sequences

## What HERALD Should Be: Option B (Reference-Based Compression)

### True HERALD Architecture

**Import process should do:**
```
┌─────────────────────────────────────────────────────────────┐
│ HERALD Import Flow (SHOULD BE)                               │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Input sequence                                              │
│       ↓                                                      │
│  Compute SHA256(sequence)                                    │
│       ↓                                                      │
│  Check if exists in SEQUENCES or DELTAS CF                   │
│       ↓                                                      │
│  If exists: Add representation (header) only                 │
│       ↓                                                      │
│  If new: Find similar sequences (k-mer index, 90% identity)  │
│       ↓                                                      │
│  ┌─────────────────────────────────────────┐                │
│  │ Found similar reference?                │                │
│  │                                         │                │
│  │ YES:                        NO:         │                │
│  │   Encode as delta           Store as    │                │
│  │   Store in DELTAS CF        reference   │                │
│  │   Link to reference         SEQUENCES   │                │
│  │   Add representation        Add repr    │                │
│  └─────────────────────────────────────────┘                │
│       ↓                                                      │
│  Create manifest (references + delta counts)                 │
│                                                              │
│  Result: ~10% stored as references, ~90% as deltas           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

**Storage layout:**
```rust
enum SequenceStorage {
    Reference {
        hash: SHA256Hash,
        sequence: Vec<u8>,
        representations: Vec<SequenceRepresentation>,
        children: Vec<SHA256Hash>, // Sequences that delta from this
    },
    Delta {
        hash: SHA256Hash,
        reference_hash: SHA256Hash,
        operations: Vec<DeltaOperation>,
        representations: Vec<SequenceRepresentation>,
    }
}

// Storage in RocksDB:
// SEQUENCES CF:  hash -> Reference data (10% of sequences)
// DELTAS CF:     hash -> Delta data (90% of sequences)
// CHILDREN_IDX:  ref_hash -> Vec<child_hash> (for expansion)
```

**Query interface (transparent reconstruction):**
```rust
// User doesn't know if sequence is reference or delta
fn get_sequence(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
    // Try references first
    if let Some(seq) = self.backend.get_sequence(hash)? {
        return Ok(seq.sequence);
    }

    // Check deltas
    if let Some(delta) = self.backend.get_delta(hash)? {
        let reference = self.get_sequence(&delta.reference_hash)?;
        return Ok(apply_delta(&reference, &delta.operations));
    }

    Err(anyhow!("Sequence not found"))
}

// Batch reconstruction for alignment
fn get_reference_family(&self, ref_hash: &SHA256Hash) -> Result<Vec<Vec<u8>>> {
    let reference = self.get_sequence(ref_hash)?;
    let children = self.backend.get_children(ref_hash)?;

    let mut family = vec![reference.clone()];
    for child_hash in children {
        family.push(self.get_sequence(&child_hash)?);
    }
    Ok(family)
}
```

**Expected results:**
```
Database: SwissProt (571K sequences)
├─ References stored: ~57K sequences (10%) = ~120 MB
├─ Deltas stored:     ~514K deltas (90%) = ~50-100 MB compressed
└─ Total:             ~170-220 MB (vs 1.2 GB current)

Compression: 82-85% reduction

Cross-database sharing:
├─ UniRef100 imports
├─ Finds SwissProt sequences already as references
├─ Links to existing references instead of duplicating
└─ True deduplication of SIMILAR sequences, not just IDENTICAL
```

## Why "Reduce" Exists: Code Smell

### Option A Perspective (Current)
- `reduce` is a tool for creating alignment profiles
- Separate from storage layer
- Makes sense: storage is content-addressed, reduce is for aligners
- **But**: Creates duplicate data (sequences stored twice)

### Option B Perspective (HERALD)
- `reduce` shouldn't exist as separate command
- Compression happens at import time
- "Profile" is just the reference set already in storage
- **Refactor**: `reduce` becomes `export-alignment-index`
  - Doesn't create new data structures
  - Just exports references from storage
  - Aligner indices built from SEQUENCES CF directly

## Why You're Seeing 0% Shared Sequences

### Current Implementation (Content-Addressed Only)

```
SwissProt sequence:   MVALPRWFDK
  ↓
  SHA256: a3f2b8c9... (hash of full sequence)
  ↓
  Stored in SEQUENCES CF

UniRef50 cluster representative: MVALPRWFDKEXTRASTUFF (longest in cluster)
  ↓
  SHA256: 7d4e1a5c... (different hash!)
  ↓
  Stored in SEQUENCES CF separately

Result: 0% sharing (different byte content = different hashes)
```

### HERALD Implementation (Reference-Based)

```
SwissProt sequence:   MVALPRWFDK
  ↓
  SHA256: a3f2b8c9...
  ↓
  No similar sequence exists → Store as REFERENCE

UniRef50 imports (includes same sequence in cluster):
  ↓
  Sequence: MVALPRWFDK (exact match)
  ↓
  SHA256: a3f2b8c9... (same hash!)
  ↓
  Already exists as reference → Add representation only
  ↓
  SHARED! (same reference)

UniRef50 cluster representative: MVALPRWFDKEXTRASTUFF
  ↓
  Find similar: 90% match with MVALPRWFDK (reference)
  ↓
  Encode as delta: [APPEND "EXTRASTUFF" at position 10]
  ↓
  Store delta linking to SwissProt reference
  ↓
  SHARED! (uses same reference)
```

**However**: UniRef clustering picks LONGEST as representative, so in reality:
- UniRef50 would make `MVALPRWFDKEXTRASTUFF` the reference
- SwissProt `MVALPRWFDK` might be stored as delta from UniRef50
- Or might be separate reference if imported first
- Order matters unless we implement smart global reference selection

## Database Diff Implementation Issues

### Current diff problems:

1. **Compares chunk hashes, not sequence hashes**
   ```rust
   // From database_diff.rs:546-577
   fn compare_sequences_from_manifests(...) -> SequenceAnalysis {
       // Gets CHUNK hashes
       let chunks_a: HashSet<_> = manifest_a.chunk_index.iter()
           .map(|m| m.hash.clone()).collect();

       // Counts sequences in shared CHUNKS
       let shared_seq_count: usize = manifest_a.chunk_index.iter()
           .filter(|m| shared_chunk_hashes.contains(&m.hash))
           .map(|m| m.sequence_count).sum();
   }
   ```

   **Problem**: Shared chunks ≠ shared sequences!
   - Chunks are taxonomically grouped
   - Different databases have different taxonomic boundaries
   - Same sequences, different chunks → 0% sharing reported

2. **Doesn't load actual sequence hashes**
   - `ChunkManifest` contains `sequence_refs: Vec<SHA256Hash>`
   - But diff doesn't load these from RocksDB
   - Just compares chunk-level metadata

3. **No visualization of reference relationships**
   - Can't see which sequences are references
   - Can't see delta relationships
   - Can't see reference sharing across databases

### What diff SHOULD do:

```rust
struct ProperSequenceAnalysis {
    // Individual sequence hashes
    sequences_a: HashSet<SHA256Hash>,
    sequences_b: HashSet<SHA256Hash>,

    // Shared at sequence level
    shared_sequences: Vec<SHA256Hash>,
    shared_percentage: f64,

    // Reference vs delta breakdown
    references_a: usize,
    deltas_a: usize,
    references_b: usize,
    deltas_b: usize,

    // Cross-database reference sharing
    shared_references: Vec<SHA256Hash>,
    shared_reference_percentage: f64,

    // Sample sequences for display
    sample_shared: Vec<SequenceInfo>,
    sample_unique_a: Vec<SequenceInfo>,
    sample_unique_b: Vec<SequenceInfo>,
}

fn compare_sequences_properly(
    manifest_a: &TemporalManifest,
    manifest_b: &TemporalManifest,
    storage: &SequenceStorage,
) -> Result<ProperSequenceAnalysis> {
    // Load actual sequence hashes from chunk manifests
    let mut seqs_a = HashSet::new();
    for chunk_metadata in &manifest_a.chunk_index {
        // Load ChunkManifest from RocksDB
        let chunk = storage.get_chunk(&chunk_metadata.hash)?;
        let manifest: ChunkManifest = bincode::deserialize(&chunk)?;

        // Extract actual sequence hashes
        seqs_a.extend(manifest.sequence_refs);
    }

    // Same for b
    let mut seqs_b = HashSet::new();
    for chunk_metadata in &manifest_b.chunk_index {
        let chunk = storage.get_chunk(&chunk_metadata.hash)?;
        let manifest: ChunkManifest = bincode::deserialize(&chunk)?;
        seqs_b.extend(manifest.sequence_refs);
    }

    // Now compare actual sequence hashes
    let shared = seqs_a.intersection(&seqs_b).cloned().collect();

    // Classify as references vs deltas
    let (refs_a, deltas_a) = classify_sequences(&seqs_a, storage)?;
    let (refs_b, deltas_b) = classify_sequences(&seqs_b, storage)?;

    // Find shared references specifically
    let shared_refs = refs_a.intersection(&refs_b).cloned().collect();

    // Return detailed analysis
    Ok(ProperSequenceAnalysis {
        sequences_a: seqs_a,
        sequences_b: seqs_b,
        shared_sequences: shared,
        // ... etc
    })
}
```

### Visualization improvements needed:

1. **Sequence-level comparison**:
   ```
   Database Comparison: uniprot/swissprot vs uniprot/uniref50

   Sequences:
     SwissProt:  571,609 sequences
     UniRef50:   70,408,371 sequences

     Shared:     45,231 sequences (7.9% of SwissProt, 0.06% of UniRef50)
     Unique to SwissProt: 526,378 sequences
     Unique to UniRef50:  70,363,140 sequences
   ```

2. **Reference/Delta breakdown** (if HERALD implemented):
   ```
   Storage Analysis:
     SwissProt:
       References: 57,160 (10%)
       Deltas:     514,449 (90%)

     UniRef50:
       References: 7,040,837 (10%)
       Deltas:     63,367,534 (90%)

     Shared References: 12,450 (21.8% of SwissProt refs)
       → These sequences serve as references in BOTH databases
       → Actual storage savings from sharing
   ```

3. **Visual reference tree** (for small datasets):
   ```
   Reference Families (sample):

   REF: sha256:a3f2b8c9 (Human insulin)
     ├─ Databases: SwissProt, UniRef100
     ├─ Children: 145 sequences
     │   ├─ sha256:1a2b3c4d (Mouse insulin) - 2 substitutions
     │   ├─ sha256:5e6f7g8h (Rat insulin) - 3 substitutions
     │   └─ ... 142 more
     └─ Total family size: 146 sequences

   REF: sha256:7d4e1a5c (E. coli RecA)
     ├─ Databases: UniRef90, NCBI nr
     ├─ Children: 8,234 sequences
     └─ Total family size: 8,235 sequences
   ```

4. **Compression effectiveness**:
   ```
   Storage Efficiency:
     Traditional (if stored separately):
       SwissProt: 1.2 GB
       UniRef50:  280 GB
       Total:     281.2 GB

     Current (content-addressed):
       Shared identical: 45 MB (45,231 sequences)
       Unique storage:   281.155 GB
       Savings:          0.016% (negligible)

     HERALD (if implemented):
       References:       28 GB (10% of unique sequences)
       Deltas:           42 GB (90% compressed 85%)
       Savings:          75% (211 GB saved)
   ```

## Migration Path to True HERALD

### Phase 1: Extend storage layer (1-2 weeks)

1. **Add delta storage type**:
   ```rust
   // talaria-storage/src/types.rs

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub enum StoredSequence {
       Reference(CanonicalSequence),
       Delta(DeltaSequence),
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct DeltaSequence {
       pub hash: SHA256Hash,
       pub reference_hash: SHA256Hash,
       pub operations: Vec<DeltaOperation>,
       pub compressed_size: usize,
   }
   ```

2. **Implement similarity detection**:
   ```rust
   // talaria-sequoia/src/storage/similarity.rs

   pub struct SimilarityIndex {
       kmer_index: HashMap<Kmer, Vec<SHA256Hash>>,
       threshold: f64,
   }

   impl SimilarityIndex {
       pub fn find_similar(&self, sequence: &[u8]) -> Vec<(SHA256Hash, f64)> {
           // k-mer based similarity search
           // Return candidates with similarity scores
       }

       pub fn select_best_reference(&self, candidates: Vec<(SHA256Hash, f64)>)
           -> Option<SHA256Hash> {
           // Choose reference that minimizes delta size
       }
   }
   ```

3. **Update store_sequence**:
   ```rust
   // talaria-sequoia/src/storage/sequence.rs

   pub fn store_sequence(
       &self,
       sequence: &str,
       header: &str,
       source: DatabaseSource,
   ) -> Result<SHA256Hash> {
       let hash = SHA256Hash::compute(sequence.as_bytes());

       // Check if already exists
       if self.sequence_exists(&hash)? {
           self.add_representation(&hash, header, source)?;
           return Ok(hash);
       }

       // Find similar sequences
       let similar = self.similarity_index.find_similar(sequence.as_bytes());

       if let Some(ref_hash) = self.similarity_index.select_best_reference(similar) {
           // Store as delta
           let delta = encode_delta(sequence, &self.get_sequence(&ref_hash)?);

           if delta.size_ratio() < 0.2 {  // Delta is <20% of original
               self.store_delta(hash, ref_hash, delta)?;
               self.add_representation(&hash, header, source)?;
               return Ok(hash);
           }
       }

       // Store as reference
       self.store_reference(hash, sequence)?;
       self.add_representation(&hash, header, source)?;
       self.similarity_index.add_sequence(hash, sequence);

       Ok(hash)
   }
   ```

### Phase 2: Transparent reconstruction (1 week)

1. **Update query interface**:
   ```rust
   pub fn get_sequence(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
       if let Some(seq) = self.get_reference(hash)? {
           return Ok(seq);
       }

       if let Some(delta) = self.get_delta(hash)? {
           let reference = self.get_sequence(&delta.reference_hash)?;
           return Ok(apply_delta(&reference, &delta.operations));
       }

       Err(anyhow!("Sequence not found"))
   }
   ```

2. **Add reconstruction cache**:
   ```rust
   pub struct SequenceCache {
       cache: LruCache<SHA256Hash, Vec<u8>>,
   }
   ```

### Phase 3: Refactor reduce command (1 week)

1. **Change from storage to export**:
   ```rust
   // OLD: reduce creates and stores references + deltas
   // NEW: reduce exports existing references for alignment

   pub fn export_alignment_index(
       database: &str,
       aligner: AlignerType,
   ) -> Result<PathBuf> {
       let manifest = manager.get_manifest(database)?;

       // Get references directly from storage
       let references = storage.get_references_for_chunks(&manifest.chunk_index)?;

       // Export to aligner format
       write_aligner_index(aligner, references)?;

       Ok(index_path)
   }
   ```

### Phase 4: Improve diff visualization (1 week)

1. **Load actual sequence hashes**
2. **Classify references vs deltas**
3. **Compute sharing at sequence level**
4. **Add visual outputs** (tree, graphs, statistics)

## Testing Strategy

### Validation approach:

1. **Import SwissProt with HERALD**:
   - Should identify ~57K references (10%)
   - Should encode ~514K as deltas (90%)
   - Measure storage: expect ~170-220 MB vs 1.2 GB

2. **Import UniRef50 with HERALD**:
   - Should reuse some SwissProt references
   - Measure shared references
   - Verify cross-database deduplication

3. **Run proper diff**:
   - Should show >0% sequence sharing
   - Should show reference sharing
   - Should visualize relationships

4. **Benchmark reconstruction**:
   - Measure delta decode time
   - Verify sequence correctness
   - Test batch reconstruction

5. **Alignment validation**:
   - Build index from references only
   - Compare results to full database
   - Verify sensitivity maintained

## Decision Required

**Choose one path:**

### Path A: Keep current architecture
- Rename paper claims to match reality
- "Content-addressed storage with deduplication of identical sequences"
- "Reduce" is separate alignment optimization tool
- Simpler, proven design
- Lower risk

### Path B: Implement true HERALD
- Refactor storage layer for reference-based compression
- Achieve 90% storage reduction as promised
- Cross-database reference sharing
- Higher complexity, higher reward
- **Recommended** - this is the innovation

## Recommended Action

**Implement Path B (True HERALD)** because:

1. You wrote the paper - architecture is designed
2. Code is 50% there (delta encoding, chunking exist)
3. This is the real innovation (not just "Git for FASTA")
4. Delivers promised 90% compression
5. Solves the duplication problem (no separate reduce storage)
6. Enables true cross-database optimization

**Then fix diff to visualize properly** - load actual sequence hashes and show reference relationships.

---

*Document created: 2025-10-06*
*Status: Architecture analysis and migration plan*
*Next step: Decision on Path A vs Path B*
