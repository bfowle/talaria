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

### Format
```
source/dataset[@version][:profile]
```

### Components
- **source**: Database source (e.g., `uniprot`, `ncbi`)
- **dataset**: Dataset name (e.g., `swissprot`, `nr`)
- **@version**: Optional version (e.g., `@2024-01-01`, `@current`)
- **:profile**: Reduction profile (e.g., `:blast-30`, `:30-percent`)

### Examples
```bash
uniprot/swissprot                  # Current version
uniprot/swissprot@2024-01-01      # Specific version
uniprot/swissprot:blast-30        # Reduced version
uniprot/swissprot@2024-01-01:blast-30  # Specific version's reduction
```

---

## Commands

### database

Manage biological sequence databases with HERALD (Sequence Query Optimization with Indexed Architecture) for efficient storage and updates.

#### Usage
```bash
talaria database <SUBCOMMAND> [OPTIONS]
```

#### Subcommands

##### database download

Download a new database using HERALD.

```bash
talaria database download <SOURCE> [OPTIONS]
```

**Arguments:**
- `<SOURCE>`: Database source (e.g., "uniprot", "ncbi")

**Options:**
- `-d, --dataset <NAME>`: Dataset name (e.g., "swissprot", "nr")
- `--taxonomy`: Download taxonomy data

**Example:**
```bash
talaria database download uniprot -d swissprot
```

##### database update

Check for and download database updates.

```bash
talaria database update <DATABASE> [OPTIONS]
```

**Arguments:**
- `<DATABASE>`: Database reference (e.g., "uniprot/swissprot")

**Options:**
- `--download`: Download updates if available
- `--force`: Force update even if recent

**Example:**
```bash
talaria database update uniprot/swissprot --download
```

##### database list

List downloaded databases.

```bash
talaria database list [OPTIONS]
```

**Options:**
- `--detailed`: Show detailed information
- `--all-versions`: Show all versions, not just current

**Example:**
```bash
talaria database list
```

##### database add

Add a custom FASTA file to HERALD.

```bash
talaria database add [OPTIONS]
```

