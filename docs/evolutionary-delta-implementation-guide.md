# Evolutionary Delta Compression: Step-by-Step Implementation Guide

> **Purpose**: Practical roadmap for implementing the evolutionary delta compression described in `evolutionary-compression-research.md` Section 3.4

**Status**: This document provides actionable steps to move from research concept to working implementation.

---

## Part 1: Current State Analysis

### What Currently Exists ✅

**1. Delta Encoding Infrastructure** (`talaria-herald/src/delta/canonical.rs`)
```rust
// Myers diff algorithm - FULLY IMPLEMENTED
pub struct MyersDeltaCompressor {
    max_distance: usize,
    use_banded: bool,
}

impl DeltaCompressor for MyersDeltaCompressor {
    fn compute_delta(&self, reference: &[u8], target: &[u8]) -> Result<Delta>;
    fn apply_delta(&self, reference: &[u8], delta: &Delta) -> Result<Vec<u8>>;
    fn estimate_ratio(&self, reference: &[u8], target: &[u8]) -> f32;
}

// Delta operations: Copy, Insert, Skip - WORKS
pub enum DeltaOp { /* ... */ }
pub struct Delta { /* ... */ }
```

**Status**: ✅ **300+ lines of working code, tested**

**2. Phylogenetic Clustering** (`talaria-bio/src/clustering/phylogenetic.rs`)
```rust
pub struct PhylogeneticClusterer {
    config: ClusteringConfig,
    taxonomy_db: Option<TaxonomyDB>,
}

// Groups sequences by taxonomy with diversity awareness
pub fn create_clusters(&self, sequences: Vec<Sequence>) -> Vec<TaxonomicCluster>;
```

**Status**: ✅ **590+ lines of working code, tested**

### What's Missing ❌

**Critical finding**: Infrastructure exists but is **NEVER CALLED**

```bash
# Proof - search for usage in main workflows:
$ grep -r "MyersDeltaCompressor\|compute_delta" talaria-herald/src/database/
# NO MATCHES

$ grep -r "PhylogeneticClusterer" talaria-cli/src/ talaria-herald/src/operations/
# NO MATCHES
```

**What needs to be built**:
1. ❌ Integration with reduction workflow
2. ❌ Reference sequence selection algorithm
3. ❌ Delta chain storage in RocksDB
4. ❌ Reconstruction from deltas
5. ❌ Phylogenetic tree structure (for advanced features)
6. ❌ Query interface for evolutionary analysis

---

## Part 2: Pragmatic Implementation Path

### Philosophy: Incremental Value Delivery

**Not**: Build entire phylogenetic tree system → 6-10 months → users get nothing until done

**Instead**: Three phases, each delivers value independently

```
Phase 1 (1-2 months): Basic deltas    → 10-50x compression  ✅ Ship to users
Phase 2 (2-3 months): Smart clustering → 20-100x compression ✅ Ship to users
Phase 3 (3-4 months): Phylo trees     → 50-500x + queries   ✅ Ship to users
```

---

## Part 3: Phase 1 - Basic Delta Integration (1-2 months)

### Goal
Connect existing delta code to reduction workflow for immediate compression gains.

### Design Decisions

**Reference Selection Strategy**: Medoid per taxonomic group
```
Why medoid (not random):
- Minimizes total distance to all other sequences in group
- Naturally finds "central" sequence
- Simple to compute: O(n²) pairwise distances

Algorithm:
For each taxonomic group:
  1. Compute pairwise identity for all sequences
  2. Select sequence with minimum sum of distances
  3. That's the reference (medoid)
```

**Storage Structure**:
```
RocksDB Column Family: DELTAS
  Key: target_sequence_hash (SHA256)
  Value: CanonicalDelta {
    reference_hash: Hash,
    delta: Delta {
      ops: Vec<DeltaOp>,
      compression_ratio: f32,
    },
    created_at: DateTime,
  }

Decision: Store deltas ONLY if compression_ratio > 0.8 (80% reduction)
Otherwise: Store full sequence (some sequences don't compress well)
```

