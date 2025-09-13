# Command Line Interface API Reference

Talaria provides a comprehensive command-line interface for intelligent FASTA reduction and bioinformatics processing. This document provides complete API reference for all commands, options, and usage patterns.

## Global Options

These options are available for all commands and control global behavior:

### `-v, --verbose`
**Type:** Flag (repeatable)  
**Default:** None  
**Description:** Increase verbosity level. Can be repeated multiple times for more detailed output.

```bash
talaria -v reduce ...           # Basic verbose output
talaria -vv reduce ...          # More detailed output  
talaria -vvv reduce ...         # Debug-level output
```

### `-j, --threads <NUMBER>`
**Type:** Integer  
**Default:** `0` (auto-detect all available cores)  
**Description:** Number of threads to use for parallel processing.

```bash
talaria -j 4 reduce ...         # Use 4 threads
talaria -j 0 reduce ...         # Use all available cores
```

---

## Database Reference Format

Many commands support database references for working with stored databases:

```
source/dataset[@version][:profile]
```

**Components:**
- `source`: Database source (e.g., `uniprot`, `ncbi`)
- `dataset`: Dataset name (e.g., `swissprot`, `nr`)
- `@version`: Optional version (e.g., `@2024-01-01`, default: `current`)
- `:profile`: Reduction profile (e.g., `:blast-30`, required for validate/reconstruct)

**Examples:**
- `uniprot/swissprot` - Current version of SwissProt
- `uniprot/swissprot@2024-01-01` - Specific version
- `uniprot/swissprot:blast-30` - Reduction profile
- `ncbi/nr@2024-01-01:fast-25` - Specific version's reduction

---

## Commands

### reduce

Intelligently reduce a FASTA file for optimal aligner indexing by selecting representative sequences and encoding similar sequences as deltas.

#### Usage
```bash
# Database approach (automatically stores result)
talaria reduce [DATABASE] [OPTIONS]

# File-based approach (traditional)
talaria reduce -i <INPUT> -o <OUTPUT> [OPTIONS]
```

#### Positional Arguments

**`[DATABASE]`** (Optional)  
Database to reduce (e.g., `uniprot/swissprot`, `ncbi/nr@2024-01-01`).  
When specified, automatically stores result in database structure.

#### File-based Arguments

**`-i, --input <FILE>`**  
Path to input FASTA file (required if DATABASE not specified).

**`-o, --output <FILE>`**  
Path for output reduced FASTA file (required if DATABASE not specified and --store not used).

#### Core Options

**`-a, --target-aligner <ALIGNER>`**  
**Default:** `generic`  
**Values:** `lambda`, `blast`, `kraken`, `diamond`, `mmseqs2`, `generic`  
Target aligner for optimization.

**`-r, --reduction-ratio <RATIO>`**  
**Type:** Float (0.0-1.0)  
**Default:** `0.3`  
Target reduction ratio where 0.3 means 30% of original size.

**`--profile <NAME>`**  
Profile name for stored reduction (e.g., `blast-optimized`).  
Default: auto-generated from ratio (e.g., `30-percent`).

**`--store`**  
Store reduced version in database structure (only needed with `-i`).

**`--min-length <LENGTH>`**  
**Type:** Integer  
**Default:** `50`  
Minimum sequence length to consider.

**`-m, --metadata <FILE>`**  
Output path for delta metadata file.

**`-c, --config <FILE>`**  
Path to TOML configuration file.

#### Advanced Options

**`--similarity-threshold <THRESHOLD>`**  
Enable similarity-based clustering (0.0-1.0).

**`--low-complexity-filter`**  
Filter out low-complexity sequences.

**`--align-select`**  
Use alignment-based selection.

**`--taxonomy-aware`**  
Consider taxonomic IDs when selecting references.

**`--no-deltas`**  
Skip delta encoding (faster, no reconstruction).

**`--max-align-length <LENGTH>`**  
Maximum sequence length for alignment (default: 10000).

#### Examples

##### Database-based Reduction (NEW)
```bash
# Reduce stored database with auto-storage
talaria reduce uniprot/swissprot --profile blast-30 -r 0.3

# Reduce specific version
talaria reduce uniprot/swissprot@2024-01-01 --profile old-blast -r 0.3

# Further reduce existing reduction
talaria reduce uniprot/swissprot:blast-30 --profile ultra-fast -r 0.1

# Use custom aligner optimization
talaria reduce ncbi/nr --profile diamond-optimized -a diamond -r 0.25
```

##### File-based Reduction (Traditional)
```bash
# Simple reduction
talaria reduce -i database.fasta -o reduced.fasta

# With metadata for reconstruction
talaria reduce -i input.fasta -o output.fasta -m deltas.tal -r 0.3

# Store external file in database structure
talaria reduce -i external.fasta -o /tmp/out.fasta --store --profile custom
```

---

### validate

Validate reduction quality by comparing original, reduced, and delta files.

#### Usage
```bash
# Database approach
talaria validate DATABASE:PROFILE [OPTIONS]

# File-based approach
talaria validate -o <ORIGINAL> -r <REDUCED> -d <DELTAS> [OPTIONS]
```

