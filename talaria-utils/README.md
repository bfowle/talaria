# Talaria Utils

## Overview

`talaria-utils` is a foundational utility module that provides common functionality shared across all Talaria components. It serves as the backbone for user interaction, database management, workspace orchestration, and parallel processing throughout the Talaria ecosystem.

This module is designed with the following principles:
- **Modularity**: Organized into focused, single-purpose submodules
- **Reusability**: Common patterns extracted and generalized for system-wide use
- **Performance**: Optimized for large-scale bioinformatics operations
- **User Experience**: Rich terminal output with progress tracking and formatted displays

### Key Responsibilities

1. **Database Management**: Reference parsing, version detection, and aliasing
2. **Display & UI**: Terminal formatting, progress bars, tree visualization, and tables
3. **Workspace Management**: Temporary file orchestration with SEQUOIA integration
4. **Parallel Processing**: Thread pool management and parallelization utilities

## Architecture

### Module Organization

```
talaria-utils/
├── src/
│   ├── database/          # Database reference and version management
│   │   ├── mod.rs         # Module exports
│   │   ├── reference.rs   # Database reference types and parsing
│   │   └── version.rs     # Version detection and aliasing
│   │
│   ├── display/           # User interface and formatting
│   │   ├── mod.rs         # Module exports
│   │   ├── format.rs      # Byte/duration/size formatting
│   │   ├── formatter.rs   # Structured output with sections
│   │   ├── output.rs      # Trees, tables, and colored messages
│   │   └── progress.rs    # Progress bars and spinners
│   │
│   ├── workspace/         # Workspace management
│   │   ├── mod.rs         # Module exports
│   │   ├── temp.rs        # Temporary workspace lifecycle
│   │   └── sequoia.rs     # SEQUOIA-specific workspace operations
│   │
│   ├── parallel.rs        # Parallel processing utilities
│   └── lib.rs             # Public API exports
```

### Design Patterns

1. **Builder Pattern**: Used in workspace configuration and tree node construction
2. **Strategy Pattern**: Version detection with pluggable detector implementations
3. **Facade Pattern**: Simplified public API hiding complex internal implementations
4. **Resource Management**: RAII pattern for workspace cleanup

## Database Module

The database module provides sophisticated database reference management with support for versioning, aliasing, and profile selection.

### Core Components

#### DatabaseReference

Represents a complete database reference with source, dataset, version, and reduction profile.

```rust
// DatabaseReference is re-exported from talaria-core for convenience
use talaria_utils::database::DatabaseReference;  // Or: use talaria_core::types::DatabaseReference;

// Basic reference
let db = DatabaseReference::new(
    "uniprot".to_string(),
    "swissprot".to_string()
);

// Full reference with version and profile
let db = DatabaseReference::with_all(
    "uniprot".to_string(),
    "swissprot".to_string(),
    Some("2024_04".to_string()),
    Some("50-percent".to_string()),
);

// Parse from string format
let db = parse_database_reference("uniprot/swissprot@2024_04:50-percent")?;
```

**String Format**: `source/dataset[@version][:profile]`
- `uniprot/swissprot` - Latest version with auto-detect profile
- `ncbi/nr@stable` - Stable version alias
- `custom/mydb:minimal` - Current version with minimal profile
- `uniprot/trembl@2024_04:50-percent` - Specific version and profile

#### Version Detection and Management

The `VersionDetector` automatically identifies upstream database versions from file headers and content:

```rust
use talaria_utils::database::{VersionDetector, DatabaseVersion};

let detector = VersionDetector::new();

// Detect UniProt version from FASTA headers
let version = detector.detect_from_fasta(&path, "uniprot", "swissprot")?;
// Returns: DatabaseVersion with upstream_version: Some("2024_04")

// Version aliases support multiple naming schemes
let version = DatabaseVersion {
    timestamp: "20250915_053033".to_string(),
    upstream_version: Some("2024_04".to_string()),
    aliases: VersionAliases {
        system: vec!["current".to_string(), "latest".to_string()],
        upstream: vec!["2024_04".to_string()],
        custom: vec!["paper-2024".to_string()],
    },
    // ... other fields
};
```

