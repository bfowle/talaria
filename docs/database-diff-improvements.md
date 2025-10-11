# Database Diff Implementation Improvements

## Current Problems

### 1. Comparing Chunks Instead of Sequences

**Current code** (`talaria-sequoia/src/operations/database_diff.rs:546-577`):
```rust
fn compare_sequences_from_manifests(
    manifest_a: &TemporalManifest,
    manifest_b: &TemporalManifest,
) -> SequenceAnalysis {
    // Gets CHUNK hashes (ManifestMetadata)
    let chunks_a: HashSet<_> = manifest_a.chunk_index.iter()
        .map(|m| m.hash.clone()).collect();

    let chunks_b: HashSet<_> = manifest_b.chunk_index.iter()
        .map(|m| m.hash.clone()).collect();

    // Finds SHARED CHUNKS
    let shared_chunk_hashes: HashSet<_> = chunks_a.intersection(&chunks_b).cloned().collect();

    // Counts sequences in shared chunks
    let shared_seq_count: usize = manifest_a.chunk_index.iter()
        .filter(|m| shared_chunk_hashes.contains(&m.hash))
        .map(|m| m.sequence_count).sum();
}
```

**Problem**: This counts sequences in chunks that have identical hashes, NOT sequences that exist in both databases.

**Why it fails**:
- Chunks are content-addressed: `SHA256(ChunkManifest)`
- ChunkManifest contains: sequence_refs, taxon_ids, metadata
- Different databases chunk differently (different taxonomic boundaries)
- Same sequences, different chunks → different hashes → 0% sharing

**Example**:
```
SwissProt chunk:
  - Chunk hash: sha256:aaa...
  - Sequences: [seq1, seq2, seq3] (Human proteins)
  - Taxon IDs: [9606]

UniRef50 chunk:
  - Chunk hash: sha256:bbb... (DIFFERENT! Different chunking)
  - Sequences: [seq1, seq2, seq3] (SAME sequences!)
  - Taxon IDs: [9606, 10090, 10116] (broader taxonomic grouping)

Current diff: 0% shared (chunk hashes don't match)
Should be:     100% shared (all 3 sequences in both)
```

### 2. No Access to Actual Sequence Hashes

The current implementation has access to:
- `ManifestMetadata.hash` - Hash of the ChunkManifest object
- `ManifestMetadata.sequence_count` - How many sequences in chunk
- `ManifestMetadata.taxon_ids` - Taxonomic IDs in chunk
- `ManifestMetadata.size` - Total size

It does NOT have:
- `ChunkManifest.sequence_refs` - The actual sequence hashes
- Individual sequence information

**To get actual sequence hashes, must**:
1. Load chunk from storage: `storage.get_chunk(&metadata.hash)`
2. Deserialize: `bincode::deserialize::<ChunkManifest>(&chunk_data)`
3. Extract: `manifest.sequence_refs` (Vec<SHA256Hash>)

### 3. No Visualization of Relationships

Current output:
```
Shared sequences: 0 (0.0%)
```

Needed output:
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Database Comparison: uniprot/swissprot vs uniprot/uniref50
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Sequences:
  SwissProt:      571,609 sequences
  UniRef50:       70,408,371 sequences

  Shared:         45,231 sequences (7.9% of SwissProt, 0.06% of UniRef50)
  ├─ Identical sequences found in both databases
  └─ Content hash matches (sha256 of amino acid sequence)

  Unique to SwissProt:   526,378 sequences (92.1%)
  Unique to UniRef50:    70,363,140 sequences (99.94%)

Chunks:
  SwissProt:      2,341 chunks
  UniRef50:       28,567 chunks

  Shared chunks:  0 (0.0%)
  └─ Different chunking strategies prevent chunk-level sharing

Storage Analysis:
  If stored separately:
    SwissProt:    1.2 GB
    UniRef50:     280 GB
    Total:        281.2 GB

  With content-addressed deduplication:
    Shared sequences:  45 MB (45,231 × ~1KB avg)
    Unique storage:    281.155 GB
    Savings:           45 MB (0.016%)

  └─ Low deduplication because UniRef50 uses cluster representatives,
     not original sequences from SwissProt

