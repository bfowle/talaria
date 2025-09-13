# Team Collaboration with Talaria Database Management

Talaria's database management system is designed with team collaboration in mind. By centralizing database storage, versioning, and reduced versions in a structured directory, teams can share a single source of truth for their bioinformatics data.

## Overview

The database management system provides:
- **Centralized Storage**: All databases stored in a consistent directory structure
- **Version Control**: Date-based versioning with symlinks to current versions
- **Reduction Profiles**: Store multiple reduced versions alongside originals
- **Metadata Tracking**: Complete traceability from reduced → original → source
- **Shared Access**: Teams can mount the database directory on shared storage

## Directory Structure

```
~/.talaria/databases/
├── data/                    # All database files
│   └── uniprot/
│       └── swissprot/
│           ├── current -> 2025-09-12  # Symlink to current version
│           └── 2025-09-12/
│               ├── swissprot.fasta
│               ├── metadata.json
│               └── reduced/         # Reduced versions
│                   ├── 30-percent/
│                   │   ├── swissprot.fasta
│                   │   ├── swissprot.deltas.tal
│                   │   └── metadata.json
│                   └── blast-optimized/
│                       ├── swissprot.fasta
│                       ├── swissprot.deltas.tal
│                       └── metadata.json
└── metadata/                # Additional metadata (future use)
```

## Setting Up Team Collaboration

### 1. Configure Shared Database Directory

Set up a shared network location accessible by all team members:

```bash
# In talaria.toml configuration
[database]
database_dir = "/shared/team/talaria/databases/data"
retention_count = 3  # Keep 3 old versions
```

Or use environment variable:
```bash
export TALARIA_DATABASE_DIR="/shared/team/talaria/databases/data"
```

### 2. Initial Database Setup

One team member downloads the initial databases:

```bash
# Download UniProt SwissProt
talaria database download uniprot -d swissprot

# Download NCBI NR
talaria database download ncbi -d nr

# List available databases
talaria database list
```

### 3. Creating Reduced Versions

Team members can create and store reduced versions for specific use cases:

```bash
# Create a 30% reduced version for BLAST searches
talaria reduce \
  -i /shared/team/talaria/databases/data/uniprot/swissprot/current/swissprot.fasta \
  -o /tmp/reduced.fasta \
  --store \
  --database uniprot/swissprot \
  --profile blast-30 \
  --reduction-ratio 0.3 \
  --target-aligner blast

# Create a taxonomy-aware reduction
talaria reduce \
  -i /shared/team/talaria/databases/data/uniprot/swissprot/current/swissprot.fasta \
  -o /tmp/reduced.fasta \
  --store \
  --database uniprot/swissprot \
  --profile taxonomy-balanced \
  --reduction-ratio 0.5 \
  --taxonomy-aware
```

### 4. Using Reduced Versions

Team members can reference reduced versions directly:

```bash
# Use in searches
talaria search \
  -d /shared/team/talaria/databases/data/uniprot/swissprot/current/reduced/blast-30/swissprot.fasta \
  -q query.fasta

# Compare versions
talaria database diff \
  uniprot/swissprot \
  uniprot/swissprot:blast-30

# Compare different reduction profiles
talaria database diff \
  uniprot/swissprot:blast-30 \
  uniprot/swissprot:taxonomy-balanced
```

## Listing and Managing Databases

### View All Databases
```bash
# Basic listing
talaria database list

# Show with reduced versions
talaria database list --show-reduced

# Detailed view with all versions
talaria database list --detailed --all-versions --show-reduced
```

### Example Output with Reductions
```
Found 2 database(s):

┌────────────────────┬──────────┬──────────┬──────────────────────┬──────────┐
│ Database           │ Version  │ Size     │ Modified             │ Versions │
├────────────────────┼──────────┼──────────┼──────────────────────┼──────────┤
│ uniprot/swissprot  │ 2025-09-12 │ 268 MiB  │ 2025-09-12 14:30:00 │ 3        │
│   └─ blast-30      │ 30%      │ 80 MiB   │                      │ 45K seqs │
│   └─ taxonomy-bal  │ 50%      │ 134 MiB  │                      │ 76K seqs │
│ ncbi/nr            │ 2025-09-11 │ 180 GiB  │ 2025-09-11 08:00:00 │ 2        │
│   └─ fast-search   │ 25%      │ 45 GiB   │                      │ 12M seqs │
└────────────────────┴──────────┴──────────┴──────────────────────┴──────────┘
```

## Version Management

### Comparing Versions
```bash
# Compare current with previous version
talaria database diff uniprot/swissprot

# Compare specific versions
talaria database diff \
  uniprot/swissprot@2025-09-10 \
  uniprot/swissprot@2025-09-12

# Generate detailed HTML report
talaria database diff \
  uniprot/swissprot@2025-09-10 \
  uniprot/swissprot@2025-09-12 \
  --format html \
  --output comparison-report.html \
  --visual
```

