# File Formats API Reference

Talaria supports multiple input and output file formats for biological sequence data, metadata, and configuration. This document provides comprehensive format specifications, validation rules, and usage examples for all supported formats.

## Overview

Talaria processes three main categories of files:

- **Sequence Files:** FASTA, FASTQ, and other sequence formats
- **Metadata Files:** Delta encoding, taxonomic mapping, and statistics
- **Configuration Files:** TOML configuration and validation schemas

---

## FASTA Format

### Standard FASTA

Talaria uses standard FASTA format with enhanced header parsing for biological metadata.

#### Basic Structure

```fasta
>sequence_identifier optional description
SEQUENCE_DATA_LINE_1
SEQUENCE_DATA_LINE_2
...
>next_sequence_identifier optional description
NEXT_SEQUENCE_DATA
```

#### Header Format Specifications

**Standard Headers:**
```fasta
>gi|123456|ref|NP_001234.1| hypothetical protein [Organism name]
```

**UniProt Headers:**
```fasta
>sp|P12345|PROT_HUMAN Protein name OS=Homo sapiens OX=9606 GN=GENE PE=1 SV=2
```

**Custom Headers:**
```fasta
>sequence_001|taxonomy:9606|length:254 Description of sequence function
```

#### Supported Header Patterns

Talaria automatically extracts metadata from common header formats:

| Pattern | Example | Extracted Data |
|---------|---------|----------------|
| **NCBI GenBank** | `>gi\|123\|gb\|ABC123\|` | GI number, accession |
| **NCBI RefSeq** | `>gi\|456\|ref\|NP_001234\|` | GI number, RefSeq ID |
| **UniProt SwissProt** | `>sp\|P12345\|PROT_HUMAN` | Accession, entry name |
| **UniProt TrEMBL** | `>tr\|Q67890\|Q67890_MOUSE` | Accession, entry name |
| **EMBL** | `>embl\|CAA12345\|` | EMBL accession |
| **PDB** | `>pdb\|1ABC\|A` | PDB ID, chain |

#### Taxonomy Extraction

Talaria recognizes multiple taxonomy annotation patterns:

```fasta
# NCBI taxonomy ID
>sequence_id [taxid:9606]

# UniProt organism code  
>sp|P12345|PROT_HUMAN ... OX=9606

# Custom taxonomy tags
>seq_001|taxonomy:9606|species:Homo_sapiens

# Organism name in brackets
>sequence_id hypothetical protein [Homo sapiens]
```

#### Sequence Data Rules

**Valid Characters:**
- **Proteins:** A-Z amino acid codes, X (unknown), * (stop), - (gap)
- **Nucleotides:** A, T, G, C, U, N (unknown), - (gap)  
- **Ambiguous:** IUPAC ambiguity codes (R, Y, S, W, K, M, etc.)

**Line Length:**
- Default: 80 characters per line
- Range: 50-200 characters (configurable)
- No maximum line length enforced during parsing

**Case Handling:**
- Input: Case-insensitive (converted to uppercase)
- Output: Uppercase by default (configurable)

#### Example Valid FASTA

```fasta
>sp|P12345|INSULIN_HUMAN Insulin OS=Homo sapiens OX=9606 GN=INS PE=1 SV=1
MALWMRLLPLLALLALWGPDPAAAFVNQHLCGSHLVEALYLVCGERGFFYTPKTRREAEDL
QVGQVELGGGPGAGSLQPLALEGSLQKRGIVEQCCTSICSLYQLENYCN

>gi|987654|ref|NP_000207.1| insulin [Homo sapiens]  
MALWMRLLPLLALLALWGPDPAAAFVNQHLCGSHLVEALYLVCGERGFFYTPKTRREAEDL
QVGQVELGGGPGAGSLQPLALEGSLQKRGIVEQCCTSICSLYQLENYCN
```

#### FASTA Validation

**Required Elements:**
- Header line starting with `>`
- Non-empty sequence identifier
- At least one sequence line with valid characters