Taxonomy Overlap:
  Shared taxa:    8,361 (56.5% of SwissProt, 6.0% of UniRef50)
  Top shared:
    ├─ 9606 (Homo sapiens): 12,450 sequences in SwissProt, 23,789 in UniRef50
    ├─ 10090 (Mus musculus): 8,234 sequences in SwissProt, 15,672 in UniRef50
    └─ ...

Sample Shared Sequences:
  1. sha256:a3f2b8c9 - Human insulin (found in both)
  2. sha256:7d4e1a5c - RecA protein (found in both)
  3. ...

Why Low Sharing?
  UniRef50 uses cluster representatives (longest sequence in 50% identity cluster)
  SwissProt contains curated full-length sequences
  Even when covering same proteins, sequences differ due to:
    - Clustering picks longest variant
    - Headers differ (different IDs)
    - Sequences may have minor differences
```

## Improved Implementation

### Phase 1: Load Actual Sequence Hashes

```rust
// talaria-sequoia/src/operations/database_diff.rs

use std::collections::HashSet;
use talaria_core::SHA256Hash;
use crate::{ChunkManifest, ManifestMetadata, TemporalManifest};

/// Extract all sequence hashes from a manifest by loading chunks
fn extract_sequence_hashes(
    manifest: &TemporalManifest,
    storage: &impl ChunkStorage,
) -> Result<HashSet<SHA256Hash>> {
    let mut all_sequences = HashSet::new();

    for chunk_metadata in &manifest.chunk_index {
        // Load the actual ChunkManifest from storage
        let chunk_data = storage.get_chunk(&chunk_metadata.hash)?;
        let chunk_manifest: ChunkManifest = bincode::deserialize(&chunk_data)?;

        // Extract sequence hashes
        all_sequences.extend(chunk_manifest.sequence_refs.iter().cloned());
    }

    Ok(all_sequences)
}

/// Compare sequences properly at hash level
fn compare_sequences_properly(
    manifest_a: &TemporalManifest,
    manifest_b: &TemporalManifest,
    storage: &impl ChunkStorage,
) -> Result<SequenceAnalysis> {
    // Load actual sequence hashes
    let seqs_a = extract_sequence_hashes(manifest_a, storage)?;
    let seqs_b = extract_sequence_hashes(manifest_b, storage)?;

    // Compute set operations
    let shared: HashSet<_> = seqs_a.intersection(&seqs_b).cloned().collect();
    let unique_a: HashSet<_> = seqs_a.difference(&seqs_b).cloned().collect();
    let unique_b: HashSet<_> = seqs_b.difference(&seqs_a).cloned().collect();

    // Get sample sequences for display
    let sample_shared = get_sequence_samples(&shared, storage, 10)?;
    let sample_unique_a = get_sequence_samples(&unique_a, storage, 5)?;
    let sample_unique_b = get_sequence_samples(&unique_b, storage, 5)?;

    Ok(SequenceAnalysis {
        total_sequences_a: seqs_a.len(),
        total_sequences_b: seqs_b.len(),
        shared_sequences: shared.len(),
        unique_to_a: unique_a.len(),
        unique_to_b: unique_b.len(),
        shared_percentage_a: (shared.len() as f64 / seqs_a.len() as f64) * 100.0,
        shared_percentage_b: (shared.len() as f64 / seqs_b.len() as f64) * 100.0,
        sample_shared_ids: sample_shared,
        sample_unique_a_ids: sample_unique_a,
        sample_unique_b_ids: sample_unique_b,
    })
}
```

### Phase 2: Enhanced Analysis Structure

```rust
// Extended analysis with more detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedSequenceAnalysis {
    // Basic counts
    pub total_sequences_a: usize,
    pub total_sequences_b: usize,

    // Sharing analysis
    pub shared_sequences: usize,
    pub shared_percentage_a: f64,
    pub shared_percentage_b: f64,
    pub unique_to_a: usize,
    pub unique_to_b: usize,

    // Storage analysis
    pub estimated_size_a: usize,
    pub estimated_size_b: usize,
    pub estimated_shared_size: usize,
    pub dedup_savings_bytes: usize,
    pub dedup_savings_percentage: f64,

    // Sample data for display
    pub sample_shared: Vec<SequenceInfo>,
    pub sample_unique_a: Vec<SequenceInfo>,
    pub sample_unique_b: Vec<SequenceInfo>,

    // FUTURE: If HERALD implemented
    pub reference_analysis: Option<ReferenceAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceInfo {
    pub hash: SHA256Hash,
    pub header: String,
    pub length: usize,
    pub taxon_id: Option<TaxonId>,
    pub in_databases: Vec<String>,
}