### Implementation Steps

#### Step 1.1: Add Delta Storage to SequenceStorage

**File**: `talaria-herald/src/storage/sequence.rs`

**Changes**:
```rust
pub struct SequenceStorage {
    // Existing fields...
    delta_compressor: Option<Arc<MyersDeltaCompressor>>,
}

impl SequenceStorage {
    // NEW: Enable delta compression mode
    pub fn enable_delta_compression(&mut self, max_distance: usize) {
        self.delta_compressor = Some(Arc::new(
            MyersDeltaCompressor::new(max_distance, true)
        ));
    }

    // MODIFY: store_sequence to optionally compute delta
    pub fn store_sequence_with_delta(
        &self,
        sequence: &str,
        header: &str,
        source: DatabaseSource,
        reference_hash: Option<SHA256Hash>,  // NEW parameter
    ) -> Result<(SHA256Hash, bool, Option<DeltaStats>)> {
        let canonical_hash = SHA256Hash::compute(sequence.as_bytes());

        // Check if reference provided and delta enabled
        if let (Some(ref_hash), Some(compressor)) =
            (reference_hash, &self.delta_compressor)
        {
            // Try delta encoding
            let reference_seq = self.get_canonical(&ref_hash)?;
            let delta = compressor.compute_delta(
                reference_seq.sequence.as_bytes(),
                sequence.as_bytes()
            )?;

            // Only store delta if compression > 80%
            if delta.compression_ratio > 0.8 {
                self.storage_backend.store_delta(&canonical_hash, &delta)?;
                return Ok((canonical_hash, false, Some(delta.stats())));
            }
        }

        // Fallback: store full sequence
        self.store_canonical_sequence(sequence, header, source)
    }
}
```

#### Step 1.2: Add Delta Column Family to RocksDB

**File**: `talaria-storage/src/backend/rocksdb_backend.rs`

**Changes**:
```rust
const CF_DELTAS: &str = "DELTAS";

pub fn open_database(path: &Path) -> Result<DB> {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    let cfs = vec![
        CF_SEQUENCES,
        CF_REPRESENTATIONS,
        CF_MANIFESTS,
        CF_INDICES,
        CF_DELTAS,  // NEW
    ];

    // ... existing code
}

// NEW: Delta operations
pub fn store_delta(&self, target_hash: &SHA256Hash, delta: &Delta) -> Result<()> {
    let cf = self.db.cf_handle(CF_DELTAS).unwrap();
    let value = rmp_serde::to_vec(delta)?;
    self.db.put_cf(cf, target_hash.as_bytes(), value)?;
    Ok(())
}

pub fn get_delta(&self, target_hash: &SHA256Hash) -> Result<Option<Delta>> {
    let cf = self.db.cf_handle(CF_DELTAS).unwrap();
    match self.db.get_cf(cf, target_hash.as_bytes())? {
        Some(bytes) => Ok(Some(rmp_serde::from_slice(&bytes)?)),
        None => Ok(None),
    }
}
```

#### Step 1.3: Integrate with Reducer

**File**: `talaria-herald/src/operations/reducer.rs`

