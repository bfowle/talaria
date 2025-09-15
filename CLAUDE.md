# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Environment Variables

Talaria uses environment variables to configure paths and behavior:

### Path Configuration
- `TALARIA_HOME`: Base directory for all Talaria data (default: `~/.talaria`)
- `TALARIA_DATA_DIR`: Data directory (default: `$TALARIA_HOME`)
- `TALARIA_DATABASES_DIR`: Database storage directory (default: `$TALARIA_DATA_DIR/databases`)
- `TALARIA_TOOLS_DIR`: External tools directory (default: `$TALARIA_DATA_DIR/tools`)
- `TALARIA_CACHE_DIR`: Cache directory (default: `$TALARIA_DATA_DIR/cache`)

### Logging and Performance
- `TALARIA_LOG`: Log level (error, warn, info, debug, trace)
- `TALARIA_THREADS`: Number of threads to use for parallel processing

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