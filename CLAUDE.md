# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

This is a minimal Rust project using Cargo as the build system. The project structure follows standard Rust conventions:

- `src/main.rs`: Entry point of the application
- `Cargo.toml`: Project manifest and dependency management
- `target/`: Build artifacts (excluded from version control)

The project is currently in its initial state with a basic "Hello, world!" implementation.