**Changes**:
```rust
pub struct SequenceReducer {
    // Existing fields...
    enable_delta_compression: bool,  // NEW
}

impl SequenceReducer {
    pub fn reduce_with_deltas(&mut self, sequences: Vec<Sequence>) -> Result<ReductionResult> {
        // Step 1: Group by taxonomy (existing)
        let taxonomic_groups = self.group_by_taxonomy(sequences);

        let mut references = Vec::new();
        let mut delta_encoded = Vec::new();

        for (taxon_id, group_sequences) in taxonomic_groups {
            // Step 2: Select medoid as reference
            let reference_idx = self.select_medoid(&group_sequences)?;
            let reference = &group_sequences[reference_idx];
            let ref_hash = SHA256Hash::compute(reference.sequence_str().as_bytes());

            // Step 3: Store reference (full sequence)
            self.sequence_storage.store_sequence(
                &reference.sequence_str(),
                &reference.header,
                self.source.clone(),
            )?;
            references.push(ref_hash.clone());

            // Step 4: Encode remaining sequences as deltas
            for (i, seq) in group_sequences.iter().enumerate() {
                if i == reference_idx {
                    continue;  // Skip reference itself
                }

                let (seq_hash, _, delta_stats) = self.sequence_storage
                    .store_sequence_with_delta(
                        &seq.sequence_str(),
                        &seq.header,
                        self.source.clone(),
                        Some(ref_hash.clone()),  // Reference for delta
                    )?;

                if let Some(stats) = delta_stats {
                    delta_encoded.push((seq_hash, stats));
                }
            }
        }

        Ok(ReductionResult {
            references,
            delta_encoded,
            compression_stats: self.compute_stats(),
        })
    }

    // NEW: Select medoid (central sequence)
    fn select_medoid(&self, sequences: &[Sequence]) -> Result<usize> {
        let n = sequences.len();
        if n == 1 {
            return Ok(0);
        }

        // Compute pairwise identity matrix
        let mut distances = vec![vec![0.0; n]; n];
        for i in 0..n {
            for j in (i+1)..n {
                let identity = self.compute_identity(
                    sequences[i].sequence_str().as_bytes(),
                    sequences[j].sequence_str().as_bytes(),
                )?;
                let distance = 1.0 - identity;
                distances[i][j] = distance;
                distances[j][i] = distance;
            }
        }

        // Find sequence with minimum sum of distances
        let mut best_idx = 0;
        let mut best_sum = f32::MAX;
        for i in 0..n {
            let sum: f32 = distances[i].iter().sum();
            if sum < best_sum {
                best_sum = sum;
                best_idx = i;
            }
        }

        Ok(best_idx)
    }

    fn compute_identity(&self, a: &[u8], b: &[u8]) -> Result<f32> {
        let delta = self.delta_compressor.compute_delta(a, b)?;
        Ok(1.0 - delta.compression_ratio)
    }
}
```

#### Step 1.4: Add Reconstruction Support

**File**: `talaria-herald/src/storage/sequence.rs`

**Changes**:
```rust
impl SequenceStorage {
    // NEW: Get sequence (may require delta reconstruction)
    pub fn get_sequence_reconstructed(&self, hash: &SHA256Hash) -> Result<Vec<u8>> {
        // Try canonical first
        if let Some(canonical) = self.get_canonical(hash)? {
            return Ok(canonical.sequence.into_bytes());
        }

        // Try delta reconstruction
        if let Some(delta) = self.storage_backend.get_delta(hash)? {
            let reference = self.get_canonical(&delta.reference_hash)?
                .ok_or_else(|| anyhow!("Reference not found"))?;

            let compressor = self.delta_compressor.as_ref()
                .ok_or_else(|| anyhow!("Delta compressor not initialized"))?;

            let reconstructed = compressor.apply_delta(
                reference.sequence.as_bytes(),
                &delta.delta,
            )?;

            // Verify hash matches
            let computed_hash = SHA256Hash::compute(&reconstructed);
            if computed_hash != *hash {
                return Err(anyhow!("Delta reconstruction hash mismatch"));
            }

            return Ok(reconstructed);
        }

        Err(anyhow!("Sequence not found: {:?}", hash))
    }
}
```

### Testing Plan for Phase 1

**Unit Tests**:
```rust
#[test]
fn test_delta_compression_storage() {
    let mut storage = SequenceStorage::new_test();
    storage.enable_delta_compression(1000);

    let ref_seq = "MSKGEELFTGVVPILVELDGDVNGH";
    let target_seq = "MSKGEELFTGVVPILVVLDGDVNGH";  // One change

    // Store reference
    let (ref_hash, _) = storage.store_sequence(ref_seq, ">ref", source)?;

    // Store target with delta
    let (target_hash, _, delta_stats) = storage.store_sequence_with_delta(
        target_seq,
        ">target",
        source,
        Some(ref_hash),
    )?;

    assert!(delta_stats.is_some());
    assert!(delta_stats.unwrap().compression_ratio > 0.9);

    // Reconstruct
    let reconstructed = storage.get_sequence_reconstructed(&target_hash)?;
    assert_eq!(reconstructed, target_seq.as_bytes());
}
```