// FUTURE: For HERALD implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceAnalysis {
    // Reference/delta breakdown
    pub references_a: usize,
    pub deltas_a: usize,
    pub references_b: usize,
    pub deltas_b: usize,

    // Shared references (actual storage deduplication)
    pub shared_references: Vec<SHA256Hash>,
    pub shared_reference_count: usize,
    pub shared_reference_percentage: f64,

    // Reference families (for visualization)
    pub reference_families: Vec<ReferenceFamilyInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceFamilyInfo {
    pub reference_hash: SHA256Hash,
    pub reference_header: String,
    pub databases: Vec<String>,
    pub child_count: usize,
    pub total_family_size: usize,
    pub compression_ratio: f64,
}
```

### Phase 3: Rich Visualization

```rust
// talaria-cli/src/cli/formatting/diff_formatter.rs

use comfy_table::{Table, Cell, Color, Attribute};
use colored::Colorize;

pub fn display_enhanced_diff(
    analysis: &EnhancedSequenceAnalysis,
    db_a_name: &str,
    db_b_name: &str,
) {
    // Header
    println!("\n{}", "━".repeat(80).bright_cyan());
    println!("{:^80}", format!("Database Comparison: {} vs {}", db_a_name, db_b_name).bright_white().bold());
    println!("{}", "━".repeat(80).bright_cyan());

    // Sequence counts
    println!("\n{}", "Sequences:".bright_yellow().bold());
    let mut table = Table::new();
    table.set_header(vec!["Database", "Total Sequences", "Unique", "Shared"]);

    table.add_row(vec![
        Cell::new(db_a_name).fg(Color::Cyan),
        Cell::new(format_number(analysis.total_sequences_a)),
        Cell::new(format!("{} ({:.1}%)",
            format_number(analysis.unique_to_a),
            100.0 - analysis.shared_percentage_a
        )),
        Cell::new(format!("{} ({:.1}%)",
            format_number(analysis.shared_sequences),
            analysis.shared_percentage_a
        )).fg(Color::Green),
    ]);

    table.add_row(vec![
        Cell::new(db_b_name).fg(Color::Cyan),
        Cell::new(format_number(analysis.total_sequences_b)),
        Cell::new(format!("{} ({:.1}%)",
            format_number(analysis.unique_to_b),
            100.0 - analysis.shared_percentage_b
        )),
        Cell::new(format!("{} ({:.1}%)",
            format_number(analysis.shared_sequences),
            analysis.shared_percentage_b
        )).fg(Color::Green),
    ]);

    println!("{table}");

    // Storage analysis
    println!("\n{}", "Storage Analysis:".bright_yellow().bold());
    println!("  If stored separately:");
    println!("    {} {}", db_a_name.bright_cyan(), format_bytes(analysis.estimated_size_a));
    println!("    {} {}", db_b_name.bright_cyan(), format_bytes(analysis.estimated_size_b));
    println!("    {} {}",
        "Total:".bright_white(),
        format_bytes(analysis.estimated_size_a + analysis.estimated_size_b)
    );

    println!("\n  With content-addressed deduplication:");
    println!("    {} {}",
        "Shared sequences:".bright_green(),
        format_bytes(analysis.estimated_shared_size)
    );
    println!("    {} {}",
        "Unique storage:".bright_white(),
        format_bytes(analysis.estimated_size_a + analysis.estimated_size_b - analysis.estimated_shared_size)
    );
    println!("    {} {} ({:.1}%)",
        "Savings:".bright_green(),
        format_bytes(analysis.dedup_savings_bytes),
        analysis.dedup_savings_percentage
    );

    // Sample shared sequences
    if !analysis.sample_shared.is_empty() {
        println!("\n{}", "Sample Shared Sequences:".bright_yellow().bold());
        for (i, seq) in analysis.sample_shared.iter().enumerate() {
            println!("  {}. {} - {}",
                i + 1,
                seq.hash.truncated(12).bright_cyan(),
                seq.header.bright_white()
            );
        }
    }

    // Interpretation help
    println!("\n{}", "Interpretation:".bright_yellow().bold());
    if analysis.shared_percentage_a < 1.0 && analysis.shared_percentage_b < 1.0 {
        println!("  {} Low sequence sharing detected", "ℹ".bright_blue());
        println!("    This is expected when comparing:");
        println!("      - Clustered databases (UniRef50/90) vs unclustered (SwissProt)");
        println!("      - Different database sources (UniProt vs NCBI)");
        println!("      - Databases with different sequence representations");
    } else if analysis.shared_percentage_a > 80.0 || analysis.shared_percentage_b > 80.0 {
        println!("  {} High sequence sharing detected", "✓".bright_green());
        println!("    Significant deduplication possible with content-addressed storage");
    }

    println!();
}
```

### Phase 4: Visual Tree for Reference Families (FUTURE)

```rust
// When HERALD is implemented, show reference relationships

