# Resumable Downloads

Talaria supports resumable downloads for all database sources, allowing you to recover from interrupted downloads without starting over.

## How It Works

When downloading large databases (like UniProt TrEMBL or NCBI NR), network interruptions or timeouts can occur. Talaria automatically saves the download progress and can resume from where it left off.

### Key Features

- **Automatic State Tracking**: Download progress is saved automatically
- **Server Validation**: Checks if the server file has changed (via ETag)
- **Integrity Verification**: Validates partial files before resuming
- **Bandwidth Efficient**: Only downloads remaining data

## Using Resume

### Basic Resume

To resume an interrupted download, simply run the same command with the `--resume` flag:

```bash
# Initial download (gets interrupted)
talaria database download uniprot/trembl

# Resume the download
talaria database download uniprot/trembl --resume
```

### Continue Flag (Alias)

The `-c` or `--continue-download` flag is an alias for `--resume`:

```bash
# Same as --resume
talaria database download ncbi/nr -c
```

## How Resume Works

### 1. State Persistence

When a download is interrupted, Talaria saves:
- Downloaded bytes count
- Temporary file location
- Server ETag (file version identifier)
- Last-modified timestamp
- Partial file hash for validation

### 2. Resume Process

When resuming, Talaria:
1. Checks if the partial file exists
2. Validates the file size matches recorded progress
3. Verifies the server file hasn't changed (ETag check)
4. Sends a byte-range request to continue from the last position
5. Appends new data to the existing partial file

### 3. Server Support Detection

Talaria automatically detects if the server supports resume:
- Checks for `Accept-Ranges: bytes` header
- Tests with a small range request if needed
- Falls back to fresh download if resume isn't supported

## Resume State Files

Resume state is stored in hidden files alongside the download:

```
~/.talaria/databases/
├── uniprot/
│   ├── swissprot/
│   │   ├── uniprot_sprot.fasta       # Final file
│   │   ├── uniprot_sprot.download.tmp # Partial download
│   │   └── .uniprot_sprot.fasta.resume # Resume state
```

## Handling Changes

### Server File Changes

If the server file has changed since the download started (detected via ETag mismatch), Talaria will:
1. Notify you that the file has changed
2. Start a fresh download automatically
3. Clean up the old partial file

### Corrupted Partial Files

If the partial file is corrupted or doesn't match the recorded state:
1. Talaria detects the mismatch via hash validation
2. Starts a fresh download
3. Cleans up the corrupted partial file

## Force Download

To ignore resume state and force a fresh download:

```bash
talaria database download uniprot/swissprot --force
```

This will:
- Clear any existing resume state
- Delete partial files
- Start a completely fresh download

## Best Practices

### 1. Large Downloads

For very large databases (>10GB), always use resume:

```bash
# Good practice for large files
talaria database download uniprot/trembl --resume

# Or set as environment variable
export TALARIA_RESUME=1
talaria database download ncbi/nr
```

### 2. Unstable Connections

On unstable connections, combine resume with retries:

```bash
talaria database download uniprot/uniref100 --resume --retries 5
```

### 3. Rate Limiting

To avoid overwhelming servers, use rate limiting with resume:

```bash
talaria database download ncbi/nt --resume --limit-rate 5000  # 5MB/s
```

## Troubleshooting

### Resume Not Working

If resume isn't working as expected:

1. **Check server support**:
   ```bash
   curl -I https://ftp.uniprot.org/path/to/file.gz | grep -i accept-ranges
   ```

2. **Clear resume state**:
   ```bash
   # Remove resume state files
   rm ~/.talaria/databases/uniprot/swissprot/.*.resume
   ```

3. **Use force download**:
   ```bash
   talaria database download uniprot/swissprot --force
   ```

### Partial File Cleanup

To manually clean up partial downloads:

```bash
# Find all partial download files
find ~/.talaria/databases -name "*.download.tmp" -o -name ".*.resume"

# Remove them (careful!)
find ~/.talaria/databases -name "*.download.tmp" -o -name ".*.resume" -delete
```

## Technical Details

### Supported Sources

Resume is supported for all database sources:
- ✅ UniProt (SwissProt, TrEMBL, UniRef)
- ✅ NCBI (NR, NT, RefSeq, Taxonomy)
- ✅ Custom databases
- ✅ PDB (when implemented)
- ✅ PFAM (when implemented)

### State Format

Resume state is stored as JSON:

```json
{
  "url": "https://ftp.ebi.ac.uk/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz",
  "output_path": "/home/user/.talaria/databases/uniprot/trembl/uniprot_trembl.fasta",
  "temp_path": "/home/user/.talaria/databases/uniprot/trembl/uniprot_trembl.download.tmp",
  "bytes_downloaded": 52428800,
  "total_size": 104857600,
  "etag": "\"5d41402abc4b2a76b9719d911017c592\"",
  "last_modified": "Wed, 01 Jan 2025 00:00:00 GMT",
  "partial_hash": {
    "bytes": [/* SHA256 hash bytes */]
  }
}
```

### Performance Considerations

- **Chunk Size**: Downloads in 8KB chunks for efficiency
- **Flush Frequency**: Flushes to disk every 10MB
- **Progress Updates**: Updates state every 10MB
- **Timeout**: 2-hour timeout for very large files

## Integration with SEQUOIA

Resume functionality integrates with SEQUOIA's processing state:
- Tracks download as an operation type
- Allows resume across application restarts
- Maintains data integrity through content-addressing

## Environment Variables

Control resume behavior via environment variables:

```bash
# Always resume by default
export TALARIA_RESUME=1

# Preserve partial files on failure for debugging
export TALARIA_PRESERVE_ON_FAILURE=1

# Set download rate limit (KB/s)
export TALARIA_DOWNLOAD_RATE_LIMIT=5000
```

## See Also

- [Database Downloads](../databases/downloading.md)
- [SEQUOIA Storage](../sequoia/overview.md)
- [Troubleshooting](../sequoia/troubleshooting.md)