**Integration Test**:
```bash
# Create test dataset with similar sequences
$ cat > test_similar.fasta <<EOF
>seq1 TaxID=562
MSKGEELFTGVVPILVELDGDVNGH
>seq2 TaxID=562
MSKGEELFTGVVPILVVLDGDVNGH
>seq3 TaxID=562
MSKGEELFTGVVPILIELDGDVNGH
EOF

# Run reduce with deltas
$ talaria reduce test_similar.fasta --enable-deltas --output reduced.fasta

# Expected output:
# ✓ Selected 1 reference sequence
# ✓ Encoded 2 sequences as deltas
# ✓ Compression: 95.2% (delta ops: 12 bytes vs 26 bytes full)
```

**Performance Benchmark**:
```rust
#[bench]
fn bench_delta_compression_protein_family(b: &mut Bencher) {
    let sequences = generate_kinase_family(1000);  // 1000 similar kinases

    b.iter(|| {
        let reducer = SequenceReducer::new_with_deltas();
        reducer.reduce_with_deltas(sequences.clone())
    });

    // Expected: ~50-100x compression for protein families
}
```

### Expected Results for Phase 1

**Compression Gains**:
- Protein families (80%+ identity): **50-100x compression**
- Orthologs across species (60-80%): **10-30x compression**
- Unrelated proteins (<40%): **No delta, store full sequence**

**Storage Overhead**:
- Delta operations: ~10-50 bytes per sequence (vs 300-500 bytes full)
- Reference sequences: Stored once per taxonomic group
- Total: **+2 GB** for UniRef50 (6 GB total vs 4 GB current)

**Performance**:
- Reduction: ~10-20% slower (delta computation overhead)
- Reconstruction: ~1-2ms per sequence (acceptable for queries)
- Memory: Unchanged (deltas computed on-the-fly)

---

## Part 4: Phase 2 - Smart Similarity Clustering (2-3 months)

### Goal
Multi-level clustering by sequence identity (90%, 70%, 50%) with hierarchical delta chains.

### Design: Hierarchical Delta Chains

```
Current (Phase 1): Flat structure
  Reference_1 → [Delta_A, Delta_B, Delta_C, ...]
  Reference_2 → [Delta_D, Delta_E, ...]

Enhanced (Phase 2): Hierarchical structure
  Cluster_90%
    Reference_90 → [Delta_91%, Delta_93%, ...]
    └─ Cluster_70%
        Reference_70 → [Delta_75%, Delta_68%, ...]
        └─ Cluster_50%
            Reference_50 → [Delta_55%, Delta_48%, ...]
```

### Implementation Steps

#### Step 2.1: Hierarchical Clustering

**File**: `talaria-herald/src/operations/hierarchical_clustering.rs` (NEW)

```rust
pub struct HierarchicalCluster {
    identity_threshold: f32,  // 0.90, 0.70, 0.50
    reference: Sequence,
    members: Vec<Sequence>,
    subclusters: Vec<HierarchicalCluster>,
}

impl HierarchicalCluster {
    pub fn build_hierarchy(
        sequences: Vec<Sequence>,
        thresholds: &[f32],  // [0.90, 0.70, 0.50]
    ) -> Result<Vec<Self>> {
        if thresholds.is_empty() {
            return Ok(vec![]);
        }

        let current_threshold = thresholds[0];
        let remaining_thresholds = &thresholds[1..];

        // Cluster at current level
        let clusters = Self::cluster_at_level(sequences, current_threshold)?;

        // Recursively build subclusters
        let hierarchical_clusters = clusters.into_iter().map(|mut cluster| {
            // Sequences below threshold become subclusters
            let low_identity_seqs: Vec<_> = cluster.members.iter()
                .filter(|s| identity(s, &cluster.reference) < current_threshold)
                .cloned()
                .collect();

            if !low_identity_seqs.is_empty() && !remaining_thresholds.is_empty() {
                cluster.subclusters = Self::build_hierarchy(
                    low_identity_seqs,
                    remaining_thresholds,
                )?;
            }

            Ok(cluster)
        }).collect()?;

        Ok(hierarchical_clusters)
    }
}
```