pub fn display_reference_tree(
    reference_analysis: &ReferenceAnalysis,
    max_families: usize,
) {
    println!("\n{}", "Reference Families (Top 10):".bright_yellow().bold());

    for (i, family) in reference_analysis.reference_families.iter().take(max_families).enumerate() {
        // Reference header
        println!("\n  {}. REF: {} ({})",
            i + 1,
            family.reference_hash.truncated(12).bright_cyan(),
            family.reference_header.bright_white()
        );

        // Databases containing this reference
        println!("     {} {}",
            "├─ Databases:".bright_black(),
            family.databases.join(", ").bright_white()
        );

        // Child count
        println!("     {} {}",
            "├─ Children:".bright_black(),
            format_number(family.child_count)
        );

        // Compression ratio
        println!("     {} {:.1}x compression",
            "├─ Compression:".bright_black(),
            family.compression_ratio
        );

        // Total size
        println!("     {} {} sequences",
            "└─ Total family:".bright_black(),
            format_number(family.total_family_size)
        );
    }
}
```

### Phase 5: Export Options

```rust
// Export diff results to various formats

pub enum DiffOutputFormat {
    Text,
    Json,
    Html,
    Csv,
}

pub fn export_diff(
    analysis: &EnhancedSequenceAnalysis,
    format: DiffOutputFormat,
    output_path: &Path,
) -> Result<()> {
    match format {
        DiffOutputFormat::Text => export_text(analysis, output_path),
        DiffOutputFormat::Json => export_json(analysis, output_path),
        DiffOutputFormat::Html => export_html(analysis, output_path),
        DiffOutputFormat::Csv => export_csv(analysis, output_path),
    }
}

