# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Important Guidelines

### Simplicity First
- DO NOT over-complicate solutions with backwards compatibility unless EXPLICITLY requested
- Prefer simple, clean solutions over complex ones
- Remove old code paths rather than maintaining multiple versions
- When in doubt, choose the simpler approach

## Environment Variables

Talaria uses environment variables to configure paths and behavior:

### Path Configuration
- `TALARIA_HOME`: Base directory for all Talaria data (default: `$HOME/.talaria`)
- `TALARIA_DATA_DIR`: Data directory (default: `$TALARIA_HOME`)
- `TALARIA_DATABASES_DIR`: Database storage directory (default: `$TALARIA_DATA_DIR/databases`)
- `TALARIA_TOOLS_DIR`: External tools directory (default: `$TALARIA_DATA_DIR/tools`)
- `TALARIA_CACHE_DIR`: Cache directory (default: `$TALARIA_DATA_DIR/cache`)
- `TALARIA_WORKSPACE_DIR`: Temporal workspace directory (default: `/tmp/talaria` or `$TMPDIR/talaria`)

### Logging and Performance
- `TALARIA_LOG`: Log level (error, warn, info, debug, trace)
- `TALARIA_THREADS`: Number of threads to use for parallel processing
- `TALARIA_LAMBDA_VERBOSE`: Show detailed LAMBDA aligner output (for debugging)

### Workspace Management
- `TALARIA_PRESERVE_ON_FAILURE`: Keep workspace on errors for debugging
- `TALARIA_PRESERVE_LAMBDA_ON_FAILURE`: Keep LAMBDA workspace specifically
- `TALARIA_PRESERVE_ALWAYS`: Always preserve workspace (for inspection)

### Cloud and Remote Storage
- `TALARIA_MANIFEST_SERVER`: URL for remote manifest storage (S3, GCS, Azure, HTTP)
- `TALARIA_CHUNK_SERVER`: URL for remote chunk storage
- `TALARIA_REMOTE_REPO`: Remote repository URL for CASG sync

## Commands

### Build
```bash
cargo build          # Build debug version
cargo build --release  # Build release version
```

### Run
```bash
cargo run           # Run debug version
cargo run --release  # Run release version
```

### Test
```bash
cargo test          # Run all tests
cargo test [test_name]  # Run specific test
```

### Lint and Format
```bash
cargo fmt           # Format code
cargo clippy        # Run linter
cargo audit         # Check for security vulnerabilities
cargo machete       # Find unused dependencies

# Run all quality checks at once
./scripts/check-quality.sh
```

## Architecture

Talaria is a bioinformatics tool for FASTA sequence database reduction using content-addressed storage. Key components:

### Core Modules
- `src/main.rs`: Entry point and CLI handling
- `src/core/`: Core functionality
  - `paths.rs`: Centralized path configuration using environment variables
  - `database_manager.rs`: Database management with content-addressed storage
  - `reducer.rs`: Sequence reduction algorithms
- `src/casg/`: Content-Addressed Sequence Graph implementation
- `src/bio/`: Bioinformatics utilities (FASTA, taxonomy)
- `src/tools/`: External tool integration (LAMBDA aligner)
- `src/cli/`: Command-line interface modules

### Path Management
All paths are centralized through `src/core/paths.rs` which respects environment variables for configuration. This allows flexible deployment and testing scenarios without hardcoded paths.

### Reduction Workflow
The reduction process follows these steps:
1. **Workspace Creation**: Creates temporary workspace at `${TALARIA_WORKSPACE_DIR}/{id}/` (default: `/tmp/talaria/{id}/`)
2. **Sanitization**: Validates and cleans input sequences
3. **Reference Selection**: Uses LAMBDA aligner (if available) to select optimal references
4. **Delta Encoding**: Encodes non-references as differences from closest references
5. **CASG Storage**: Stores data using content-addressed chunking
6. **Output Generation**: Produces reduced FASTA and delta metadata

### Workspace Structure
```
${TALARIA_WORKSPACE_DIR}/{timestamp}_{uuid}/    # Default: /tmp/talaria/{timestamp}_{uuid}/
├── input/                 # Original input files
├── sanitized/            # Cleaned sequences
├── reference_selection/  # Selection process files
├── alignments/           # LAMBDA alignment data
│   ├── iterations/       # Per-iteration results
│   ├── indices/         # LAMBDA indices
│   └── temp/           # Temporary files
├── output/              # Final output files
├── logs/               # Process logs
└── metadata/           # Workspace metadata
```

### Important Notes
- Always use `crate::core::paths::talaria_home()` instead of hardcoding paths
- Workspace is automatically cleaned up unless preservation is enabled
- LAMBDA aligner now uses CASG workspace instead of system /tmp directory
- All temporary operations go through TempWorkspace for proper tracking

### UI and Output Guidelines
- Use Unicode symbols for terminal output visualization (e.g., ✓, ●, ○, ✗, ─, ▶, ├─, └─)
- Avoid emoji characters in favor of standard Unicode symbols to ensure terminal compatibility
- Prefer structured output with tree-like formatting for hierarchical information display