#### Step 2.2: Delta Chain Storage

**New RocksDB structure**:
```rust
// Column Family: DELTA_CHAINS
Key: chain_id (UUID)
Value: DeltaChain {
    root_reference: Hash,
    levels: Vec<DeltaLevel>,
}

struct DeltaLevel {
    identity_threshold: f32,
    deltas: HashMap<Hash, DeltaOp>,  // target → delta from parent
    parent_level: Option<usize>,
}
```

#### Step 2.3: Smart Reference Selection

**Upgrade from medoid to phylogenetically-informed selection**:

```rust
fn select_reference_smart(
    sequences: &[Sequence],
    taxonomy_db: &TaxonomyDB,
) -> Result<usize> {
    // Strategy: Select reference closest to phylogenetic root of group
    let taxon_ids: Vec<_> = sequences.iter()
        .filter_map(|s| s.taxon_id)
        .collect();

    let common_ancestor = taxonomy_db.find_lca(&taxon_ids)?;

    // Find sequence from taxon closest to common ancestor
    let mut best_idx = 0;
    let mut best_distance = usize::MAX;

    for (i, seq) in sequences.iter().enumerate() {
        if let Some(taxon) = seq.taxon_id {
            let distance = taxonomy_db.distance_to_ancestor(taxon, common_ancestor);
            if distance < best_distance {
                best_distance = distance;
                best_idx = i;
            }
        }
    }

    Ok(best_idx)
}
```

### Expected Results for Phase 2

**Compression Gains**:
- Highly conserved families (>90% identity): **100-500x**
- Moderately conserved (70-90%): **30-100x**
- Divergent families (50-70%): **10-30x**

**Storage**: **+1 GB** additional for hierarchy metadata (9 GB total)

---

## Part 5: Phase 3 - Phylogenetic Trees (3-4 months)

### Goal
Full phylogenetic tree structure with ancestral reconstruction and evolutionary queries.

### Design: Tree Storage

```rust
// Column Family: PHYLO_TREES
Key: family_id (e.g., "Kinase", "Globin")
Value: PhylogeneticTree {
    newick: String,  // Tree structure in Newick format
    nodes: HashMap<NodeId, PhyloNode>,
    root: NodeId,
}

struct PhyloNode {
    node_id: NodeId,
    sequence_hash: Option<Hash>,  // Leaf = sequence, Internal = ancestor
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    branch_length: f64,  // Evolutionary distance
    mutations: Vec<Mutation>,  // Changes from parent
}

struct Mutation {
    position: usize,
    from: u8,
    to: u8,
    mutation_type: MutationType,  // Synonymous, Nonsynonymous, etc.
}
```

### Implementation: Tree Builder

**File**: `talaria-herald/src/phylo/tree_builder.rs` (NEW)

```rust
pub struct PhylogeneticTreeBuilder {
    taxonomy_db: TaxonomyDB,
}

impl PhylogeneticTreeBuilder {
    // Build tree from NCBI taxonomy as scaffold
    pub fn build_from_taxonomy(
        &self,
        sequences: Vec<Sequence>,
        family_name: &str,
    ) -> Result<PhylogeneticTree> {
        // Step 1: Get taxonomic tree for these sequences
        let taxon_ids = extract_taxon_ids(&sequences);
        let tax_tree = self.taxonomy_db.build_tree(&taxon_ids)?;

        // Step 2: Map sequences to tree leaves
        let mut tree = PhylogeneticTree::from_taxonomy(tax_tree);
        for seq in sequences {
            tree.add_leaf(seq)?;
        }

        // Step 3: Reconstruct ancestral sequences at internal nodes
        tree.reconstruct_ancestors()?;

        // Step 4: Compute mutations along edges
        tree.compute_edge_mutations()?;

        Ok(tree)
    }
}
```