fn export_html(
    analysis: &EnhancedSequenceAnalysis,
    output_path: &Path,
) -> Result<()> {
    let html = format!(r#"
<!DOCTYPE html>
<html>
<head>
    <title>Database Comparison</title>
    <script src="https://cdn.plot.ly/plotly-latest.min.js"></script>
    <style>
        body {{ font-family: 'Segoe UI', sans-serif; margin: 20px; }}
        .metric {{ display: inline-block; margin: 20px; padding: 20px; border: 1px solid #ddd; }}
        .metric h3 {{ margin: 0; color: #333; }}
        .metric .value {{ font-size: 2em; color: #0066cc; }}
    </style>
</head>
<body>
    <h1>Database Comparison Results</h1>

    <div class="metrics">
        <div class="metric">
            <h3>Total Sequences A</h3>
            <div class="value">{}</div>
        </div>
        <div class="metric">
            <h3>Total Sequences B</h3>
            <div class="value">{}</div>
        </div>
        <div class="metric">
            <h3>Shared Sequences</h3>
            <div class="value">{}</div>
        </div>
        <div class="metric">
            <h3>Deduplication Savings</h3>
            <div class="value">{}</div>
        </div>
    </div>

    <div id="venn-diagram"></div>

    <script>
        // Venn diagram visualization
        var data = [{{
            type: 'scatter',
            x: [/* coordinates */],
            y: [/* coordinates */],
            mode: 'markers+text',
            text: ['Unique A', 'Shared', 'Unique B'],
            textposition: 'middle center',
            marker: {{ size: [/* sizes */] }}
        }}];

        Plotly.newPlot('venn-diagram', data);
    </script>

    <h2>Sample Shared Sequences</h2>
    <table>
        <tr><th>Hash</th><th>Header</th><th>Length</th></tr>
        {}
    </table>
</body>
</html>
    "#,
        analysis.total_sequences_a,
        analysis.total_sequences_b,
        analysis.shared_sequences,
        format_bytes(analysis.dedup_savings_bytes),
        // Sample sequences table rows
        analysis.sample_shared.iter()
            .map(|s| format!("<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                s.hash.truncated(16), s.header, s.length))
            .collect::<Vec<_>>()
            .join("\n")
    );

    std::fs::write(output_path, html)?;
    Ok(())
}
```

## Implementation Plan

### Week 1: Core Functionality
- [ ] Implement `extract_sequence_hashes()` to load ChunkManifests
- [ ] Update `compare_sequences_from_manifests()` to use actual hashes
- [ ] Add `get_sequence_samples()` for display
- [ ] Test with SwissProt vs UniRef50

### Week 2: Enhanced Analysis
- [ ] Create `EnhancedSequenceAnalysis` structure
- [ ] Add storage size estimation
- [ ] Compute deduplication savings
- [ ] Add sample sequence retrieval with headers

### Week 3: Visualization
- [ ] Implement rich terminal formatting
- [ ] Add colored tables with comfy_table
- [ ] Add interpretation messages
- [ ] Create progress indicators for large comparisons

### Week 4: Export Options
- [ ] JSON export
- [ ] HTML export with visualizations
- [ ] CSV export for analysis
- [ ] Add `--format` flag to diff command

### Future (Post-HERALD Implementation):
- [ ] Reference family analysis
- [ ] Visual reference trees
- [ ] Compression effectiveness metrics
- [ ] Cross-database reference sharing statistics

## Testing

### Test Cases:

1. **Same database, different versions**:
   ```bash
   talaria database diff uniprot/swissprot@2024-01 uniprot/swissprot@2024-02
   ```
   - Should show incremental changes
   - High sharing expected (>95%)

2. **Related databases**:
   ```bash
   talaria database diff uniprot/swissprot uniprot/uniref100
   ```
   - SwissProt should be subset of UniRef100
   - Expect 100% of SwissProt in UniRef100

3. **Unrelated databases**:
   ```bash
   talaria database diff uniprot/swissprot ncbi/nr
   ```
   - Moderate sharing (10-30%)
   - Different sources, some overlap

4. **Clustered vs unclustered**:
   ```bash
   talaria database diff uniprot/swissprot uniprot/uniref50
   ```
   - Low sharing (0-5%)
   - UniRef50 uses representatives

### Validation:

- [ ] Verify shared count by manual inspection of sample
- [ ] Check storage savings match actual deduplication
- [ ] Ensure percentages add up correctly
- [ ] Test with empty databases
- [ ] Test with identical databases (100% sharing)

## Success Criteria

1. **Accuracy**: Diff shows actual sequence-level sharing, not chunk-level
2. **Performance**: Can compare 70M sequence databases in <5 minutes
3. **Clarity**: Visualizations make sharing immediately obvious
4. **Utility**: Users understand why sharing is high/low
5. **Extensibility**: Easy to add reference analysis later

---

*Document created: 2025-10-06*
*Status: Implementation plan for improved database diff*
*Priority: High (needed for validating HERALD benefits)*