**Common Errors:**
```bash
# Missing header
ATCGATCGATCG    # ERROR: No header line

# Empty identifier  
> description only    # ERROR: No sequence ID

# Invalid characters
>seq1
ATCGXYZ123    # ERROR: Invalid nucleotide characters

# Mixed sequence types in same file
>prot1
MALW...       # Protein sequence
>nucl1  
ATCG...       # ERROR: Mixed protein/nucleotide
```

#### FASTA Performance Optimizations

**Memory-Mapped Parsing:**
- Files >100MB automatically use memory mapping
- Reduces memory usage for large files
- Faster random access to sequences

**Parallel Processing:**
- Large files split into chunks for parallel parsing
- Chunk boundaries respect sequence boundaries
- Configurable chunk size (default: 10K sequences)

---

## Delta File Format

### Delta Metadata Format (.dat)

Delta files store compressed representations of sequences similar to reference sequences. This format enables efficient storage and reconstruction of large sequence databases.

#### File Structure

```
# Talaria Delta Format v1.0
# Reference: reference_sequence_id
# Target: target_sequence_id  
# Distance: edit_distance
# Operations: insertion(I), deletion(D), substitution(S), match(M)

reference_id    target_id    edit_distance    operations
seq_ref_001     seq_del_002  5               3M,1I,2M,1D,1S,10M
seq_ref_001     seq_del_003  8               1S,15M,2I,1D,5M  
seq_ref_004     seq_del_005  12              2M,3D,1I,8M,1S,4M
```

#### Delta Operations Format

Operations are encoded as comma-separated tuples:

| Operation | Format | Description | Example |
|-----------|---------|-------------|---------|
| **Match** | `nM` | n identical characters | `10M` = 10 matches |
| **Substitution** | `nS` | n substitutions | `2S` = 2 substitutions |
| **Insertion** | `nI` | n insertions in target | `3I` = insert 3 chars |
| **Deletion** | `nD` | n deletions from reference | `1D` = delete 1 char |

#### Detailed Delta Format

For complex delta encoding with actual sequence data:

```
# Extended Delta Format
reference_id:seq_ref_001
target_id:seq_del_002
reference_length:245
target_length:248
edit_distance:5
operations:
  3M    # Positions 1-3 match
  1I:T  # Insert T at position 4
  2M    # Positions 4-5 match (in reference)
  1D    # Delete position 6 from reference  
  1S:A>G # Substitute A with G at position 7
  10M   # Positions 8-17 match
---
```

#### Delta File Validation

**Consistency Checks:**
- Edit distance matches operation count
- All referenced sequences exist
- Operations don't exceed sequence boundaries

**Common Errors:**
```bash
# Inconsistent edit distance
seq_ref_001  seq_del_002  5  3M,1I,2M,1D,1S,10M,2I  # ERROR: Distance=5, actual=8

# Missing reference
missing_ref  seq_del_002  3  1M,1I,1M  # ERROR: Reference not found

# Invalid operations  
seq_ref_001  seq_del_002  2  5M,3X,1M  # ERROR: Unknown operation 'X'
```

#### Delta Reconstruction Algorithm

1. **Load Reference:** Read reference sequence into memory
2. **Parse Operations:** Split operation string by commas
3. **Apply Operations:** Process each operation sequentially
4. **Validate Result:** Check final sequence length and consistency

```python
def reconstruct_sequence(reference_seq, operations):
    result = []
    ref_pos = 0
    
    for op in operations.split(','):
        if op.endswith('M'):  # Match
            count = int(op[:-1])
            result.extend(reference_seq[ref_pos:ref_pos+count])
            ref_pos += count
        elif op.endswith('I'):  # Insertion
            # Insert from operation or separate data
            pass
        # ... handle other operations
    
    return ''.join(result)
```

---

## Reference-to-Children Mapping (.ref2child)

### Format Specification

Maps reference sequences to their derived (child) sequences for efficient lookup during reconstruction.