**Supported Detection**:
- **UniProt**: Extracts release from headers (e.g., `OS=... OX=... GN=...`)
- **NCBI**: Parses GenBank/RefSeq format headers
- **PDB**: Detects RCSB PDB versioning
- **Custom**: User-defined version patterns via regex

#### Version Store

Abstract interface for version persistence and querying:

```rust
use talaria_utils::database::VersionManager;

let manager = VersionManager::new(database_dir)?;

// Register a new version with aliases
manager.register_version(version, vec!["stable", "production"])?;

// Query versions by alias
let version = manager.resolve_alias("stable")?;

// List all versions for a database
let versions = manager.list_versions("uniprot", "swissprot")?;
```

## Display Module

The display module provides rich terminal output capabilities with formatting, progress tracking, and visualization.

### Formatting Utilities

#### Byte and Duration Formatting

```rust
use talaria_utils::display::{format_bytes, format_duration, get_file_size};

// Human-readable byte formatting
println!("{}", format_bytes(15728640)); // "15.0 MB"
println!("{}", format_bytes(1073741824)); // "1.0 GB"

// Duration formatting
use std::time::Duration;
println!("{}", format_duration(Duration::from_secs(90))); // "1m 30s"
println!("{}", format_duration(Duration::from_secs(3665))); // "1h 1m 5s"

// File size helper
let size = get_file_size("/path/to/file.fasta")?;
println!("File size: {}", format_bytes(size));
```

### Structured Output Formatter

Create organized, hierarchical output with status indicators:

```rust
use talaria_utils::display::{OutputFormatter, Section, Item, Status};

let mut formatter = OutputFormatter::new();

// Add a section with items
let mut section = Section::new("Database Processing");
section.add_item(
    Item::new("Loading sequences")
        .with_value("15,234 sequences")
        .with_status(Status::Complete)
);
section.add_item(
    Item::new("Building index")
        .with_status(Status::InProgress)
);
section.set_status(Status::InProgress);

formatter.add_section(section);
formatter.render(); // Prints formatted output

// Output:
// [⏳] Database Processing
//   [✓] Loading sequences: 15,234 sequences
//   [⏳] Building index
```

**Status Indicators**:
- `[ ]` - Pending
- `[⏳]` - InProgress
- `[✓]` - Complete
- `[✗]` - Failed
- `[⊘]` - Skipped

### Tree Visualization

Display hierarchical data structures:

```rust
use talaria_utils::display::TreeNode;

let tree = TreeNode::new("Database Structure")
    .add_child(
        TreeNode::new("uniprot/")
            .add_child(TreeNode::new("swissprot/")
                .add_child(TreeNode::new("2024_04/"))
                .add_child(TreeNode::new("2024_03/"))
            )
            .add_child(TreeNode::new("trembl/"))
    )
    .add_child(
        TreeNode::new("ncbi/")
            .add_child(TreeNode::new("nr/"))
            .add_child(TreeNode::new("nt/"))
    );

println!("{}", tree.render());

// Output:
// Database Structure
// ├─ uniprot/
// │  ├─ swissprot/
// │  │  ├─ 2024_04/
// │  │  └─ 2024_03/
// │  └─ trembl/
// └─ ncbi/
//    ├─ nr/
//    └─ nt/
```

### Tables

Create formatted tables using comfy-table:

```rust
use talaria_utils::display::{create_standard_table, header_cell};

let mut table = create_standard_table();
table.set_header(vec![
    header_cell("Database"),
    header_cell("Version"),
    header_cell("Sequences"),
    header_cell("Size"),
]);

table.add_row(vec![
    "UniProt/SwissProt",
    "2024_04",
    "571,282",
    "256 MB",
]);
table.add_row(vec![
    "NCBI nr",
    "2024-09-15",
    "625,353,169",
    "380 GB",
]);

println!("{}", table);
```

### Colored Messages

Predefined message types with consistent styling:

```rust
use talaria_utils::display::{info, success, warning, error};

info("Processing database...");      // ℹ Blue message
success("Database loaded!");         // ✓ Green message
warning("Large file detected");      // ⚠ Yellow message
error("Failed to open file");        // ✗ Red message
```