### Implementation: Ancestral Reconstruction

```rust
impl PhylogeneticTree {
    fn reconstruct_ancestors(&mut self) -> Result<()> {
        // Post-order traversal: leaves → root
        for node_id in self.post_order_traversal() {
            if self.is_leaf(node_id) {
                continue;  // Leaves have known sequences
            }

            // Get children sequences
            let children = self.get_children(node_id);
            let child_seqs: Vec<_> = children.iter()
                .map(|&child| self.get_sequence(child))
                .collect()?;

            // Reconstruct ancestor using consensus
            let ancestral_seq = self.reconstruct_consensus(&child_seqs)?;

            // Store ancestral sequence
            let hash = SHA256Hash::compute(ancestral_seq.as_bytes());
            self.sequence_storage.store_ancestral(hash, &ancestral_seq)?;
            self.nodes.get_mut(&node_id).unwrap().sequence_hash = Some(hash);
        }

        Ok(())
    }

    fn reconstruct_consensus(&self, sequences: &[Vec<u8>]) -> Result<Vec<u8>> {
        let len = sequences[0].len();
        let mut consensus = Vec::with_capacity(len);

        for pos in 0..len {
            // Count amino acids at this position
            let mut counts = HashMap::new();
            for seq in sequences {
                *counts.entry(seq[pos]).or_insert(0) += 1;
            }

            // Select most common (majority rule)
            let most_common = counts.iter()
                .max_by_key(|(_, &count)| count)
                .map(|(&aa, _)| aa)
                .unwrap_or(b'X');  // X = unknown

            consensus.push(most_common);
        }

        Ok(consensus)
    }
}
```

### Implementation: Query Interface

**File**: `talaria-cli/src/cli/commands/phylo.rs` (NEW)

```rust
pub fn cmd_phylo_query(args: PhyloQueryArgs) -> Result<()> {
    match args.query_type {
        QueryType::Ancestors { sequence_id } => {
            show_ancestral_path(sequence_id)
        },
        QueryType::Descendants { ancestor_id } => {
            show_all_descendants(ancestor_id, args.show_mutations)
        },
        QueryType::Mutations { family, filter } => {
            show_evolutionary_mutations(family, filter)
        },
    }
}

fn show_all_descendants(ancestor_id: &str, show_mutations: bool) -> Result<()> {
    let tree = phylo_index.get_tree_for_sequence(ancestor_id)?;
    let node = tree.find_node(ancestor_id)?;

    println!("Descendants of {} ({} total):", ancestor_id, tree.count_descendants(node));

    for descendant in tree.traverse_descendants(node) {
        if show_mutations {
            let path = tree.get_path(node, descendant)?;
            let mutations = tree.get_mutations_along_path(&path)?;
            println!("  {} - {} mutations", descendant.id, mutations.len());
            for mutation in mutations {
                println!("    {}{}{} at position {}",
                    mutation.from as char,
                    mutation.position,
                    mutation.to as char,
                    mutation.mutation_type,
                );
            }
        } else {
            println!("  {}", descendant.id);
        }
    }

    Ok(())
}
```

### The "Gigantic Tree" Problem - Solution

**Q**: How to efficiently query "show all mutations from Ancestral_Kinase"?

**A**: Pre-computed indices + lazy loading

```rust
// Index built once during tree construction
struct PhylogeneticIndex {
    // Map: family_name → tree_file_path
    family_to_tree: HashMap<String, PathBuf>,

    // Map: sequence_hash → (family_name, node_id)
    sequence_to_node: HashMap<Hash, (String, NodeId)>,

    // Map: node_id → descendant_count
    descendant_counts: HashMap<NodeId, usize>,
}

// Query implementation - loads only needed parts
fn get_descendants_lazy(ancestor_id: &str) -> Result<impl Iterator<Item = Mutation>> {
    // Step 1: Find which tree contains this sequence
    let (family, node_id) = index.sequence_to_node.get(ancestor_id)?;

    // Step 2: Load ONLY this tree (not all 20K trees)
    let tree = load_tree(family)?;  // ~1-10 MB for typical family

    // Step 3: Traverse descendants (in-memory, fast)
    let descendants = tree.traverse_from(node_id);

    // Step 4: Stream mutations (lazy iterator, don't load all at once)
    Ok(descendants.flat_map(|desc| tree.get_edge_mutations(desc)))
}
```