```
# Reference-to-children mapping
# Format: reference_id<TAB>child_id1<TAB>child_id2<TAB>...

sp|P12345|INSULIN_HUMAN	sp|P12346|INSULIN_RAT	sp|P12347|INSULIN_MOUSE	tr|Q12345|INSULIN_CHIMP
gi|123456|ref|NP_001234	gi|123457|ref|NP_001235	gi|123458|ref|NP_001236
seq_reference_001	seq_delta_002	seq_delta_003	seq_delta_004	seq_delta_005
```

#### File Structure Rules

- **Delimiter:** Tab character (`\t`) 
- **First Column:** Reference sequence identifier
- **Subsequent Columns:** Child sequence identifiers (space-separated if multiple per column)
- **Comments:** Lines starting with `#` are ignored
- **Empty Lines:** Ignored

#### Usage Examples

```bash
# Create reference mapping
talaria reduce -i input.fasta -o ref.fasta --ref2child mapping.ref2child

# Use mapping for reconstruction
talaria reconstruct -r ref.fasta -d deltas.dat --mapping mapping.ref2child
```

---

## Taxonomic Data Formats

### NCBI Taxonomy Format

Talaria can import and use NCBI taxonomy data for taxonomy-aware reduction.

#### nodes.dmp Format

Standard NCBI taxonomy nodes format:

```
# Format: tax_id | parent_tax_id | rank | embl_code | ...
1	1	no rank	-	8	0	1	0	0	1	0	0		
2	131567	superkingdom	-	0	0	11	0	0	1	0	0		
6	335928	genus	-	0	1	11	1	0	1	1	0		
9	32199	species	-	0	1	11	1	0	1	1	0		
```

#### names.dmp Format  

Taxonomy names and classifications:

```
# Format: tax_id | name_txt | unique_name | name_class
1	all	-	synonym
1	root	-	scientific name  
2	Bacteria	Bacteria <prokaryote>	scientific name
2	bacteria	-	genbank common name
```

#### Custom Taxonomy Format

Simplified taxonomy format for custom databases:

```toml
# taxonomy.toml
[taxa]
9606 = { name = "Homo sapiens", rank = "species", parent = 9605 }
9605 = { name = "Homo", rank = "genus", parent = 9604 }
9604 = { name = "Hominidae", rank = "family", parent = 314146 }
```

---

## Statistics and Report Formats

### JSON Statistics Format

Comprehensive statistics output in machine-readable JSON:

```json
{
  "file_info": {
    "filename": "database.fasta",
    "file_size": 1024000000,
    "parsed_at": "2024-01-15T10:30:00Z",
    "format": "fasta"
  },
  "sequence_metrics": {
    "total_sequences": 1500000,
    "total_length": 750000000,
    "average_length": 500.0,
    "median_length": 425,
    "min_length": 50,
    "max_length": 35000,
    "n50": 680,
    "n90": 1200,
    "length_distribution": {
      "0-100": 50000,
      "101-500": 800000,  
      "501-1000": 450000,
      "1001+": 200000
    }
  },
  "composition_analysis": {
    "sequence_type": "protein",
    "amino_acid_frequencies": {
      "A": 8.2, "R": 5.1, "N": 4.3, "D": 5.5,
      "C": 1.4, "Q": 3.9, "E": 6.7, "G": 7.1
    },
    "low_complexity_percentage": 12.5,
    "ambiguous_residues": 1250
  },
  "complexity_metrics": {
    "shannon_entropy": 1.85,
    "simpson_diversity": 0.92,
    "sequence_diversity": 0.875
  },
  "reduction_statistics": {
    "original_sequences": 1500000,
    "reference_sequences": 450000,
    "delta_encoded_sequences": 1050000,
    "compression_ratio": 0.30,
    "space_savings": 3.33,
    "taxonomic_coverage": 0.98
  }
}
```

### CSV Statistics Format

Tabular format for spreadsheet analysis:

```csv
metric,value,unit,description
total_sequences,1500000,count,Total number of sequences
total_length,750000000,bp,Total sequence length
average_length,500.0,bp,Mean sequence length
median_length,425,bp,Median sequence length
min_length,50,bp,Shortest sequence length
max_length,35000,bp,Longest sequence length
n50,680,bp,N50 assembly statistic
n90,1200,bp,N90 assembly statistic
gc_content,42.5,percent,GC content (nucleotides only)
shannon_entropy,1.85,bits,Sequence complexity measure
compression_ratio,0.30,ratio,Reduction compression ratio
taxonomic_coverage,0.98,fraction,Preserved taxonomic diversity
```

### HTML Report Format

Rich HTML reports with interactive visualizations:

```html
<!DOCTYPE html>
<html>
<head>
    <title>Talaria Analysis Report</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>/* Embedded CSS styles */</style>
</head>
<body>
    <h1>FASTA Analysis Report</h1>
    
    <div class="summary-section">
        <h2>● Summary Statistics</h2>
        <table class="stats-table">
            <tr><td>Total Sequences</td><td>1,500,000</td></tr>
            <tr><td>Total Length</td><td>750 Mbp</td></tr>
            <tr><td>Average Length</td><td>500 bp</td></tr>
        </table>
    </div>
    
    <div class="visualization-section">  
        <h2>▶ Length Distribution</h2>
        <div id="length-histogram"></div>
        <script>/* D3.js visualization code */</script>
    </div>
    
    <div class="reduction-section">
        <h2>■ Reduction Analysis</h2>
        <div class="reduction-metrics">
            <div class="metric">
                <span class="label">Compression Ratio</span>
                <span class="value">30%</span>
            </div>
        </div>
    </div>
</body>
</html>
```

---

## Configuration File Formats

### TOML Configuration

Primary configuration format using TOML (Tom's Obvious, Minimal Language):

```toml
# Talaria Configuration File
# https://toml.io/en/

[reduction]
target_ratio = 0.3
min_sequence_length = 50
max_delta_distance = 100
similarity_threshold = 0.9
taxonomy_aware = true

[alignment]
gap_penalty = -11
gap_extension = -1  
algorithm = "needleman-wunsch"

# Scoring matrix (optional)
[alignment.matrix]
type = "BLOSUM62"

[output]
format = "fasta"
include_metadata = true
compress_output = false
line_length = 80
header_format = "standard"

[performance]
chunk_size = 10000
batch_size = 1000
cache_alignments = true
parallel_io = true
memory_limit = "auto"
temp_directory = "/tmp/talaria"
```

### YAML Configuration (Alternative)

Alternative YAML format for configuration:

```yaml
# Talaria Configuration (YAML)
reduction:
  target_ratio: 0.3
  min_sequence_length: 50
  max_delta_distance: 100
  similarity_threshold: 0.9
  taxonomy_aware: true

alignment:
  gap_penalty: -11
  gap_extension: -1
  algorithm: needleman-wunsch
  matrix:
    type: BLOSUM62

output:
  format: fasta
  include_metadata: true
  compress_output: false
  line_length: 80
  header_format: standard

performance:
  chunk_size: 10000
  batch_size: 1000
  cache_alignments: true
  parallel_io: true
  memory_limit: auto
  temp_directory: /tmp/talaria
```

### JSON Schema for Validation

Configuration validation schema:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Talaria Configuration Schema",
  "type": "object",
  "properties": {
    "reduction": {
      "type": "object",
      "properties": {
        "target_ratio": {
          "type": "number",
          "minimum": 0.0,
          "maximum": 1.0
        },
        "min_sequence_length": {
          "type": "integer",
          "minimum": 1
        },
        "similarity_threshold": {
          "type": "number",
          "minimum": 0.0,
          "maximum": 1.0
        },
        "taxonomy_aware": {
          "type": "boolean"
        }
      },
      "required": ["target_ratio"],
      "additionalProperties": false
    }
  }
}
```

---

## Compressed File Support

### Automatic Compression Detection

Talaria automatically detects and handles compressed files:

| Extension | Format | Compression |
|-----------|---------|-------------|
| `.fasta` | FASTA | None |
| `.fasta.gz` | FASTA | Gzip |
| `.fasta.bz2` | FASTA | Bzip2 |
| `.fasta.xz` | FASTA | XZ/LZMA |
| `.fa.gz` | FASTA | Gzip |

### Compression Examples

```bash
# Input automatically decompressed
talaria reduce -i database.fasta.gz -o reduced.fasta