### Progress Tracking

Create progress bars and spinners for long-running operations:

```rust
use talaria_utils::display::{create_progress_bar, create_spinner, ProgressBarManager};

// Simple progress bar
let pb = create_progress_bar(1000);
for i in 0..1000 {
    pb.set_position(i);
    // Do work...
}
pb.finish_with_message("Complete!");

// Spinner for indeterminate progress
let spinner = create_spinner("Processing...");
// Do work...
spinner.finish_with_message("✓ Done");

// Managed progress bars for multiple operations
let manager = ProgressBarManager::new();
let pb1 = manager.add_progress_bar(100, "Task 1");
let pb2 = manager.add_progress_bar(200, "Task 2");
// Update independently...
```

## Workspace Module

The workspace module manages temporary file operations with automatic cleanup and SEQUOIA integration.

### Temporary Workspace

Self-cleaning temporary directories with metadata tracking:

```rust
use talaria_utils::workspace::{TempWorkspace, WorkspaceConfig};

// Note: WorkspaceConfig, WorkspaceStats, and WorkspaceMetadata are the canonical
// workspace types, consolidated here to avoid duplication across modules

// Basic workspace
let workspace = TempWorkspace::new("reduce")?;
let input_dir = workspace.create_subdir("input")?;
let output_dir = workspace.create_subdir("output")?;

// Custom configuration
let config = WorkspaceConfig {
    sequoia_root: PathBuf::from("/custom/path"),
    preserve_on_failure: true,  // Keep on error
    preserve_always: false,      // Auto-cleanup
    max_age_seconds: 86400,      // 24 hours
};
let workspace = TempWorkspace::with_config("analyze", config)?;

// Workspace automatically cleans up on drop unless preserved
```

**Directory Structure**:
```
${TALARIA_WORKSPACE_DIR}/
├── 20250915_053033_abc123/     # Timestamp + UUID
│   ├── metadata.json            # Workspace metadata
│   ├── input/                   # Created subdirectories
│   ├── output/
│   └── temp/
```

**Environment Variables**:
- `TALARIA_WORKSPACE_DIR`: Override workspace root (default: `/tmp/talaria`)
- `TALARIA_PRESERVE_ON_FAILURE`: Keep workspace on errors
- `TALARIA_PRESERVE_ALWAYS`: Never auto-delete workspaces

### Workspace Metadata and Stats

Track workspace lifecycle and statistics:

```rust
use talaria_utils::workspace::{WorkspaceMetadata, WorkspaceStatus, WorkspaceStats};

let mut metadata = WorkspaceMetadata {
    id: workspace.id.clone(),
    created_at: timestamp,
    command: "reduce".to_string(),
    input_file: Some("input.fasta".to_string()),
    output_file: Some("output.fasta".to_string()),
    status: WorkspaceStatus::Active,
    error_message: None,
    stats: WorkspaceStats {
        input_sequences: 50000,
        sanitized_sequences: 49850,
        removed_sequences: 150,
        selected_references: 100,
        alignment_iterations: 3,
        total_alignments: 148500,
        final_output_sequences: 100,
    },
};

// Update and persist
workspace.update_metadata(metadata)?;
```

### SEQUOIA Workspace Manager

Content-addressed storage integration for workspace files:

```rust
use talaria_utils::workspace::{SequoiaWorkspaceManager, SequoiaTransaction};

let mut manager = SequoiaWorkspaceManager::new()?;

// Create SEQUOIA-managed workspace
let workspace = manager.create_workspace("reduce")?;

// Content-addressed file storage
let content_hash = manager.add_file(&workspace, file_path)?;

// Start a transaction for atomic operations
let transaction = SequoiaTransaction::new(&workspace);
transaction.add_file("sequences.fasta", content)?;
transaction.add_metadata("stats.json", stats)?;
transaction.commit()?;  // Atomic commit

// Generate statistics
let stats = manager.get_statistics(&workspace)?;
println!("Unique chunks: {}", stats.unique_chunks);
println!("Dedup ratio: {:.1}%", stats.deduplication_ratio * 100.0);
```

### Workspace Discovery

Find and manage existing workspaces:

```rust
use talaria_utils::workspace::{list_workspaces, find_workspace};

// List all workspaces
let workspaces = list_workspaces()?;
for ws in workspaces {
    println!("{}: {} - {:?}", ws.id, ws.command, ws.status);
}

// Find specific workspace
if let Some(ws) = find_workspace("20250915_053033_abc123")? {
    println!("Found workspace: {:?}", ws.root);
}

// Clean old workspaces
TempWorkspace::cleanup_old(Duration::from_secs(86400))?; // 24 hours
```

## Parallel Processing Module

Utilities for efficient parallel processing using Rayon:

```rust
use talaria_utils::parallel::{
    configure_thread_pool,
    chunk_size_for_parallelism,
    get_available_cores,
    should_parallelize,
};

// Configure global thread pool
configure_thread_pool(8)?;  // Use 8 threads
configure_thread_pool(0)?;  // Use all available cores

// Calculate optimal chunk size
let chunk_size = chunk_size_for_parallelism(100000, 8);
// Returns size optimized for 8 threads processing 100k items

// Check if parallelization is beneficial
if should_parallelize(items.len(), 1000) {
    // Process in parallel if >1000 items and multiple threads available
    items.par_iter().for_each(|item| process(item));
} else {
    // Sequential processing for small datasets
    items.iter().for_each(|item| process(item));
}

// Get system information
let cores = get_available_cores();
println!("Available CPU cores: {}", cores);
```

**Optimization Strategies**:
- Chunks aim for 10-100 items per thread
- Maximum chunk size capped at 1000 for cache efficiency
- Minimum threshold prevents overhead for small datasets

## Integration with Talaria Ecosystem

### Used By

1. **talaria-cli**: Primary consumer for all user-facing output
   - Progress bars during reduction
   - Database reference parsing
   - Workspace management for temp files
   - Formatted output for results

2. **talaria-sequoia**: Workspace and formatting utilities
   - SEQUOIA workspace management
   - Version detection for manifest creation
   - Progress tracking for large operations

3. **talaria-tools**: Display and workspace utilities
   - Progress bars for tool downloads
   - Temporary directories for tool installation
   - Formatted output for tool status

### Integration Examples

#### CLI Integration
```rust
// In talaria-cli
use talaria_utils::{
    workspace::TempWorkspace,
    display::{create_progress_bar, success, error},
    database::parse_database_reference,
};

let db = parse_database_reference(&args.database)?;
let workspace = TempWorkspace::new("reduce")?;
let pb = create_progress_bar(total_sequences);

// Process with progress
for seq in sequences {
    pb.inc(1);
    process_sequence(seq, &workspace)?;
}

pb.finish();
success(&format!("Processed {} sequences", total_sequences));
```

#### SEQUOIA Integration
```rust
// In talaria-sequoia
use talaria_utils::{
    workspace::SequoiaWorkspaceManager,
    database::VersionDetector,
};

let manager = SequoiaWorkspaceManager::new()?;
let detector = VersionDetector::new();

// Detect version for manifest
let version = detector.detect_from_fasta(&path, source, dataset)?;
manifest.version = version.upstream_version;
```

## Configuration

### Environment Variables

| Variable | Module | Description | Default |
|----------|--------|-------------|---------|
| `TALARIA_HOME` | All | Base directory for Talaria | `~/.talaria` |
| `TALARIA_WORKSPACE_DIR` | Workspace | Temp workspace root | `/tmp/talaria` or `$TMPDIR/talaria` |
| `TALARIA_PRESERVE_ON_FAILURE` | Workspace | Keep workspace on errors | `false` |
| `TALARIA_PRESERVE_ALWAYS` | Workspace | Never auto-delete | `false` |
| `TALARIA_PRESERVE_LAMBDA_ON_FAILURE` | Workspace | Keep LAMBDA workspaces | `false` |
| `NO_COLOR` | Display | Disable colored output | `false` |
| `TALARIA_PROGRESS` | Display | Progress bar style | `auto` |

### Runtime Configuration

