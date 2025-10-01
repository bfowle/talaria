# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Important Guidelines

### CRITICAL: Search Before Adding
**BEFORE adding ANY new functionality:**
1. ALWAYS search for existing implementations first using Grep/Glob
2. Check if the functionality already exists and just needs fixing
3. Look for similar patterns in the codebase to understand conventions
4. DO NOT add new methods/functions without verifying they don't already exist
5. DO NOT hallucinate method names or assume functionality exists without checking

**This prevents:**
- Code bloat from duplicate functionality
- Compilation errors from non-existent methods
- Diverging from established patterns
- Creating unnecessary complexity

### Simplicity First
- DO NOT over-complicate solutions with backwards compatibility unless EXPLICITLY requested
- Prefer simple, clean solutions over complex ones
- Remove old code paths rather than maintaining multiple versions
- When in doubt, choose the simpler approach

### CRITICAL: Never Pollute Production Code with Test-Only Variants

**This is a fundamental anti-pattern that MUST be avoided at all costs.**

#### The Problem
Adding test-specific variants to production enums, structs, or APIs creates technical debt and blurs the separation between production and test code.

**Anti-pattern (FORBIDDEN):**
```rust
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
    Test,  // ❌ WRONG - test pollution in production enum
}

pub struct Config {
    pub database: String,
    pub is_test_mode: bool,  // ❌ WRONG - test flag in production struct
}
```

#### The Solution
Use proper testing methodologies that keep test code separate from production code.

**Correct approach:**
```rust
// Production code stays clean - no test-specific variants
pub enum DatabaseSource {
    UniProt(UniProtDatabase),
    NCBI(NCBIDatabase),
    Custom(String),
}

// In tests - use real variants or proper mocking
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // Option 1: Use a real variant with test data
        let source = DatabaseSource::Custom("test_database".to_string());

        // Option 2: Use mockall for complex behavior
        let mock_source = MockDatabaseSource::new();
        mock_source.expect_fetch().returning(|_| Ok(test_data()));

        // Option 3: Use builder pattern from talaria-test crate
        let source = TestDatabaseBuilder::new()
            .with_name("test")
            .build();
    }
}
```

#### Proper Testing Practices

**1. Use Existing Variants with Test Data**
```rust
// Good - uses Custom variant for testing
let test_source = DatabaseSource::Custom("integration_test".to_string());
```

**2. Create Test Fixtures in talaria-test Crate**
```rust
// In talaria-test/src/fixtures.rs
pub struct TestDatabaseBuilder {
    name: String,
    sequences: Vec<Sequence>,
}

impl TestDatabaseBuilder {
    pub fn new() -> Self { /* ... */ }
    pub fn with_name(mut self, name: &str) -> Self { /* ... */ }
    pub fn build(self) -> DatabaseSource { /* ... */ }
}
```

**3. Use mockall for Interface Mocking**
```rust
#[cfg(test)]
use mockall::{automock, predicate::*};

#[automock]
pub trait DatabaseProvider {
    fn fetch_sequences(&self) -> Result<Vec<Sequence>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_mock() {
        let mut mock = MockDatabaseProvider::new();
        mock.expect_fetch_sequences()
            .returning(|| Ok(vec![test_sequence()]));
    }
}
```

**4. Use Conditional Compilation for Test Utilities**
```rust
#[cfg(test)]
impl DatabaseSource {
    /// Test-only constructor - only available in test builds
    pub fn new_test(name: &str) -> Self {
        Self::Custom(name.to_string())
    }
}
```

#### What This Prevents
- **Code Bloat**: Production binaries don't carry test-only code
- **API Confusion**: Users don't see test variants in documentation
- **Maintenance Burden**: No need to handle test cases in production match statements
- **Security**: Test-only paths can't be accidentally triggered in production
- **Clean Architecture**: Clear separation between production and test concerns

#### Guidelines Summary
1. **NEVER** add test-specific enum variants to production enums
2. **NEVER** add `is_test`, `test_mode`, or similar flags to production structs
3. **ALWAYS** use existing variants with test data for simple cases
4. **ALWAYS** use mockall or similar for complex mocking needs
5. **ALWAYS** put test fixtures in the `talaria-test` crate
6. **ALWAYS** use `#[cfg(test)]` for test-only implementations
7. **ALWAYS** question if production code needs to know about testing

#### Examples in Talaria
- ✅ Use `DatabaseSource::Custom("test".to_string())` in tests
- ✅ Create `TestSequenceBuilder` in `talaria-test`
- ✅ Mock interfaces with `mockall`
- ❌ Don't add `DatabaseSource::Test`
- ❌ Don't add `Config { test_mode: bool }`
- ❌ Don't add `if cfg!(test)` branches in production logic

## Commit Message Guidelines

