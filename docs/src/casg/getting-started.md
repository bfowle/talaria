# Getting Started with CASG

This tutorial will walk you through your first CASG operations. In 10 minutes, you'll understand how to use CASG for efficient database management.

## Prerequisites

Before starting, ensure you have:
- Talaria installed (`cargo install talaria` or download from releases)
- At least 10 GB free disk space for examples
- Internet connection for downloading databases

## Your First CASG Download

Let's start by downloading a small database to see CASG in action:

### Step 1: Download UniProt SwissProt

```bash
talaria database download uniprot/swissprot
```

**What You'll See:**
```
[INFO] Fetching manifest for uniprot/swissprot...
[INFO] Manifest retrieved: 571,282 sequences in 127 chunks
[INFO] Checking local CASG storage...
[INFO] Need to download 127 chunks (204 MB total)
[INFO] Downloading chunks... [████████████████] 127/127
[INFO] Verifying chunk integrity...
[SUCCESS] Database downloaded and verified!

Statistics:
  • Sequences: 571,282
  • Chunks: 127
  • Total size: 204 MB
  • Download time: 4m 32s
  • Storage saved: 67% (vs uncompressed)
```

### Step 2: Understand What Just Happened

CASG created this structure on your system:

```
~/.talaria/
├── databases/
│   ├── manifests/
│   │   └── uniprot_swissprot_2024-03-15.manifest
│   └── chunks/
│       ├── [127 chunk files organized by hash]
```

**Key Points:**
- The manifest (1 KB) describes the entire database
- Each chunk contains related sequences (taxonomically grouped)
- Chunks are named by their content hash (ensures integrity)

## Checking for Updates

Now let's see CASG's update efficiency:

### Step 3: Check for Updates (Dry Run)

```bash
talaria database update uniprot/swissprot --dry-run
```

**What You'll See:**
```
[INFO] Checking for updates to uniprot/swissprot...
[INFO] Current version: 2024-03-15 (571,282 sequences)
[INFO] Latest version: 2024-03-15 (571,282 sequences)
[SUCCESS] Database is up to date!
```

If updates were available, you'd see:
```
[INFO] Updates available:
  • New sequences: 137
  • Modified sequences: 12
  • Affected chunks: 3 of 127
  • Download size: 2.4 MB (vs 204 MB full)
  • Savings: 98.8%

Run without --dry-run to apply updates
```

### Step 4: Perform an Actual Update

When updates are available:

```bash
talaria database update uniprot/swissprot
```

CASG will:
1. Download only the changed chunks
2. Verify their integrity
3. Update the local manifest
4. Keep the old version accessible

## Using CASG with Reduction

CASG integrates seamlessly with Talaria's reduction capabilities:

### Step 5: Reduce a CASG Database

```bash
talaria reduce \
    --database uniprot/swissprot \
    --output reduced_swissprot.fasta \
    --reduction-level 0.7
```

**What Happens:**
```
[INFO] Loading database from CASG storage...
[INFO] Loaded 571,282 sequences from 127 chunks
[INFO] Selecting reference sequences...
[INFO] Computing delta encodings...
[SUCCESS] Reduction complete!

Results:
  • Input sequences: 571,282
  • Reference sequences: 171,385 (30%)
  • Delta sequences: 399,897 (70%)
  • Size reduction: 68%
  • Coverage maintained: 99.8%
```

## Exploring CASG Commands

### List Available Databases

```bash
talaria database list
```

Output:
```
Available CASG Databases:
  • uniprot/swissprot   [Local: v2024-03-15] [Remote: v2024-03-15] ✓
  • uniprot/trembl      [Not downloaded] [Remote: v2024-03-14]
  • ncbi/nr             [Not downloaded] [Remote: v2024-03-13]
  • ncbi/nt             [Not downloaded] [Remote: v2024-03-13]
```

### Check Database Status

```bash
talaria database status uniprot/swissprot
```

Output:
```
Database: uniprot/swissprot
Status: Downloaded and up-to-date

Local Version:
  • Date: 2024-03-15
  • Sequences: 571,282
  • Chunks: 127
  • Size: 204 MB
  • Hash: 5a9b3c8f2d1a...

Remote Version:
  • Date: 2024-03-15
  • Status: Same as local ✓
```

### Verify Database Integrity

```bash
talaria database verify uniprot/swissprot
```

Output:
```
[INFO] Verifying uniprot/swissprot integrity...
[INFO] Checking manifest...
[INFO] Verifying 127 chunks...
[████████████████] 127/127
[SUCCESS] All chunks verified successfully!
```

## Understanding CASG Benefits

Let's compare traditional vs CASG approaches:

### Traditional Database Management

```bash
# Traditional: Full download every time
wget ftp://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz
# Downloads: 85 MB compressed, 260 MB uncompressed
# Every update: Another 85 MB download
```

### CASG Database Management

```bash
# CASG: Smart incremental updates
talaria database download uniprot/swissprot  # Initial: 204 MB
talaria database update uniprot/swissprot    # Updates: ~2 MB typically
```

**Monthly Bandwidth Comparison** (daily updates):
- Traditional: 30 × 85 MB = 2,550 MB
- CASG: 204 MB + (29 × 2 MB) = 262 MB
- **Savings: 90%**

## Common Workflows

### Workflow 1: Daily Update Check

Create a simple script for daily updates:

```bash
#!/bin/bash
# daily_update.sh

echo "Checking for database updates..."

for db in uniprot/swissprot ncbi/nr; do
    echo "Updating $db..."
    talaria database update $db
done

echo "All databases updated!"
```

### Workflow 2: Space-Efficient Multi-Version Storage

Keep multiple versions without duplicating data:

```bash
# Download specific version
talaria database download uniprot/swissprot --version 2024-03-01

# Download latest
talaria database download uniprot/swissprot

# Both versions share common chunks!
# Only differences are stored separately
```

### Workflow 3: Team Synchronization

Share CASG storage across team:

```bash
# On server: Set up shared CASG repository
export TALARIA_HOME=/shared/talaria
talaria database download uniprot/swissprot

# On team machines: Point to shared storage
export TALARIA_HOME=/mnt/shared/talaria
talaria reduce --database uniprot/swissprot ...
```

## Troubleshooting Common Issues

### Issue: "No CASG data found"

```bash
# Initialize CASG if needed
talaria casg init

# Verify TALARIA_HOME is set correctly
echo $TALARIA_HOME
```

### Issue: "Chunk verification failed"

```bash
# Remove corrupted chunk and re-download
talaria database repair uniprot/swissprot
```

### Issue: "Out of disk space during download"

```bash
# Check available space
df -h ~/.talaria

# Clean old versions
talaria database clean --keep-latest 2
```

## Next Steps

Now that you understand CASG basics:

1. **Learn Best Practices**: Read [CASG Best Practices](./best-practices.md)
2. **Explore Advanced Features**: See [Common Workflows](./workflows.md)
3. **Understand Performance**: Check [Performance Metrics](./performance.md)
4. **Deep Dive**: Read the [Technical Architecture](../whitepapers/casg-architecture.md)

## Quick Reference Card

| Command | Purpose |
|---------|---------|
| `talaria database download <db>` | Download database |
| `talaria database update <db>` | Update to latest version |
| `talaria database list` | List available databases |
| `talaria database status <db>` | Check database status |
| `talaria database verify <db>` | Verify integrity |
| `talaria database clean` | Remove old versions |
| `talaria casg init` | Initialize CASG |

Remember: CASG makes database management efficient, verifiable, and reproducible!