**Storage strategy**:
```
/home/user/.talaria/phylo_trees/
  ├─ Kinase.tree           (2 MB - 10K sequences)
  ├─ Globin.tree           (500 KB - 2K sequences)
  ├─ Protease.tree         (5 MB - 20K sequences)
  └─ ... (20K families total = ~40 GB on disk)

Only load trees as needed (lazy loading)
Keep recently used in cache (LRU, ~1 GB RAM)
```

### Expected Results for Phase 3

**Compression**: **50-500x** (ancestral references + delta chains)

**Storage**: **+10 GB** for phylogenetic trees (19 GB total for UniRef50)

**Query Performance**:
- Find ancestors: **O(log n)** tree traversal
- Find descendants: **O(descendants)** with lazy loading
- Show mutations: **O(path_length)** typically 5-20 nodes

**New Capabilities**:
```bash
# Ancestral reconstruction
talaria phylo ancestors sp|P12345 --output-newick

# Evolutionary analysis
talaria phylo mutations --family Kinase --type nonsynonymous

# Time travel
talaria phylo reconstruct --node internal_node_123 --time 100MYA
```

---

## Part 6: Storage Requirements & Scaling

### Size Estimates for UniRef50 (48M sequences)

| Component | Phase 1 | Phase 2 | Phase 3 | % of Raw |
|-----------|---------|---------|---------|----------|
| **Canonical sequences** | 2.0 GB | 1.5 GB | 1.0 GB | 0.6% |
| **Delta operations** | 2.0 GB | 3.0 GB | 4.0 GB | 2.4% |
| **Clustering metadata** | 0.2 GB | 1.0 GB | 1.0 GB | 0.6% |
| **Phylogenetic trees** | - | - | 10.0 GB | 6.1% |
| **Indices** | 0.5 GB | 1.0 GB | 3.0 GB | 1.8% |
| **RocksDB overhead** | 1.3 GB | 2.5 GB | 0.0 GB | 0.0% |
| **Total** | **6.0 GB** | **9.0 GB** | **19.0 GB** | **11.5%** |

**Comparison**:
- Raw FASTA: **165 GB** (100%)
- gzip: **48 GB** (29%)
- Current HERALD: **4.2 GB** (2.5%)
- Phase 1: **6.0 GB** (3.6%) - 27.5x better than raw
- Phase 2: **9.0 GB** (5.5%) - 18.3x better than raw
- Phase 3: **19.0 GB** (11.5%) - 8.7x better than raw + evolutionary queries

**Conclusion**: Even with full phylogenetic trees, still **8.7x better than gzip** and enables entirely new analytical capabilities.

---

## Part 7: Known Limitations & Workarounds

### Limitation 1: Horizontal Gene Transfer (HGT)

**Problem**: Phylogenetic trees assume vertical inheritance; HGT breaks this model.

**Workaround**:
```rust
// Detect anomalous delta compression ratios
if delta.compression_ratio < expected_from_taxonomy {
    // Flag as potential HGT
    metadata.add_flag("potential_hgt");
    // Store full sequence instead of delta
}
```

### Limitation 2: Recombination Events

**Problem**: Different parts of sequence have different evolutionary histories.

**Workaround**: Domain-level phylogenetic trees (future work)
```
Current: One tree per protein family
Future: One tree per protein domain
Result: Handle mosaic proteins correctly
```

### Limitation 3: Tree Update Cost

**Problem**: New sequences require tree rebuilding (expensive).