#### Positional Arguments

**`[DATABASE:PROFILE]`** (Optional)  
Database reduction to validate (e.g., `uniprot/swissprot:blast-30`).  
Profile is required for validation.

#### File-based Arguments

**`-o, --original <FILE>`**  
Original FASTA file (required if DATABASE:PROFILE not specified).

**`-r, --reduced <FILE>`**  
Reduced FASTA file (required if DATABASE:PROFILE not specified).

**`-d, --deltas <FILE>`**  
Delta metadata file (required if DATABASE:PROFILE not specified).

#### Optional Arguments

**`--original-results <FILE>`**  
Alignment results from original (for comparison).

**`--reduced-results <FILE>`**  
Alignment results from reduced (for comparison).

**`--report <FILE>`**  
Output validation report in JSON format.

#### Examples

##### Database-based Validation (NEW)
```bash
# Validate stored reduction
talaria validate uniprot/swissprot:blast-30

# Validate specific version's reduction
talaria validate uniprot/swissprot@2024-01-01:blast-30

# With alignment comparison
talaria validate ncbi/nr:fast-25 \
    --original-results orig.m8 \
    --reduced-results red.m8 \
    --report validation.json
```

##### File-based Validation (Traditional)
```bash
# Basic validation
talaria validate -o original.fasta -r reduced.fasta -d deltas.tal

# With detailed report
talaria validate \
    -o orig.fasta \
    -r red.fasta \
    -d deltas.tal \
    --report validation_report.json
```

---

### reconstruct

Reconstruct original sequences from reference sequences and delta metadata.

#### Usage
```bash
# Database approach
talaria reconstruct DATABASE:PROFILE [OPTIONS]

# File-based approach
talaria reconstruct -r <REFERENCES> -d <DELTAS> [OPTIONS]
```

#### Positional Arguments

**`[DATABASE:PROFILE]`** (Optional)  
Database reduction to reconstruct (e.g., `uniprot/swissprot:blast-30`).  
Profile is required for reconstruction.

#### File-based Arguments

**`-r, --references <FILE>`**  
Reference FASTA file (required if DATABASE:PROFILE not specified).

**`-d, --deltas <FILE>`**  
Delta metadata file (required if DATABASE:PROFILE not specified).

#### Optional Arguments

**`-o, --output <FILE>`**  
Output reconstructed FASTA file.  
Default: auto-generated based on input.

**`--sequences <IDS>`**  
Only reconstruct specific sequences (comma-separated IDs).

#### Examples

##### Database-based Reconstruction (NEW)
```bash
# Reconstruct all sequences (auto-generates output name)
talaria reconstruct uniprot/swissprot:blast-30

# Specify output file
talaria reconstruct uniprot/swissprot:blast-30 -o reconstructed.fasta

# Reconstruct specific sequences
talaria reconstruct ncbi/nr:fast-25 --sequences P12345,Q67890

# From specific version
talaria reconstruct uniprot/swissprot@2024-01-01:blast-30
```

##### File-based Reconstruction (Traditional)
```bash
# Basic reconstruction
talaria reconstruct -r refs.fasta -d deltas.tal -o output.fasta

# Auto-generate output name
talaria reconstruct -r refs.fasta -d deltas.tal

# Selective reconstruction
talaria reconstruct -r refs.fasta -d deltas.tal --sequences ID1,ID2
```

---

### database

Manage biological sequence databases with versioning and reductions.

#### Subcommands

##### database download

Download databases from supported sources.

```bash
talaria database download [OPTIONS]
```

**Options:**
- `--database <SOURCE>`: Database source (`uniprot`, `ncbi`)
- `-d, --dataset <NAME>`: Dataset to download
- `-o, --output <DIR>`: Output directory (default: centralized)
- `-r, --resume`: Resume incomplete download
- `--skip-verify`: Skip checksum verification
- `--list-datasets`: List available datasets

**Examples:**
```bash
# Download UniProt SwissProt
talaria database download --database uniprot -d swissprot

# Download NCBI NR with resume
talaria database download --database ncbi -d nr --resume

# List available datasets
talaria database download --list-datasets
```

##### database list

List stored databases and their reductions.

```bash
talaria database list [OPTIONS]
```

**Options:**
- `--show-reduced`: Show reduced versions
- `--detailed`: Show detailed information
- `--all-versions`: Show all versions (not just current)
- `--database <REF>`: Specific database to list
- `--sort <FIELD>`: Sort by field (`name`, `size`, `date`)

**Examples:**
```bash
# List all databases
talaria database list

# Show with reductions
talaria database list --show-reduced

# Detailed view of specific database
talaria database list --database uniprot/swissprot --detailed --all-versions
```

##### database diff

Compare database versions or reductions.

```bash
talaria database diff <OLD> [NEW] [OPTIONS]
```

**Arguments:**
- `<OLD>`: First database reference
- `[NEW]`: Second database reference (optional, compares with previous if omitted)