```rust
// Workspace configuration
let config = WorkspaceConfig {
    sequoia_root: custom_path,
    preserve_on_failure: true,
    preserve_always: false,
    max_age_seconds: 3600,
};

// Thread pool configuration
configure_thread_pool(num_threads)?;

// Progress bar styling
std::env::set_var("TALARIA_PROGRESS", "plain"); // Simple ASCII
std::env::set_var("TALARIA_PROGRESS", "fancy"); // Unicode characters
```

## API Reference

### Database Module

```rust
// Types
pub struct DatabaseReference { /* fields */ }
pub struct DatabaseVersion { /* fields */ }
pub struct VersionAliases { /* fields */ }
pub struct VersionDetector { /* fields */ }
pub struct VersionManager { /* fields */ }

// Functions
pub fn parse_database_reference(input: &str) -> Result<DatabaseReference>;
pub fn format_database_reference(db: &DatabaseReference) -> String;

// Traits
impl Display for DatabaseReference;
impl FromStr for DatabaseReference;
```

### Display Module

```rust
// Types
pub struct OutputFormatter { /* fields */ }
pub struct Section { /* fields */ }
pub struct Item { /* fields */ }
pub enum Status { Pending, InProgress, Complete, Failed, Skipped }
pub struct TreeNode { /* fields */ }
pub struct ProgressBarManager { /* fields */ }

// Functions
pub fn format_bytes(bytes: u64) -> String;
pub fn format_duration(duration: Duration) -> String;
pub fn format_number<T: Display>(n: T) -> String;
pub fn create_progress_bar(total: u64) -> ProgressBar;
pub fn create_spinner(msg: &str) -> ProgressBar;
pub fn info(msg: &str);
pub fn success(msg: &str);
pub fn warning(msg: &str);
pub fn error(msg: &str);

// Traits
pub trait StatusReporter;
pub trait OutputFormattable;
```

### Workspace Module

```rust
// Types
pub struct TempWorkspace { /* fields */ }
pub struct WorkspaceConfig { /* fields */ }
pub struct WorkspaceMetadata { /* fields */ }
pub enum WorkspaceStatus { Active, Completed, Failed, Preserved }
pub struct WorkspaceStats { /* fields */ }
pub struct SequoiaWorkspaceManager { /* fields */ }
pub struct SequoiaTransaction { /* fields */ }

// Functions
pub fn list_workspaces() -> Result<Vec<WorkspaceMetadata>>;
pub fn find_workspace(id: &str) -> Result<Option<TempWorkspace>>;
```

### Parallel Module

```rust
// Functions
pub fn configure_thread_pool(threads: usize) -> Result<()>;
pub fn chunk_size_for_parallelism(total: usize, threads: usize) -> usize;
pub fn get_available_cores() -> usize;
pub fn should_parallelize(count: usize, threshold: usize) -> bool;
```

## Best Practices

### Error Handling

Always use `Result` types and provide context:

```rust
use anyhow::{Context, Result};

pub fn process_database(path: &Path) -> Result<()> {
    let workspace = TempWorkspace::new("process")
        .context("Failed to create workspace")?;

    let file = File::open(path)
        .with_context(|| format!("Failed to open: {:?}", path))?;

    // ...
}
```

### Resource Management

Use RAII patterns for automatic cleanup:

```rust
// Workspace cleans up automatically
{
    let workspace = TempWorkspace::new("temp")?;
    // Use workspace...
} // Cleaned up here

// Explicit preservation
let workspace = TempWorkspace::new("debug")?
    .with_preserve_on_failure(true);
```

### Progress Tracking

Always provide feedback for long operations:

```rust
let pb = create_progress_bar(items.len() as u64);
pb.set_message("Processing items");

for item in items {
    pb.inc(1);
    process(item)?;
}

pb.finish_with_message("✓ Complete");
```

### Parallel Processing

Profile before parallelizing:

```rust
// Measure overhead
let start = Instant::now();
if should_parallelize(data.len(), 10000) {
    data.par_iter().for_each(process);
} else {
    data.iter().for_each(process);
}
println!("Processed in {:?}", start.elapsed());
```

## Testing

### Unit Tests

Each module has comprehensive unit tests:

```bash
# Run all tests
cargo test --package talaria-utils

# Run specific module tests
cargo test --package talaria-utils database::
cargo test --package talaria-utils display::
cargo test --package talaria-utils workspace::

# Run with output for debugging
cargo test --package talaria-utils -- --nocapture
```