### Cleaning Old Versions
```bash
# Remove old versions (keeps 3 by default)
talaria database clean uniprot/swissprot

# Keep more versions
talaria database clean uniprot/swissprot --keep 5
```

## Reduction Profiles

### Standard Profiles

Teams can establish standard reduction profiles for common use cases:

- **blast-optimized**: 30% reduction optimized for BLAST searches
- **diamond-optimized**: 40% reduction optimized for DIAMOND
- **taxonomy-balanced**: 50% reduction maintaining taxonomic diversity
- **ultra-fast**: 10% reduction for very quick preliminary searches
- **high-sensitivity**: 70% reduction maintaining high sensitivity

### Profile Metadata

Each reduction profile stores comprehensive metadata:

```json
{
  "source_database": "uniprot/swissprot",
  "reduction_ratio": 0.3,
  "target_aligner": "blast",
  "original_sequences": 150000,
  "reference_sequences": 45000,
  "child_sequences": 105000,
  "input_size": 281018368,
  "output_size": 84305510,
  "reduction_date": "2025-09-12T14:35:00Z",
  "parameters": {
    "min_length": 50,
    "similarity_threshold": 0.95,
    "taxonomy_aware": false,
    "align_select": false,
    "no_deltas": false,
    "max_align_length": 10000
  }
}
```

## Best Practices

### 1. Naming Conventions

Use descriptive profile names that indicate:
- Purpose: `blast-`, `diamond-`, `taxonomy-`
- Reduction level: `-30`, `-50`, `-minimal`
- Special features: `-balanced`, `-fast`, `-sensitive`

Examples:
- `blast-30-fast`
- `diamond-50-balanced`
- `taxonomy-70-sensitive`

### 2. Documentation

Document reduction profiles in a team wiki or README:

```markdown
## Reduction Profiles

### blast-30-fast
- **Purpose**: Quick BLAST searches for initial screening
- **Reduction**: 30% of original
- **Use Case**: Preliminary homology searches
- **Created By**: Team Member A
- **Date**: 2025-09-12
```

### 3. Synchronization

For distributed teams:
- Use rsync or similar tools to sync database directories
- Set up automated nightly downloads for updated databases
- Use version control for configuration files

### 4. Access Control

Set appropriate permissions:
```bash
# Read access for all team members
chmod -R 755 /shared/team/talaria/databases

# Write access for database maintainers
chgrp -R talaria-admin /shared/team/talaria/databases
chmod -R 775 /shared/team/talaria/databases
```

## Automation

### Scheduled Updates

Create a cron job for automatic updates:

```bash
# /etc/cron.d/talaria-update
0 2 * * 0 talaria-admin /usr/local/bin/talaria database download uniprot -d swissprot
0 3 * * 0 talaria-admin /usr/local/bin/talaria database download uniprot -d trembl
0 4 * * 0 talaria-admin /usr/local/bin/talaria reduce --store --database uniprot/swissprot --profile blast-30 --reduction-ratio 0.3
```

### Update Notifications

Set up notifications for database updates:

```bash
#!/bin/bash
# notify-updates.sh

BEFORE=$(talaria database list --json)
talaria database download uniprot -d swissprot
AFTER=$(talaria database list --json)

if [ "$BEFORE" != "$AFTER" ]; then
    echo "Database updated" | mail -s "Talaria Database Update" team@example.com
fi
```

## Troubleshooting

### Common Issues

1. **Permission Denied**
   - Check directory permissions
   - Ensure shared mount is accessible
   - Verify user group membership

2. **Symlink Issues**
   - Some network filesystems don't support symlinks
   - Use direct version references instead: `uniprot/swissprot@2025-09-12`

3. **Storage Space**
   - Monitor disk usage: `du -sh /shared/team/talaria/databases`
   - Clean old versions regularly: `talaria database clean --all`
   - Consider compression for archived versions

4. **Slow Network Access**
   - Consider local caching for frequently used databases
   - Use reduced versions for routine work
   - Set up regional mirrors for distributed teams

## Advanced Features

### Custom Storage Backends

Future versions will support:
- Cloud storage (S3, GCS, Azure)
- Distributed filesystems (HDFS, Ceph)
- Database versioning with Git LFS
- Blockchain-based version verification

### API Access

Planned REST API for database management:
```bash
# Get database info
curl https://talaria-server/api/databases/uniprot/swissprot

# List reductions
curl https://talaria-server/api/databases/uniprot/swissprot/reductions

# Download specific version
curl https://talaria-server/api/databases/uniprot/swissprot/versions/2025-09-12/download
```

## Conclusion

Talaria's database management system enables teams to:
- Maintain a single source of truth for biological databases
- Share optimized reduced versions for specific workflows
- Track changes between database versions
- Ensure reproducibility across team analyses

By centralizing database management, teams can focus on science rather than data management logistics.