**Options:**
- `--source <NAME>`: Source name for the database
- `--dataset <NAME>`: Dataset name
- `--input <PATH>`: Path to FASTA file
- `--version <VERSION>`: Version string
- `--description <TEXT>`: Database description
- `--replace`: Replace existing database
- `--copy`: Keep original file (don't move)

**Example:**
```bash
talaria database add --source mylab --dataset proteins --input sequences.fasta
```

##### database list-sequences

List sequences from a HERALD database.

```bash
talaria database list-sequences <DATABASE> [OPTIONS]
```

**Arguments:**
- `<DATABASE>`: Database reference (e.g., "uniprot/swissprot")

**Options:**
- `--format <FORMAT>`: Output format (text, json, tsv, fasta)
- `--output <PATH>`: Output file path
- `--limit <N>`: Maximum number of sequences
- `--filter <TEXT>`: Filter sequences by ID
- `--ids-only`: Output only sequence IDs
- `--full`: Include all metadata

**Example:**
```bash
talaria database list-sequences uniprot/swissprot --format json -o sequences.json
```
- `--aggressive`: Remove all unreferenced chunks

**Example:**
```bash
talaria herald gc --dry-run
```

##### herald diff

Compare two HERALD database versions.

```bash
talaria herald diff <OLD> <NEW> [OPTIONS]
```

**Arguments:**
- `<OLD>`: First database version
- `<NEW>`: Second database version

**Options:**
- `--detailed`: Show chunk-level differences
- `--output <FILE>`: Save diff to file

**Example:**
```bash
talaria herald diff uniprot/swissprot@2024-01 uniprot/swissprot@2024-02
```

---

### reduce

Reduce biological sequence databases for optimal aligner performance.

#### Usage
```bash
talaria reduce [OPTIONS]
```

**Options:**
- `-i, --input <PATH>`: Input FASTA file or database reference
- `-o, --output <PATH>`: Output FASTA file
- `-d, --database <DB>`: Use HERALD database as input
- `-r, --reduction-ratio <RATIO>`: Target reduction ratio (0.0-1.0)
- `-a, --target-aligner <ALIGNER>`: Target aligner (blast, diamond, lambda, etc.)
- `--profile <NAME>`: Name for this reduction profile
- `--min-length <N>`: Minimum sequence length
- `--no-deltas`: Don't generate delta files
- `--skip-validation`: Skip validation checks

**Examples:**
```bash
# Reduce from file
talaria reduce -i input.fasta -o reduced.fasta -r 0.3

# Reduce from HERALD database
talaria reduce -d uniprot/swissprot -o reduced.fasta --profile blast-30 -r 0.3

# Optimize for specific aligner
talaria reduce -d ncbi/nr -o nr_diamond.fasta -a diamond -r 0.25
```


---

### reduce

Intelligently reduce a FASTA file for optimal aligner indexing by selecting representative sequences and encoding similar sequences as deltas.

#### Usage
```bash
# Database approach (NEW - automatically stores result)
talaria reduce [OPTIONS] [DATABASE]

# File approach (traditional)
talaria reduce [OPTIONS] -i <INPUT> -o <OUTPUT>
```

#### Positional Arguments

**`[DATABASE]`** (Optional)
Database to reduce (e.g., "uniprot/swissprot", "ncbi/nr@2024-01-01")
When specified, automatically stores result in database structure and `-i`/`-o` are not needed.

#### Required Arguments (File-based)

**`-i, --input <FILE>`**
Path to input FASTA file (required if DATABASE not specified).

**`-o, --output <FILE>`**
Path for output reduced FASTA file (required if DATABASE not specified and `--store` not used).

#### Optional Arguments

##### Core Options

**`-a, --target-aligner <ALIGNER>`**
**Default:** `generic`
**Values:** `lambda`, `blast`, `kraken`, `diamond`, `mmseqs2`, `generic`
Target aligner for optimization.

**`-r, --reduction-ratio <RATIO>`**
**Type:** Float (0.0-1.0)
**Default:** `0.3`
Target reduction ratio where 0.3 means 30% of original size.

**`--min-length <LENGTH>`**
**Type:** Integer
**Default:** `50`
Minimum sequence length to consider for reduction.

**`-m, --metadata <FILE>`**
**Type:** Path
**Default:** Auto-generated
Output path for delta metadata file. Required for reconstruction.

**`--profile <NAME>`**
**Type:** String
**Default:** Auto-generated from ratio (e.g., "30-percent")
Profile name for stored reduction (e.g., "blast-optimized"). Only used with database references or `--store`.

**`--store`**
**Type:** Flag
Store reduced version in database structure (only needed when using `-i` with external files).

**`-c, --config <FILE>`**
**Type:** Path
Path to TOML configuration file for advanced settings.

**`--protein`**
**Type:** Flag
Force protein sequence scoring (auto-detected by default).

**`--nucleotide`**
**Type:** Flag
Force nucleotide sequence scoring (auto-detected by default).

**`--skip-validation`**
**Type:** Flag
Skip the validation step after reduction for faster processing.

##### Advanced Features

**`--similarity-threshold <THRESHOLD>`**
**Type:** Float (0.0-1.0)
**Default:** `0.0` (disabled)
Enable similarity-based clustering with specified threshold.

**`--low-complexity-filter`**
**Type:** Flag
Filter out low-complexity sequences before reduction.

**`--align-select`**
**Type:** Flag
Use full alignment-based selection instead of simple length-based selection.

**`--taxonomy-aware`**
**Type:** Flag
Consider taxonomic IDs when selecting references.

**`--no-deltas`**
**Type:** Flag
Skip delta encoding entirely (faster but no reconstruction possible).

**`--max-align-length <LENGTH>`**
**Type:** Integer
**Default:** `10000`
Maximum sequence length for alignment.

#### Examples

##### Database-based reduction (NEW)
```bash
# Reduce stored database with default settings
talaria reduce uniprot/swissprot

# Reduce with custom profile and ratio
talaria reduce uniprot/swissprot --profile blast-optimized -r 0.25

# Reduce specific version
talaria reduce uniprot/swissprot@2024-01-01 --profile old-blast -r 0.3

# Further reduce an existing reduction
talaria reduce uniprot/swissprot:blast-30 --profile ultra-fast -r 0.1
```

##### File-based reduction (traditional)
```bash
# Simple 30% reduction
talaria reduce -i database.fasta -o reduced.fasta

# BLAST optimization with metadata
talaria reduce -i nr.fasta -o nr_reduced.fasta -a blast -r 0.25 -m nr_deltas.dat

# Store external file in database structure
talaria reduce -i external.fasta -o /tmp/out.fasta --store --profile custom
```

---

### validate

Validate reduction quality against original sequences and calculate coverage metrics.

#### Usage
```bash
# Database approach (NEW - automatically finds all files)
talaria validate [OPTIONS] [DATABASE:PROFILE]

# File approach (traditional)
talaria validate [OPTIONS] -o <ORIGINAL> -r <REDUCED> -d <DELTAS>
```

#### Positional Arguments

**`[DATABASE:PROFILE]`** (Optional)
Database reduction to validate (e.g., "uniprot/swissprot:blast-30")
When specified, automatically finds original, reduced, and delta files.
**Note:** Profile is required for validation.

#### Required Arguments (File-based)

**`-o, --original <FILE>`**
Original FASTA file (required if DATABASE:PROFILE not specified).

**`-r, --reduced <FILE>`**
Reduced FASTA file (required if DATABASE:PROFILE not specified).

**`-d, --deltas <FILE>`**
Delta metadata file (required if DATABASE:PROFILE not specified).

#### Optional Arguments

**`--original-results <FILE>`**
Alignment results from original database (for comparison).

**`--reduced-results <FILE>`**
Alignment results from reduced database (for comparison).

**`--report <FILE>`**
Output detailed validation report in JSON format.

#### Examples

##### Database-based validation (NEW)
```bash
# Validate stored reduction
talaria validate uniprot/swissprot:blast-30

# Validate specific version's reduction
talaria validate uniprot/swissprot@2024-01-01:blast-30

# With alignment comparison
talaria validate uniprot/swissprot:blast-30 \
  --original-results orig.m8 \
  --reduced-results red.m8 \
  --report validation.json
```

##### File-based validation (traditional)
```bash
# Basic validation
talaria validate -o original.fasta -r reduced.fasta -d deltas.tal

# With alignment comparison
talaria validate -o orig.fasta -r red.fasta -d deltas.tal \
  --original-results orig.m8 \
  --reduced-results red.m8
```

---

### reconstruct

Reconstruct original sequences from reference sequences and delta metadata.

#### Usage
```bash
# Database approach (NEW - automatically finds files)
talaria reconstruct [OPTIONS] [DATABASE:PROFILE]

# File approach (traditional)
talaria reconstruct [OPTIONS] -r <REFERENCES> -d <DELTAS> -o <OUTPUT>
```

#### Positional Arguments

**`[DATABASE:PROFILE]`** (Optional)
Database reduction to reconstruct (e.g., "uniprot/swissprot:blast-30")
When specified, automatically finds reference and delta files.
**Note:** Profile is required for reconstruction.

#### Required Arguments (File-based)

**`-r, --references <FILE>`**
Reference FASTA file (required if DATABASE:PROFILE not specified).

**`-d, --deltas <FILE>`**
Delta metadata file (required if DATABASE:PROFILE not specified).

#### Optional Arguments

**`-o, --output <FILE>`**
Output reconstructed FASTA file.
**Default:** Auto-generated based on input
- For database: `<dataset>-<profile>-reconstructed.fasta`
- For files: `reconstructed.fasta`

**`--sequences <ID1,ID2,...>`**
Only reconstruct specific sequences by ID.

#### Examples

##### Database-based reconstruction (NEW)
```bash
# Reconstruct all sequences (auto-generates output name)
talaria reconstruct uniprot/swissprot:blast-30

# Specify custom output
talaria reconstruct uniprot/swissprot:blast-30 -o my-output.fasta

# Reconstruct specific sequences only
talaria reconstruct uniprot/swissprot:blast-30 --sequences P12345,Q67890

# Reconstruct from specific version
talaria reconstruct uniprot/swissprot@2024-01-01:blast-30
```

##### File-based reconstruction (traditional)
```bash
# Basic reconstruction
talaria reconstruct -r refs.fasta -d deltas.tal -o output.fasta

# Auto-generate output name
talaria reconstruct -r refs.fasta -d deltas.tal

# Reconstruct specific sequences
talaria reconstruct -r refs.fasta -d deltas.tal --sequences P12345
```

---


##### database list

List stored databases with versions and reductions.

```bash
talaria database list [OPTIONS]
```

**Options:**
- `--database <NAME>`: Show specific database only
- `--detailed`: Show detailed information
- `--all-versions`: Show all versions (not just current)
- `--show-reduced`: Show reduced versions
- `--sort <FIELD>`: Sort by field (`name`, `size`, `date`)
- `--show-hashes`: Show Merkle roots for verification

**Examples:**
```bash
# List all databases
talaria database list

# Show with reductions
talaria database list --show-reduced

# Detailed view with all versions
talaria database list --detailed --all-versions --show-reduced

# Specific database
talaria database list --database uniprot/swissprot
```

##### database diff

Compare database versions or reductions.

```bash
talaria database diff <OLD> [NEW] [OPTIONS]
```

**Arguments:**
- `<OLD>`: First database reference (older version)
- `[NEW]`: Second database reference (defaults to previous version)

**Reference Format:**
```
source/dataset[@version][:reduction]
```

**Options:**
- `-o, --output <FILE>`: Output report file
- `-f, --format <FORMAT>`: Report format (`text`, `html`, `json`, `csv`)
- `--taxonomy`: Include taxonomic analysis
- `--detailed`: Show sequence-level changes
- `--similarity-threshold <VALUE>`: Threshold for modified sequences (0.0-1.0)
- `--headers-only`: Compare only headers (fast mode)
- `--visual`: Generate visual charts (HTML format only)

**Examples:**
```bash
# Compare with previous version
talaria database diff uniprot/swissprot

# Compare specific versions
talaria database diff uniprot/swissprot@2024-01-01 uniprot/swissprot@2024-09-01

# Compare reduced with original
talaria database diff uniprot/swissprot uniprot/swissprot:blast-30

# Generate HTML report
talaria database diff uniprot/swissprot --format html --visual -o report.html
```

##### database clean

Clean old database versions.

```bash
talaria database clean [DATABASE] [OPTIONS]
```

**Options:**
- `--keep <N>`: Number of versions to keep (default: 3)
- `--dry-run`: Show what would be deleted without deleting
- `--all`: Clean all databases

**Examples:**
```bash
# Clean old versions of specific database
talaria database clean uniprot/swissprot

# Keep 5 versions
talaria database clean uniprot/swissprot --keep 5

# Dry run
talaria database clean --all --dry-run
```

---

### search

Search for similar sequences using various aligners.

#### Usage
```bash
talaria search [OPTIONS] -d <DATABASE> -q <QUERY>
```

#### Required Arguments

**`-d, --database <FILE>`**
Database file or reference to search against.

**`-q, --query <FILE>`**
Query sequences in FASTA format.

#### Optional Arguments

**`-a, --aligner <ALIGNER>`**
**Default:** `auto`
**Values:** `blast`, `lambda`, `diamond`, `mmseqs2`, `auto`
Aligner to use for searching.

**`-o, --output <FILE>`**
Output file for results (default: stdout).

**`-e, --evalue <VALUE>`**
**Default:** `0.001`
E-value threshold for reporting matches.

**`--threads <N>`**
Number of threads to use.

#### Examples
```bash
# Search with auto-detected aligner
talaria search -d database.fasta -q queries.fasta

# Use specific aligner
talaria search -d nr.fasta -q proteins.fasta -a blast -e 1e-5

# Search against stored database
talaria search -d uniprot/swissprot:blast-30 -q queries.fasta
```

---

### stats

Display statistics about FASTA files or databases.

#### Usage
```bash
talaria stats [OPTIONS] <INPUT>
```

#### Arguments

**`<INPUT>`**
FASTA file or database reference to analyze.

#### Optional Arguments

**`--detailed`**
Show detailed per-sequence statistics.

**`--taxonomy`**
Include taxonomic distribution analysis.

**`--format <FORMAT>`**
**Values:** `text`, `json`, `csv`
Output format for statistics.

#### Examples
```bash
# Basic statistics
talaria stats database.fasta

# Detailed analysis
talaria stats database.fasta --detailed --taxonomy

# Stored database statistics
talaria stats uniprot/swissprot

# JSON output
talaria stats database.fasta --format json
```

---

### interactive

Launch interactive mode for guided operations.

#### Usage
```bash
talaria interactive
```

#### Features
- Guided database downloads
- Step-by-step reduction configuration
- Visual progress tracking
- Command history
- Tab completion

---

## Configuration

Talaria can be configured using a TOML configuration file specified with `-c` or through environment variables.

### Configuration File

Default location: `talaria.toml` in current directory or `${TALARIA_HOME}/config.toml`

```toml
[database]
database_dir = "/shared/talaria/databases/data"
retention_count = 3
auto_clean = true

[reduction]
min_sequence_length = 50
similarity_threshold = 0.95
taxonomy_aware = false
max_align_length = 10000

[search]
default_aligner = "auto"
default_evalue = 0.001

[performance]
threads = 0  # 0 = auto-detect
memory_limit = "8G"
```

### Environment Variables

```bash
# Database settings
export TALARIA_DATABASE_DIR="/shared/databases"
export TALARIA_RETENTION_COUNT=5

# Performance settings
export TALARIA_THREADS=8
export TALARIA_MEMORY_LIMIT="16G"

# Default config file
export TALARIA_CONFIG="/etc/talaria/config.toml"
```

---

## Output Formats

### Reduction Statistics
```
╔════════════════════════════════════════════════════╗
║             Reduction Statistics                    ║
╠════════════════════════════════════════════════════╣
║ Original sequences:                         565,928 ║
║ Reference sequences:                        169,778 ║
║ Child sequences:                            396,150 ║
║ Coverage:                                     70.0% ║
║ Reduction ratio:                             30.0% ║
║ File size reduction:                         68.5% ║
╚════════════════════════════════════════════════════╝
```

### Validation Metrics
```
╔════════════════════════════════════════════════════╗
║            Validation Results                       ║
╠════════════════════════════════════════════════════╣
║ Total sequences:                            565,928 ║
║ Reference sequences:                        169,778 ║
║ Child sequences:                            396,150 ║
║ Covered sequences:                          565,928 ║
║ Sequence coverage:                          100.00% ║
║ Taxonomic coverage:                          98.50% ║
║ Average delta size:                         245.3 B ║
╚════════════════════════════════════════════════════╝
```

---

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: Invalid arguments
- `3`: File not found
- `4`: Permission denied
- `5`: Network error (downloads)
- `6`: Validation failure
- `7`: Reconstruction error

---

## Performance Tips

1. **Use appropriate thread counts**: Default (0) auto-detects, but you may want to leave some cores free
2. **Enable `--skip-validation`** for large databases when you're confident in the reduction
3. **Use `--no-deltas`** if you don't need reconstruction capability
4. **Set `--max-align-length`** lower for databases with very long sequences
5. **Use stored databases** to avoid repeated file path typing and benefit from versioning
6. **Enable `--low-complexity-filter`** for databases with many simple/repetitive sequences

---

## See Also

- [Configuration Guide](../user-guide/configuration.md)
- [Basic Usage](../user-guide/basic-usage.md)
- [Workflow Examples](../workflows/README.md)
- [Team Collaboration](../../team-collaboration.md)