### Integration Tests

Test interactions between modules:

```rust
#[test]
fn test_workspace_with_progress() {
    let workspace = TempWorkspace::new("test").unwrap();
    let pb = create_progress_bar(100);

    for i in 0..100 {
        pb.inc(1);
        let file = workspace.create_file(&format!("file_{}.txt", i)).unwrap();
        // Process file...
    }

    pb.finish();
    assert!(workspace.root.exists());
}
```

### Mock Testing

Use mock implementations for testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_reference_parsing() {
        let test_cases = vec![
            ("uniprot/swissprot", "uniprot", "swissprot", None, None),
            ("ncbi/nr@stable", "ncbi", "nr", Some("stable"), None),
            ("custom/db:minimal", "custom", "db", None, Some("minimal")),
        ];

        for (input, source, dataset, version, profile) in test_cases {
            let db = parse_database_reference(input).unwrap();
            assert_eq!(db.source, source);
            assert_eq!(db.dataset, dataset);
            assert_eq!(db.version.as_deref(), version);
            assert_eq!(db.profile.as_deref(), profile);
        }
    }
}
```

## Performance Considerations

### Memory Usage

- **Progress Bars**: Use `pb.set_draw_delta(n)` to reduce update frequency
- **Tree Rendering**: Limit depth for very large trees
- **Workspace**: Use streaming for large files instead of loading into memory

### Optimization Tips

1. **Batch Progress Updates**:
```rust
let pb = create_progress_bar(total);
pb.set_draw_delta(total / 100); // Update at most 100 times
```

2. **Lazy Workspace Creation**:
```rust
// Only create subdirs when needed
let workspace = TempWorkspace::new("process")?;
if needs_temp_files {
    workspace.create_subdir("temp")?;
}
```

3. **Parallel Chunk Tuning**:
```rust
// Tune chunk size based on workload
let chunk_size = if heavy_processing {
    chunk_size_for_parallelism(items.len(), threads) / 10
} else {
    chunk_size_for_parallelism(items.len(), threads) * 2
};
```

## Troubleshooting

### Common Issues

1. **Workspace Not Cleaned Up**
   - Check `TALARIA_PRESERVE_*` environment variables
   - Verify process didn't crash before cleanup
   - Manual cleanup: `rm -rf ${TALARIA_WORKSPACE_DIR}/*`

2. **Progress Bar Not Showing**
   - Check if output is redirected (not a TTY)
   - Verify `NO_COLOR` environment variable
   - Try `TALARIA_PROGRESS=plain` for simple output

3. **Version Detection Fails**
   - Ensure file has standard database headers
   - Check detector has parser for format
   - Use manual version specification as fallback

4. **Parallel Processing Issues**
   - Verify Rayon thread pool initialized
   - Check for thread safety in closure
   - Profile to ensure parallelization benefit

### Debug Environment Variables

```bash
# Enable debug output
export RUST_LOG=talaria_utils=debug

# Disable all fancy output
export NO_COLOR=1
export TALARIA_PROGRESS=plain

# Preserve workspaces for inspection
export TALARIA_PRESERVE_ALWAYS=1

# Force sequential processing
export RAYON_NUM_THREADS=1
```

## Future Enhancements

### Planned Features

1. **Cloud Storage Support**: S3/GCS backends for workspace storage
2. **Distributed Progress**: Progress aggregation across cluster nodes
3. **Advanced Formatting**: Markdown and HTML output formatters
4. **Metrics Collection**: Performance metrics and telemetry
5. **Interactive Mode**: Terminal UI components for user interaction

### API Stability

The public API is considered stable with semantic versioning:
- **Major**: Breaking changes to public types/functions
- **Minor**: New features, backward compatible
- **Patch**: Bug fixes and internal improvements

## Contributing

Contributions are welcome! Please:

1. Write tests for new functionality
2. Update documentation and examples
3. Follow Rust standard formatting (`cargo fmt`)
4. Ensure backward compatibility when possible

## License

This module is part of the Talaria project and follows the project's licensing terms.