**Workaround**: Incremental tree insertion
```rust
fn insert_into_existing_tree(tree: &mut PhyloTree, new_seq: Sequence) {
    // Find nearest neighbor by sequence similarity
    let nearest = tree.find_nearest_leaf(&new_seq)?;

    // Insert as sibling (approximate placement)
    tree.insert_sibling(nearest, new_seq)?;

    // Mark tree as "approximate" - rebuild periodically
    tree.metadata.needs_rebuild = tree.insertions_since_rebuild > 1000;
}
```

### Limitation 4: Storage Growth

**Problem**: 19 GB is still significant for edge deployments.

**Workaround**: Tiered storage
```
Tier 1 (always downloaded): Canonical sequences + basic deltas (6 GB)
Tier 2 (on-demand): Hierarchical clusters (+ 3 GB)
Tier 3 (optional): Phylogenetic trees (+ 10 GB)

User choice:
  --mode basic     # 6 GB
  --mode enhanced  # 9 GB
  --mode full      # 19 GB
```

---

## Part 8: Migration Strategy

### Handling Existing Data

**Phase 1 Deployment**:
```rust
// Existing databases work unchanged
// New imports use delta encoding

fn import_with_migration(file: &Path) -> Result<()> {
    if config.enable_deltas {
        // Use new delta-enabled import
        import_with_deltas(file)?;
    } else {
        // Use existing import (backwards compatible)
        import_legacy(file)?;
    }
}

// CLI flag to enable
$ talaria database add file.fasta --enable-deltas
```

**Upgrade Existing Database**:
```bash
# Reprocess existing database to add deltas
$ talaria database optimize uniprot/swissprot --add-deltas

# Process:
# 1. Read all sequences
# 2. Compute deltas retrospectively
# 3. Store deltas alongside existing data
# 4. Update manifest to indicate "delta-enabled"
```

---

## Part 9: Success Metrics

### Phase 1 Success Criteria
- ✅ 10-50x compression on protein families
- ✅ <10% performance overhead on import
- ✅ <2ms reconstruction time
- ✅ Zero data loss (all sequences recoverable)
- ✅ Backwards compatible with existing databases

### Phase 2 Success Criteria
- ✅ 20-100x compression on conserved families
- ✅ Hierarchical queries work (cluster at any level)
- ✅ Storage <10 GB for UniRef50

### Phase 3 Success Criteria
- ✅ 50-500x compression with phylo trees
- ✅ Evolutionary queries <1s for typical families
- ✅ Ancestral reconstruction produces biologically valid sequences
- ✅ Tree storage <50 GB for all of UniRef50

---

## Part 10: Next Steps

### Immediate Action Items

1. **Create feature branch**:
   ```bash
   git checkout -b feature/evolutionary-delta-compression
   ```

2. **Start with Phase 1, Step 1.1**:
   - Modify `talaria-herald/src/storage/sequence.rs`
   - Add `enable_delta_compression()` method
   - Add `store_sequence_with_delta()` method

3. **Write failing test first** (TDD):
   ```rust
   #[test]
   fn test_delta_compression_basic() {
       // This will fail until implementation complete
       let mut storage = SequenceStorage::new_test();
       storage.enable_delta_compression(1000);

       // ... rest of test
   }
   ```

4. **Implement until test passes**

5. **Repeat for Steps 1.2, 1.3, 1.4**

### Timeline Estimate

- **Phase 1**: 1-2 months (4-8 weeks)
- **Phase 2**: 2-3 months (8-12 weeks)
- **Phase 3**: 3-4 months (12-16 weeks)
- **Total**: 6-9 months for full implementation

### Parallelization Opportunities

Can work on multiple phases in parallel:
- **Team member 1**: Phase 1 implementation
- **Team member 2**: Phase 2 design + tree research
- **Team member 3**: Documentation + benchmarking

---

## Conclusion

This guide provides a **concrete, actionable path** from concept to implementation. Each phase delivers value independently, and the incremental approach reduces risk.

**Key takeaway**: You don't need to implement the entire phylogenetic tree system to get value. Phase 1 alone (1-2 months) delivers 10-50x compression using existing code that just needs to be connected.

**Status**: Ready to implement. Start with Phase 1, Step 1.1.
