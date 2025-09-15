# CASG Troubleshooting Guide

Common issues and solutions when working with the Content-Addressed Sequence Graph system.

## Common Issues

### 1. Database Re-downloading on Every Run

**Symptoms:**
```
No local CASG data found
```

**Causes:**
- CASG not initialized
- Manifest saved to wrong location
- Corrupt manifest file

**Solutions:**

```bash
# Initialize CASG if needed
talaria casg init

# Check manifest exists
ls ~/.talaria/databases/manifests/

# Re-download to fix manifest
talaria database download uniprot/swissprot
```

### 2. Database Download Failures

**Symptoms:**
```
Error: Failed to download database: Network error
```

**Causes:**
- Network connectivity issues
- Firewall blocking connections
- Source server is down

**Solutions:**

```bash
# Retry download
talaria database download uniprot -d swissprot

# Use proxy if needed
export HTTP_PROXY=http://proxy.example.com:8080
talaria database download uniprot -d swissprot

# Resume incomplete download
talaria database download uniprot -d swissprot --resume
```

### 2. Chunk Storage Issues

**Symptoms:**
```
Error: Failed to store chunk: Disk full
```

**Causes:**
- Insufficient disk space
- Permission issues
- Corrupted chunk file

**Solutions:**

```bash
# Check disk space
df -h ~/.talaria/databases

# Clean up old chunks manually
find ~/.talaria/databases/chunks -type f -mtime +30 -delete

# Check permissions
ls -la ~/.talaria/databases/chunks/
```

### 3. Database Not Found

**Symptoms:**
```
Error: Database not found: uniprot/swissprot
```

**Causes:**
- Database not downloaded
- Incorrect path
- Wrong database name

**Solutions:**

```bash
# List available databases
talaria database list

# Download the database
talaria database download uniprot -d swissprot

# Check database path
ls ~/.talaria/databases/manifests/
```

### 4. Custom Database Issues

**Symptoms:**
```
Error: Failed to add custom database
```

**Causes:**
- Invalid FASTA format
- Database already exists
- CASG not initialized

**Solutions:**

```bash
# Initialize CASG first
talaria casg init

# Replace existing database
talaria database add -i sequences.fasta --source mylab --dataset proteins --replace

# Validate FASTA format
grep -c '^>' sequences.fasta  # Count sequences

# Check if database exists
talaria database list
```

### 5. Storage Space Issues

**Symptoms:**
```
Error: No space left on device
Warning: Storage usage at 95%
```

**Causes:**
- Large databases downloaded
- Multiple database versions
- Insufficient disk space

**Solutions:**

```bash
# Check storage usage
talaria casg stats
du -sh ~/.talaria/databases/

# Remove unused databases manually
rm -rf ~/.talaria/databases/chunks/[hash_prefix]/

# Move storage to larger disk using symlink
mv ~/.talaria/databases /data/casg
ln -s /data/casg ~/.talaria/databases

# Future: Garbage collection will be added
# talaria casg gc  # Not yet implemented
```

### 6. Memory Issues During Reduction

**Symptoms:**
```
Error: Out of memory during reduction
Killed (OOM)
```

**Causes:**
- Large database
- Loading entire database in memory
- Insufficient RAM

**Solutions:**

```bash
# Reduce memory usage
export TALARIA_MAX_MEMORY=8G
talaria reduce -d ncbi/nr -o reduced.fasta -r 0.3

# Process smaller database
talaria reduce -d uniprot/swissprot -o reduced.fasta -r 0.3

# Use file-based reduction instead of CASG
talaria reduce -i sequences.fasta -o reduced.fasta -r 0.3
```

### 7. Slow Performance

**Symptoms:**
- Downloads taking too long
- Assembly is slow
- Verification takes hours

**Solutions:**

```bash
# Increase parallel downloads
export TALARIA_PARALLEL_DOWNLOADS=10
talaria database update ncbi/nr --use-casg --download

# Use faster compression
talaria casg config --compression lz4

# Skip verification during assembly (faster but less safe)
talaria casg assemble uniprot/swissprot --no-verify -o output.fasta

# Enable chunk caching
export TALARIA_CACHE_SIZE=4G
talaria casg assemble ncbi/nr -o nr.fasta

# Use SSD for chunk storage
ln -s /ssd/casg ~/.talaria/databases
```

### 8. Manifest Issues

**Symptoms:**
```
Error: Invalid manifest format
Warning: Manifest not found
```