# Output automatically compressed (with config)
talaria reduce -i input.fasta -o output.fasta.gz --compress

# Mixed compression formats
talaria reduce -i input.fasta.bz2 -o output.fasta.xz
```

### Performance Considerations

- **Gzip:** Fast decompression, good compression ratio
- **Bzip2:** Slower, better compression ratio  
- **XZ/LZMA:** Slowest, best compression ratio
- **Automatic:** Based on available CPU cores and I/O speed

---

## Format Validation and Error Handling

### Input Validation

Talaria performs comprehensive format validation:

```bash
# Validate FASTA format
talaria validate-format --input sequences.fasta --format fasta

# Check for common issues
talaria validate-format --input sequences.fasta --strict --report issues.json
```

### Common Format Errors

#### FASTA Format Errors

```bash
# Error: Missing sequence data
>sequence_id_without_data

# Error: Invalid characters
>seq1  
ATCGXYZ123

# Error: Truncated file
>seq1
ATCGATCG
>seq2
ATCG[EOF - file truncated]
```

#### Delta Format Errors

```bash  
# Error: Malformed operations
seq_ref seq_tgt 5 3M,1Z,2M  # Unknown operation 'Z'

# Error: Inconsistent distances  
seq_ref seq_tgt 3 1M,1I,1D,1S,1M  # Distance=3, actual=4

# Error: Missing reference
missing_ref seq_tgt 2 1M,1I  # Reference 'missing_ref' not found
```

#### Configuration Format Errors

```toml
# Error: Invalid TOML syntax
[reduction]
target_ratio = 0.3
invalid syntax here

# Error: Out of range values
[reduction]
target_ratio = 1.5  # Must be ≤ 1.0

# Error: Type mismatch
[performance]  
chunk_size = "invalid"  # Must be integer
```

### Error Recovery

Talaria includes error recovery mechanisms:

- **Partial parsing:** Continue processing valid sequences
- **Format auto-detection:** Try alternative parsers
- **Validation warnings:** Non-fatal issues reported
- **Repair suggestions:** Automatic fixes for common problems

---

## Format Conversion

### Built-in Converters

Convert between supported formats:

```bash
# FASTA to FASTQ (with quality scores)
talaria convert --input seqs.fasta --output seqs.fastq --format fastq --quality-default 40

# Add metadata to headers
talaria convert --input basic.fasta --output annotated.fasta --add-taxonomy --add-length

# Change line length
talaria convert --input input.fasta --output output.fasta --line-length 60

# Compress output
talaria convert --input input.fasta --output output.fasta.gz --compress
```

### Custom Format Support

Extend Talaria with custom format plugins:

```toml
# Add custom format plugin
[plugins]
enabled = ["custom_format_parser"]

[plugins.custom_format_parser]
name = "phylip_parser"
input_extensions = [".phy", ".phylip"]
output_extensions = [".phy"]
```

---

## Performance and Optimization

### Large File Handling

Optimizations for processing large sequence databases:

#### Memory Management
- **Streaming:** Process sequences without loading entire file
- **Memory mapping:** Virtual memory for random access
- **Chunking:** Split large files into manageable pieces
- **Compression:** On-the-fly decompression

#### Parallel Processing  
- **Multi-threaded parsing:** Parse multiple chunks simultaneously
- **Parallel I/O:** Overlapped reading and processing
- **NUMA awareness:** Optimize for multi-socket systems

### Format-Specific Optimizations

#### FASTA Optimization
```bash
# Use memory mapping for files >100MB
talaria reduce --mmap --input large.fasta --output reduced.fasta

# Parallel parsing with custom chunk size
talaria reduce --chunk-size 50000 --input huge.fasta --output reduced.fasta