**Options:**
- `-o, --output <FILE>`: Output report file
- `--format <FORMAT>`: Report format (`text`, `html`, `json`, `csv`)
- `--detailed`: Show detailed sequence changes
- `--headers-only`: Compare only headers (fast)
- `--similarity-threshold <RATIO>`: Threshold for modified sequences

**Examples:**
```bash
# Compare with previous version
talaria database diff uniprot/swissprot

# Compare specific versions
talaria database diff uniprot/swissprot@2024-01-01 uniprot/swissprot@2024-02-01

# Compare reductions
talaria database diff uniprot/swissprot:blast-30 uniprot/swissprot:diamond-40

# Generate HTML report
talaria database diff ncbi/nr --format html --visual -o changes.html
```

##### database clean

Clean old database versions.

```bash
talaria database clean [DATABASE] [OPTIONS]
```

**Options:**
- `--keep <COUNT>`: Number of versions to keep (default: 3)
- `--all`: Clean all databases
- `--dry-run`: Show what would be deleted

**Examples:**
```bash
# Clean old versions of specific database
talaria database clean uniprot/swissprot

# Keep 5 versions
talaria database clean uniprot/swissprot --keep 5

# Clean all databases
talaria database clean --all
```

---

### search

Search sequences against a database using various aligners.

#### Usage
```bash
talaria search -d <DATABASE> -q <QUERY> [OPTIONS]
```

#### Required Arguments

**`-d, --database <FILE>`**  
Path to database file (can be reduced FASTA).

**`-q, --query <FILE>`**  
Path to query FASTA file.

#### Optional Arguments

**`-a, --aligner <ALIGNER>`**  
**Default:** `auto`  
**Values:** `lambda`, `blast`, `kraken`, `diamond`, `mmseqs2`, `auto`  
Aligner to use for search.

**`-o, --output <FILE>`**  
Output file for results (default: stdout).

**`--threads <NUMBER>`**  
Number of threads for alignment.

**`--evalue <NUMBER>`**  
E-value threshold (default: 0.001).

**`--max-target-seqs <NUMBER>`**  
Maximum number of target sequences (default: 10).

#### Examples

```bash
# Search with auto-detected aligner
talaria search -d reduced.fasta -q queries.fasta

# Use specific aligner with parameters
talaria search \
    -d nr_reduced.fasta \
    -q proteins.fasta \
    -a blast \
    --evalue 1e-10 \
    --max-target-seqs 100 \
    -o results.txt

# Search against stored database
talaria search \
    -d ~/.talaria/databases/data/uniprot/swissprot/current/reduced/blast-30/swissprot.fasta \
    -q query.fasta
```

---

### stats

Display statistics about FASTA files or reductions.

#### Usage
```bash
talaria stats <FILE> [OPTIONS]
```

#### Arguments

**`<FILE>`**  
Path to FASTA file or delta metadata file.

#### Options

**`--detailed`**  
Show detailed per-sequence statistics.

**`--format <FORMAT>`**  
Output format (`text`, `json`, `csv`).

#### Examples

```bash
# Basic statistics
talaria stats database.fasta

# Detailed analysis
talaria stats reduced.fasta --detailed

# JSON output for processing
talaria stats deltas.tal --format json
```

---

### interactive

Launch interactive mode for guided workflows.

#### Usage
```bash
talaria interactive
```

This launches a menu-driven interface for:
- Database downloads
- Reduction workflows
- Validation and testing
- Configuration management

---

## Configuration

Talaria uses TOML configuration files for advanced settings.

### Default Configuration Location
- `~/.talaria/config.toml` (user)
- `./talaria.toml` (project)

### Configuration Structure

```toml
[database]
database_dir = "~/.talaria/databases/data"
retention_count = 3

[reduction]
min_sequence_length = 50
similarity_threshold = 0.0
taxonomy_aware = false

[aligners.blast]
path = "/usr/bin/blastp"
default_evalue = 0.001
default_max_target_seqs = 10

[performance]
max_memory_gb = 8
parallel_io = true
compression_level = 6
```

### Environment Variables

- `TALARIA_CONFIG`: Path to configuration file
- `TALARIA_DATABASE_DIR`: Override database directory
- `TALARIA_THREADS`: Default thread count
- `TALARIA_LOG`: Log level (`error`, `warn`, `info`, `debug`, `trace`)

---

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: Invalid arguments
- `3`: File not found
- `4`: Permission denied
- `5`: Out of memory
- `10`: Validation failed
- `11`: Reconstruction failed

---

## Performance Tips

1. **Use appropriate thread counts**: `-j 0` uses all cores
2. **Skip validation for speed**: `--skip-validation`
3. **Use `--no-deltas` for one-way reduction**
4. **Adjust `--max-align-length` for long sequences**
5. **Use stored databases to avoid repeated file I/O**
6. **Profile-specific reductions for different aligners**

---

## See Also

- [Configuration Guide](../user-guide/configuration.md)
- [Basic Usage](../user-guide/basic-usage.md)
- [Database Management](../databases/downloading.md)
- [Workflow Examples](../workflows/)