**Causes:**
- Corrupted manifest file
- Missing manifest
- Wrong manifest location

**Solutions:**

```bash
# Check manifest location (should be database-specific)
ls ~/.talaria/databases/manifests/
# Should see: uniprot-swissprot.json, ncbi-nr.json, etc.

# Re-download database to fix manifest
talaria database download uniprot -d swissprot

# Check manifest format
cat ~/.talaria/databases/manifests/uniprot-swissprot.json | python -m json.tool | head -20
```

### 9. List Sequences Issues

**Symptoms:**
```
Error: Failed to list sequences
```

**Causes:**
- Database not downloaded
- Corrupted chunks
- Invalid format specified

**Solutions:**

```bash
# Verify database exists
talaria database list

# Try different output format
talaria database list-sequences uniprot/swissprot --format text

# Limit output
talaria database list-sequences uniprot/swissprot --limit 10

# Output only IDs
talaria database list-sequences uniprot/swissprot --ids-only
```

### 10. Concurrent Access Issues

**Symptoms:**
```
Error: Lock file exists: ~/.talaria/databases/.lock
Warning: Another process is accessing the repository
```

**Causes:**
- Multiple processes
- Stale lock file
- Crashed process

**Solutions:**

```bash
# Check for running processes
ps aux | grep talaria

# Remove stale lock (if no other processes)
rm ~/.talaria/databases/.lock

# Use read-only mode
talaria casg assemble uniprot/swissprot --read-only -o output.fasta

# Wait for lock
talaria casg assemble uniprot/swissprot --wait-lock -o output.fasta
```

### 11. Future: Cloud Storage Support

**Note**: Cloud storage backends are planned for future releases. Currently, CASG operates with local storage only.

When implemented, cloud storage will support:
- AWS S3
- Google Cloud Storage
- Azure Blob Storage
- S3-compatible storage (MinIO, Ceph)

Planned environment variables:
```bash
# Future: Cloud manifest server
export TALARIA_MANIFEST_SERVER=s3://bucket/manifests
export TALARIA_CHUNK_SERVER=https://cdn.example.com/chunks
```

## Debugging Commands

### Enable Debug Logging

```bash
# Verbose output
export RUST_LOG=talaria::casg=debug
talaria database download uniprot -d swissprot

# Trace-level logging
export RUST_LOG=talaria::casg=trace

# Log to file
export RUST_LOG=talaria::casg=debug
talaria database download uniprot -d swissprot 2> casg_debug.log
```

### Check System Status

```bash
# Check CASG repository statistics
talaria casg stats

# List databases
talaria database list

# Check specific database
talaria database info uniprot/swissprot
```

### Manual Recovery

```bash
# Backup current state
tar -czf casg_backup.tar.gz ~/.talaria/databases/

# Manual reset (remove and reinitialize)
rm -rf ~/.talaria/databases
talaria casg init

# Restore from backup
tar -xzf casg_backup.tar.gz -C ~/
```

## Performance Tuning

### Current Configuration Options

```bash
# Use more threads for parallel processing
talaria database download uniprot -d swissprot -j 16

# Move CASG storage to faster disk
mv ~/.talaria/databases /fast/ssd/casg
ln -s /fast/ssd/casg ~/.talaria/databases
```

### Future Configuration Support

Configuration file support is planned for future releases:

```toml
# Future: ~/.talaria/config.toml
[casg]
compression = "zstd"
compression_level = 3
parallel_downloads = 8
```

## Common Error Messages

| Error | Meaning | Solution |
|-------|---------|----------|
| `No local CASG data found` | Manifest not found | Re-download database |
| `ChunkNotFound` | Missing chunk in storage | Re-download database |
| `VerificationFailed` | Hash mismatch | Re-download database |
| `NetworkTimeout` | Download timeout | Retry with `--resume` |
| `StorageFull` | Disk space exhausted | Free space or move storage |
| `PermissionDenied` | File permissions issue | Check `~/.talaria/databases/` permissions |
| `Database already exists` | Custom database exists | Use `--replace` flag |

## Getting Help

```bash
# Built-in help
talaria --help
talaria casg --help
talaria database --help

# Check version
talaria --version

# View statistics
talaria casg stats

# List available databases
talaria database list
```

## See Also

- [CASG Overview](overview.md) - Understanding CASG concepts
- [Architecture](architecture.md) - System design details
- [API Reference](api-reference.md) - Programming interface
- [CLI Reference](../api/cli-reference.md#casg) - Command-line tools