This project follows the [Conventional Commits](https://www.conventionalcommits.org/) specification for commit messages. This provides a standardized format that enables automated tooling and clear communication of changes.

### Format
```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Types
- `feat`: A new feature
- `fix`: A bug fix
- `docs`: Documentation only changes
- `style`: Changes that do not affect the meaning of the code (white-space, formatting, etc)
- `refactor`: A code change that neither fixes a bug nor adds a feature
- `perf`: A code change that improves performance
- `test`: Adding missing tests or correcting existing tests
- `build`: Changes that affect the build system or external dependencies (cargo, etc)
- `ci`: Changes to CI configuration files and scripts
- `chore`: Other changes that don't modify src or test files
- `revert`: Reverts a previous commit

### Breaking Changes
- Add `!` after the type/scope to indicate a breaking change: `refactor!: drop support for Rust 1.60`
- Alternatively, include `BREAKING CHANGE:` in the footer

### Examples
```
feat: add SEQUOIA bitemporal storage system

feat(reducer): implement parallel sequence processing

fix: correct delta encoding for edge case sequences

docs: update README with new configuration options

refactor!: restructure module hierarchy for better separation

BREAKING CHANGE: Public API for DatabaseManager has changed

chore: update dependencies to latest versions

test: add integration tests for taxonomy queries
```

### Guidelines
- Use the imperative mood ("add" not "adds" or "added")
- Don't capitalize the first letter after the colon
- No period at the end of the subject line
- Keep the subject line under 72 characters
- Reference issues and PRs in the footer when applicable
- Be specific and descriptive about what changed and why

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
- `TALARIA_REMOTE_REPO`: Remote repository URL for SEQUOIA sync

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
- `src/sequoia/`: Sequence Query Optimization with Indexed Architecture implementation
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
5. **SEQUOIA Storage**: Stores data using content-addressed chunking
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
- LAMBDA aligner now uses SEQUOIA workspace instead of system /tmp directory
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
- `MerkleVerifiable` trait lives in `src/sequoia/merkle.rs`
- `TemporalVersioned` trait lives in `src/sequoia/temporal.rs`
- `ChunkingStrategy` trait lives in `src/sequoia/chunker/mod.rs`

**DO NOT** create a catch-all `src/traits/` directory. Exceptions are only for truly cross-cutting concerns used across multiple unrelated modules.

#### 2. Traits Represent Capabilities, Not Data Structures
- ✅ Good: `MerkleVerifiable` (can be verified), `Queryable`, `Addressable`
- ❌ Bad: `MerkleTree` (is a data structure), `Database` (too broad)

#### 3. Example Patterns
```rust
// Good - capability trait with its implementation
// src/sequoia/merkle.rs
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
- Database operations (SEQUOIA storage, manifest handling)
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
├── reduce_sequoia_tests.rs     # Tests for SEQUOIA-based reduction
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
- `setup_test_database()` - Create temporary SEQUOIA database
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

#### Coverage Tool: cargo-llvm-cov

We use `cargo-llvm-cov` for coverage analysis as it's faster and more reliable than alternatives.

#### Quick Start
```bash
# Install coverage tool (one-time setup)
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview

# Run coverage for entire workspace
./scripts/coverage.sh

# Generate HTML report
./scripts/coverage.sh --html

# Generate and open HTML report
./scripts/coverage.sh --html --open
```

#### Per-Crate Coverage
```bash
# Coverage for specific crates
./scripts/coverage.sh talaria-sequoia
./scripts/coverage.sh talaria-core
./scripts/coverage.sh talaria-bio
./scripts/coverage.sh talaria-utils
./scripts/coverage.sh talaria-storage
./scripts/coverage.sh talaria-tools
./scripts/coverage.sh talaria-cli
./scripts/coverage.sh talaria-test

# Generate HTML for specific crate
./scripts/coverage.sh talaria-sequoia --html --open
```

#### Advanced Usage
```bash
# Show uncovered lines
./scripts/coverage.sh --show-missing

# Clean previous coverage data
./scripts/coverage.sh --clean

# Generate LCOV report (for CI/CD)
./scripts/coverage.sh --lcov

# Generate JSON report
./scripts/coverage.sh --json

# Coverage for library code only (exclude tests)
cargo llvm-cov --lib

# Coverage for specific test
cargo llvm-cov --test download_manager_integration

# Coverage with specific features
cargo llvm-cov --features "feature1,feature2"
```

#### Coverage Reports

- **Terminal**: Default output shows coverage percentages
- **HTML**: Interactive report at `target/llvm-cov/html/index.html`
- **LCOV**: Machine-readable at `target/coverage.lcov`
- **JSON**: Detailed data at `target/coverage.json`

#### Coverage Guidelines

**Target Coverage by Component:**
- Core algorithms (reduction, chunking): >80%
- Public APIs: >70%
- Database operations: >75%
- CLI commands: >60%
- Error handling: >70%
- Utility functions: >50%

**Excluding from Coverage:**
- Generated code
- Debug/display implementations
- Simple getters/setters
- Third-party trait implementations

#### CI Integration

Coverage reports are automatically generated in CI and can be viewed:
- In pull request comments (if configured)
- As build artifacts
- On coverage tracking services (codecov.io, coveralls.io)

#### Interpreting Coverage

- **Line Coverage**: Percentage of code lines executed
- **Branch Coverage**: Percentage of conditional branches tested
- **Function Coverage**: Percentage of functions called

Focus on increasing coverage for:
1. Complex business logic
2. Error conditions
3. Edge cases
4. Public API boundaries

#### Quick Commands Reference
```bash
# Most common commands
./scripts/coverage.sh --html --open  # View coverage in browser
./scripts/coverage.sh --show-missing # Find untested code
./scripts/coverage.sh talaria-core   # Test specific crate
```