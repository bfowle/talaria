# SEQUOIA Troubleshooting Guide

Common issues and solutions when working with the Sequence Query Optimization with Indexed Architecture system.

## Common Issues

### 1. Database Re-downloading on Every Run

**Symptoms:**
```
No local SEQUOIA data found
```

**Causes:**
- SEQUOIA not initialized
- Manifest saved to wrong location
- Corrupt manifest file

**Solutions:**

```bash
# Initialize database repository if needed
talaria database init

# Check manifest exists
ls ${TALARIA_HOME}/databases/manifests/

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
df -h ${TALARIA_HOME}/databases

# Clean up old chunks manually
find ${TALARIA_HOME}/databases/chunks -type f -mtime +30 -delete

# Check permissions
ls -la ${TALARIA_HOME}/databases/chunks/
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
ls ${TALARIA_HOME}/databases/manifests/
```

### 4. Custom Database Issues

**Symptoms:**
```
Error: Failed to add custom database
```

**Causes:**
- Invalid FASTA format
- Database already exists
- SEQUOIA not initialized

**Solutions:**

```bash
# Initialize SEQUOIA first
talaria database init

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
talaria database stats
du -sh ${TALARIA_HOME}/databases/

# Remove unused databases manually
rm -rf ${TALARIA_HOME}/databases/chunks/[hash_prefix]/

# Move storage to larger disk using symlink
mv ${TALARIA_HOME}/databases /data/sequoia
ln -s /data/sequoia ${TALARIA_HOME}/databases

# Future: Garbage collection will be added
# Database cleanup functionality not yet implemented
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

# Use file-based reduction instead of SEQUOIA
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
talaria database update ncbi/nr --use-sequoia --download

# Use faster compression
# Compression configuration not yet available via CLI

# Skip verification during assembly (faster but less safe)
talaria database export uniprot/swissprot -o output.fasta

# Enable chunk caching
export TALARIA_CACHE_SIZE=4G
talaria database export ncbi/nr -o nr.fasta

# Use SSD for chunk storage
ln -s /ssd/sequoia ${TALARIA_HOME}/databases
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
ls ${TALARIA_HOME}/databases/manifests/
# Should see: uniprot-swissprot.tal, ncbi-nr.tal, etc.

# Re-download database to fix manifest
talaria database download uniprot -d swissprot

# Check manifest exists and is valid
ls -la ${TALARIA_HOME}/databases/manifests/uniprot-swissprot.tal
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
Error: Lock file exists: ${TALARIA_HOME}/databases/.lock
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
rm ${TALARIA_HOME}/databases/.lock

# Use read-only mode
talaria database export uniprot/swissprot -o output.fasta

# Wait for lock
talaria database export uniprot/swissprot -o output.fasta
```

### 11. Future: Cloud Storage Support

**Note**: Cloud storage backends are planned for future releases. Currently, SEQUOIA operates with local storage only.

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
export RUST_LOG=talaria::sequoia=debug
talaria database download uniprot -d swissprot

# Trace-level logging
export RUST_LOG=talaria::sequoia=trace

# Log to file
export RUST_LOG=talaria::sequoia=debug
talaria database download uniprot -d swissprot 2> sequoia_debug.log
```

### Check System Status

```bash
# Check SEQUOIA repository statistics
talaria database stats

# List databases
talaria database list

# Check specific database
talaria database info uniprot/swissprot
```

### Manual Recovery

```bash
# Backup current state
tar -czf sequoia_backup.tar.gz ${TALARIA_HOME}/databases/

# Manual reset (remove and reinitialize)
rm -rf ${TALARIA_HOME}/databases
talaria database init

# Restore from backup
tar -xzf sequoia_backup.tar.gz -C ~/
```

## Performance Tuning

### Current Configuration Options

```bash
# Use more threads for parallel processing
talaria database download uniprot -d swissprot -j 16

# Move SEQUOIA storage to faster disk
mv ${TALARIA_HOME}/databases /fast/ssd/sequoia
ln -s /fast/ssd/sequoia ${TALARIA_HOME}/databases
```

### Future Configuration Support

Configuration file support is planned for future releases:

```toml
# Future: ${TALARIA_HOME}/config.toml
[sequoia]
compression = "zstd"
compression_level = 3
parallel_downloads = 8
```

## Common Error Messages

| Error | Meaning | Solution |
|-------|---------|----------|
| `No local SEQUOIA data found` | Manifest not found | Re-download database |
| `ChunkNotFound` | Missing chunk in storage | Re-download database |
| `VerificationFailed` | Hash mismatch | Re-download database |
| `NetworkTimeout` | Download timeout | Retry with `--resume` |
| `StorageFull` | Disk space exhausted | Free space or move storage |
| `PermissionDenied` | File permissions issue | Check `${TALARIA_HOME}/databases/` permissions |
| `Database already exists` | Custom database exists | Use `--replace` flag |

## Getting Help

```bash
# Built-in help
talaria --help
talaria database --help
talaria database --help

# Check version
talaria --version

# View statistics
talaria database stats

# List available databases
talaria database list
```

## See Also

- [SEQUOIA Overview](overview.md) - Understanding SEQUOIA concepts
- [Architecture](architecture.md) - System design details
- [API Reference](api-reference.md) - Programming interface
- [CLI Reference](../api/cli-reference.md#sequoia) - Command-line tools