## Architecture Guidelines

### Trait Organization and Design

When implementing traits in Talaria, follow these principles:

#### 1. Colocate with Functionality
Traits should live in the same module as their primary implementation:
- `MerkleVerifiable` trait lives in `src/casg/merkle.rs`
- `TemporalVersioned` trait lives in `src/casg/temporal.rs`
- `ChunkingStrategy` trait lives in `src/casg/chunker/mod.rs`

**DO NOT** create a catch-all `src/traits/` directory. Exceptions are only for truly cross-cutting concerns used across multiple unrelated modules.

#### 2. Traits Represent Capabilities, Not Data Structures
- ✅ Good: `MerkleVerifiable` (can be verified), `Queryable`, `Addressable`
- ❌ Bad: `MerkleTree` (is a data structure), `Database` (too broad)

#### 3. Example Patterns
```rust
// Good - capability trait with its implementation
// src/casg/merkle.rs
pub trait MerkleVerifiable {
    fn compute_hash(&self) -> SHA256Hash;
}

pub struct MerkleDAG { ... }
impl MerkleVerifiable for ChunkMetadata { ... }

// Bad - trait in separate location
// src/traits/merkle_verifiable.rs
pub trait MerkleVerifiable { ... }
```

#### 4. Design Principles
- Keep traits focused on single responsibilities
- Use associated types for related type definitions
- Provide default implementations where sensible
- Prefer generics over trait objects (`dyn Trait`)
- Document trait contracts clearly
- Use semantic naming (adjectives/verbs over nouns)

## Testing

### Testing Philosophy
- Write tests for business logic and algorithms, not for Rust language features or third-party libraries
- Focus on testing behavior and contracts, not implementation details
- Every bug fix should include a regression test to prevent recurrence
- Aim for practical coverage of critical paths, not 100% coverage

### What to Test
**DO Test:**
- Core algorithms (reduction, chunking, delta encoding)
- Database operations (CASG storage, manifest handling)
- Command-line argument parsing and validation
- Data transformations and conversions
- Error handling and edge cases
- Integration between major components
- Regression cases for fixed bugs

**DON'T Test:**
- Standard library functionality (e.g., Vec::push works)
- Third-party crate functionality (they have their own tests)
- Simple getters/setters without logic
- Generated code (e.g., derive macros)
- UI formatting details (unless critical to functionality)

### Test Organization
```
tests/                      # Integration tests
├── database_fetch_tests.rs  # Tests for fetching sequences by TaxID
├── reduce_casg_tests.rs     # Tests for CASG-based reduction
├── lambda_integration_tests.rs  # LAMBDA aligner integration
└── common/                  # Shared test utilities
    └── mod.rs

src/*/                      # Unit tests (in same file as code)
└── #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_specific_behavior() { ... }
    }
```

### Running Tests
```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test database_fetch_tests

# Run specific test function
cargo test test_parse_taxids

# Run with output for debugging
cargo test -- --nocapture

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --tests

# Run with specific environment
TALARIA_HOME=/tmp/test cargo test

# Run ignored tests (requires setup)
cargo test -- --ignored
```

### Writing Tests

#### Unit Test Example
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_size_calculation() {
        let strategy = ChunkingStrategy::default();
        let size = calculate_chunk_size(&strategy, 1000);
        assert!(size <= strategy.max_chunk_size);
        assert!(size >= strategy.min_chunk_size);
    }
}
```

#### Integration Test Example
```rust
use tempfile::TempDir;

#[test]
fn test_database_workflow() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("TALARIA_HOME", temp_dir.path());

    // Test complete workflow
    let manager = DatabaseManager::new(None).unwrap();
    // ... test operations ...

    std::env::remove_var("TALARIA_HOME");
}
```

#### Regression Test Example
```rust
// Regression test for issue #123: Incorrect handling of empty sequences
#[test]
fn test_empty_sequence_handling_regression() {
    let empty_seq = Sequence {
        id: "empty".to_string(),
        sequence: vec![],
        // ...
    };

    // This used to panic, now should handle gracefully
    let result = process_sequence(&empty_seq);
    assert!(result.is_err());
}
```

### Test Utilities
Common test helpers are in `tests/common/mod.rs`:
- `create_test_sequences()` - Generate test FASTA sequences
- `setup_test_database()` - Create temporary CASG database
- `assert_manifest_valid()` - Validate manifest structure
- `with_temp_env()` - Run test with temporary environment

### Test Data
- Small test files should be inline in tests
- Large test files go in `tests/data/` (git-ignored if >1MB)
- Use `include_str!()` for static test data
- Generate test data programmatically when possible

### Continuous Integration
Tests run automatically on:
- Every push to main branch
- All pull requests
- Can be run locally with `cargo test` before pushing

### Coverage
While not aiming for 100% coverage, ensure:
- All public APIs have basic tests
- Critical algorithms have thorough tests
- Error paths are tested
- Integration points are covered

Check coverage locally:
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out html
open tarpaulin-report.html
```