# Disable validation for trusted files
talaria reduce --no-validation --input trusted.fasta --output reduced.fasta
```

#### Delta Optimization
```bash
# Use binary delta format for speed
talaria reduce --delta-format binary --metadata deltas.bin

# Compress delta files
talaria reduce --compress-deltas --metadata deltas.dat.gz
```

---

## Best Practices

### File Organization

```bash
# Recommended project structure
project/
├── input/
│   ├── original.fasta.gz      # Original data (compressed)
│   └── taxonomy.dat           # Taxonomy mapping
├── reduced/
│   ├── references.fasta       # Reference sequences
│   ├── deltas.dat             # Delta encodings  
│   └── mapping.ref2child      # Reference mapping
├── config/
│   ├── production.toml        # Production config
│   └── test.toml             # Testing config
└── output/
    ├── stats.json            # Analysis statistics
    └── report.html           # HTML report
```

### Naming Conventions

- **Sequence Files:** `database_version_type.format`
  - `uniprot_2024_01_swissprot.fasta.gz`
  - `ncbi_nr_2024_02_proteins.fasta.gz`

- **Metadata Files:** `database_version_metadata.format`
  - `uniprot_2024_01_deltas.dat`
  - `uniprot_2024_01_taxonomy.tsv`

- **Configuration Files:** `purpose_settings.toml`
  - `lambda_aggressive.toml`
  - `blast_conservative.toml`

### Validation Workflow

```bash
# 1. Validate input format
talaria validate-format --input raw_data.fasta --strict

# 2. Check sequence quality  
talaria stats --input raw_data.fasta --format json > quality_check.json

# 3. Test configuration
talaria reduce --config test.toml --dry-run --input sample.fasta

# 4. Process with validation
talaria reduce --config production.toml --validate --input raw_data.fasta --output reduced.fasta

# 5. Verify output integrity
talaria validate --original raw_data.fasta --reduced reduced.fasta --deltas deltas.dat
```

### Backup and Recovery

- **Atomic operations:** Temporary files renamed on completion
- **Checksum validation:** Verify file integrity
- **Incremental processing:** Resume interrupted operations
- **Metadata preservation:** Maintain provenance information

---

## Troubleshooting Formats

### Common Issues and Solutions

#### Memory Issues with Large Files
```bash
# Problem: Out of memory with huge FASTA file
# Solution: Use streaming mode
talaria reduce --stream --chunk-size 5000 --input huge.fasta

# Problem: Delta reconstruction uses too much RAM  
# Solution: Process in batches
talaria reconstruct --batch-size 1000 --r refs.fasta --d deltas.dat
```

#### Format Detection Issues
```bash
# Problem: Format not auto-detected
# Solution: Specify format explicitly  
talaria reduce --input-format fasta --input ambiguous_file

# Problem: Compressed file not recognized
# Solution: Check file extensions and magic numbers
file suspicious_file.fasta
hexdump -C suspicious_file.fasta | head
```

#### Character Encoding Issues
```bash
# Problem: Non-ASCII characters in sequence
# Solution: Clean and validate input
talaria convert --input messy.fasta --output clean.fasta --ascii-only --validate

# Problem: Mixed line endings (Windows/Unix)
# Solution: Normalize line endings
dos2unix input.fasta
```

### Debug Mode

Enable detailed format debugging:

```bash
# Show format detection process
TALARIA_LOG=debug talaria reduce --input unknown_format.file

# Validate specific format components
talaria debug --check-headers --check-sequences --input sequences.fasta

# Export parsing internals
talaria debug --dump-parser-state --input problematic.fasta > debug.json
```

### Format Migration

When upgrading between Talaria versions:

```bash
# Check format compatibility
talaria check-compatibility --input old_deltas.dat --version 0.2

# Migrate to new format
talaria migrate --input old_format.dat --output new_format.dat --from v0.1 --to v0.2

# Validate migration
talaria validate --original old_format.dat --migrated new_format.dat
```
