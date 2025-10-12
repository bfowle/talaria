# Caching and Database Discoverability

## Overview

HERALD implements intelligent metadata caching and enhanced CLI commands to provide excellent performance and user experience when working with RocksDB-based storage.

## Problem Statement

The move from filesystem-based to RocksDB-based storage introduced two challenges:

1. **Performance**: Querying RocksDB for database lists and versions required iterating through manifests, taking ~20 seconds for large repositories
2. **Discoverability**: Users couldn't easily browse the filesystem to see what databases exist, making it harder to understand repository contents

## Solution: Intelligent Caching Layer

### Architecture

The caching system consists of:

- **In-Memory Cache**: Fast HashMap-based cache with Arc<RwLock> for thread safety
- **Persistent Cache**: JSON files in `~/.talaria/databases/.cache/` that survive process restarts
- **Smart Invalidation**: Automatic cache invalidation when databases change
- **TTL-Based Expiration**: 5-minute default TTL to balance freshness and performance

### Cache Structure

```rust
pub struct MetadataCache {
    cache_dir: PathBuf,
    ttl: Duration,

    // In-memory caches
    database_list: Arc<RwLock<Option<CachedDatabaseList>>>,
    version_lists: Arc<RwLock<HashMap<String, CachedVersionList>>>,
    stats: Arc<RwLock<Option<CachedStats>>>,

    // Cache metadata
    metadata: Arc<RwLock<Option<CacheMetadata>>>,
}
```

### Cached Data Types

1. **Database List** (`database_list.json`)
   - All databases with metadata (name, version, chunks, size)
   - Updated when: databases added, deleted, or updated

2. **Version Lists** (`versions_<source>_<dataset>.json`)
   - All versions for a specific database
   - Includes aliases, timestamps, chunk counts
   - Updated when: versions added or deleted for that database

3. **Repository Stats** (`stats.json`)
   - Global statistics (total chunks, size, deduplication ratio)
   - Database summaries
   - Updated when: any database changes

### Cache Lifecycle

```
┌─────────────────┐
│  Command Run    │
└────────┬────────┘
         │
         v
┌─────────────────┐      ┌──────────────┐
│  Check Cache    │─YES──>│ Return Data  │
└────────┬────────┘      └──────────────┘
         │ NO/EXPIRED
         v
┌─────────────────┐
│  Query RocksDB  │
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Update Cache   │
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Return Data    │
└─────────────────┘
```

### Invalidation Triggers

The cache is automatically invalidated when:

- **Database Download/Update**: `save_database_manifest_internal()` invalidates affected database
- **Version Deletion**: `delete_database_version()` invalidates affected database
- **Entire Database Deletion**: `delete_entire_database()` invalidates affected database
- **Manual**: Can be cleared by deleting `~/.talaria/databases/.cache/`

### Performance Impact

**Before Caching:**
```
$ time talaria database list
# ... output ...
real    0m19.815s
user    0m4.496s
sys     0m13.036s
```

**After Caching (warm):**
```
$ time talaria database list
# ... output ...
real    0m1.386s
user    0m1.733s
sys     0m1.101s
```

**Result**: **14x performance improvement** for repeated queries!

## Enhanced CLI Commands

### database list --all-versions

Shows hierarchical view of all databases and their versions:

```
$ talaria database list --all-versions

Database Repository

● uniprot/swissprot (3 versions) └ RocksDB: manifest:uniprot:swissprot:*
  ├─ ▶ 20251001_184103 (2025-10-01 18:41) ─ 14,806 chunks (547.09 MiB)
  ├─ ▶ 20251001_172146 (2025-10-01 17:21) ─ 14,806 chunks (547.09 MiB)
  └─ ▶ 20251001_044555 (2025-10-01 04:45) ─ 14,806 chunks (547.09 MiB)

● uniprot/uniref50 (1 version) └ RocksDB: manifest:uniprot:uniref50:*
  └─ ▶ 20251001_012052 (2025-10-01 01:20) ─ 22,992,030 chunks (65.38 GiB)

● Use 'talaria database versions list <database>' for detailed version info
● Storage location: /home/brett/.talaria/databases/sequences/rocksdb
```

**Features:**
- Groups databases by name showing version counts
- Tree-style display with timestamps and chunk/size info
- Shows RocksDB manifest key pattern
- Provides storage location hints

### database info --format detailed

Shows comprehensive database information including RocksDB details:

```
$ talaria database info uniprot/swissprot --format detailed

Database Information
├─ Name: uniprot/swissprot
├─ Version: 20251001_044555
├─ Created: 2025-10-01 04:45:55
├─ Storage
│  ├─ Chunks: 14,806
│  ├─ Size: 547.09 MiB
│  ├─ Total Sequences: 573,661
│  ├─ Avg Chunk Size: 37.84 KiB
│  ├─ Min Chunk Size: 1000 B
│  └─ Max Chunk Size: 10.00 MiB
├─ RocksDB Storage
│  ├─ Manifest Key: manifest:uniprot:swissprot:20251001_044555
│  ├─ Alias Keys: alias:uniprot:swissprot:current, alias:uniprot:swissprot:latest
│  ├─ Storage Path: /home/brett/.talaria/databases/sequences/rocksdb
│  └─ Total Versions: 3
```

**Features:**
- Shows exact RocksDB keys for programmatic access
- Displays storage paths
- Provides version count context
- Maintains tree-style formatting

### database stats

Enhanced with RocksDB statistics section:

```
$ talaria database stats

════════════════════════════════════════════════════════════
               DATABASE REPOSITORY STATISTICS
════════════════════════════════════════════════════════════

Total chunks: 23,036,448
Total size: 66.98 GiB
Compressed chunks: 485,425
Deduplication ratio: 1.00x
Databases: 4

Databases:
  • uniprot/swissprot (3 versions, 14,806 chunks, 547.09 MiB)
  • uniprot/uniref50 (v20251001_012052, 22,992,030 chunks, 65.38 GiB)

════════════════════════════════════════════════════════════
RocksDB Storage
────────────────────────────────────────────────────────────
Storage path: /home/brett/.talaria/databases/sequences/rocksdb
Total versions: 4
RocksDB size: 2.92 GiB
SST files: 21
```

**Features:**
- Groups databases showing version counts
- Dedicated RocksDB statistics section
- Shows storage path and size
- Reports SST file count for performance monitoring

## Understanding RocksDB Storage

### Key Format

HERALD uses a structured key format in RocksDB:

**Manifest Keys:**
```
manifest:{source}:{dataset}:{timestamp}
```
Examples:
- `manifest:uniprot:swissprot:20251001_184103`
- `manifest:ncbi:refseq:20250915_123456`

**Alias Keys:**
```
alias:{source}:{dataset}:{alias}
alias:{source}:{dataset}:custom:{alias}
```
Examples:
- `alias:uniprot:swissprot:current` → points to latest timestamp
- `alias:uniprot:swissprot:latest` → points to newest timestamp
- `alias:uniprot:swissprot:custom:stable` → user-defined alias

### Storage Layout

```
~/.talaria/databases/
├── sequences/
│   └── rocksdb/                  # Main RocksDB database
│       ├── MANIFEST-*            # RocksDB metadata
│       ├── *.sst                 # Sorted String Table files
│       ├── *.log                 # Write-ahead log
│       └── OPTIONS-*             # Configuration
├── .cache/                       # Metadata cache (NEW!)
│   ├── database_list.json        # Cached database list
│   ├── versions_uniprot_swissprot.json
│   ├── versions_uniprot_uniref50.json
│   └── stats.json                # Cached repository stats
└── downloads/                    # Temporary download workspaces
    └── uniprot_swissprot_*       # Cleaned up after successful download
```

### Finding Your Data

**To list all databases:**
```bash
talaria database list
```

**To see all versions of a database:**
```bash
talaria database versions list uniprot/swissprot
```

**To see version hierarchy:**
```bash
talaria database list --all-versions
```

**To inspect a specific database:**
```bash
talaria database info uniprot/swissprot --format detailed
```

**To see global statistics:**
```bash
talaria database stats
```

## Cache Management

### Viewing Cache Status

Check cache directory:
```bash
ls -lh ~/.talaria/databases/.cache/
```

Check cache contents:
```bash
cat ~/.talaria/databases/.cache/database_list.json | jq
```

### Clearing Cache

If needed, cache can be cleared manually:
```bash
rm -rf ~/.talaria/databases/.cache/
```

The cache will be automatically rebuilt on the next query.

### Cache Configuration

The cache TTL can be modified in code:
```rust
let cache = MetadataCache::new(cache_dir)?
    .with_ttl(300); // 5 minutes (default)
```

## Best Practices

### For Users

1. **First Command May Be Slow**: The first time you run a query command, it needs to read RocksDB. Subsequent runs within 5 minutes will be instant.

2. **Use Specific Commands**:
   - Want quick overview? → `database list`
   - Want version hierarchy? → `database list --all-versions`
   - Want deep details? → `database info <db> --format detailed`

3. **Check Storage Regularly**: Run `database stats` to monitor repository size and health

### For Developers

1. **Cache Invalidation**: Always invalidate cache when modifying databases:
   ```rust
   if let Some(cache) = &self.cache {
       cache.invalidate_database(source, dataset);
   }
   ```

2. **Cache-First Pattern**: Always check cache before expensive RocksDB operations:
   ```rust
   if let Some(cache) = &self.cache {
       if let Some(data) = cache.get_data() {
           return Ok(data);
       }
   }
   // ... expensive query ...
   cache.set_data(result.clone());
   ```

3. **Testing**: Clear cache before integration tests to ensure fresh state

## Troubleshooting

### Slow Commands After Update

**Symptom**: Commands are slow even though cache should exist

**Solution**: Cache may be stale or invalidated. Wait for first command to complete (builds new cache).

### Outdated Information Displayed

**Symptom**: Command shows old version as current

**Solution**: Cache hasn't been invalidated. Either:
1. Wait 5 minutes for TTL expiration
2. Manually clear cache: `rm -rf ~/.talaria/databases/.cache/`

### Cache Not Working

**Symptom**: Every command is slow, no speedup

**Possible Causes**:
1. Cache directory not writable
2. Disk full
3. Cache initialization failed

**Solution**: Check logs and ensure `~/.talaria/databases/.cache/` is writable

## Future Enhancements

Potential improvements to the caching system:

1. **Configurable TTL**: Allow users to set cache expiration in config file
2. **Partial Cache Invalidation**: Only invalidate affected entries, not entire caches
3. **Cache Warming**: Pre-populate cache during downloads
4. **Cache Statistics**: Show cache hit rate and performance metrics
5. **Remote Cache**: Share cache across multiple machines for distributed setups

## Summary

The intelligent caching system provides:
- **14x performance improvement** for repeated queries
- **Automatic cache management** with no user intervention needed
- **Enhanced CLI commands** for better database discoverability
- **Clear visibility** into RocksDB storage structure
- **Excellent UX** maintaining the ease of the old filesystem approach

Users get the best of both worlds: RocksDB's performance and reliability, combined with the easy discoverability of filesystem-based storage.
