# Contributing to Talaria

Thank you for your interest in contributing to Talaria! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Development Process](#development-process)
- [Code Style](#code-style)
- [Testing](#testing)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
- [Reporting Issues](#reporting-issues)

## Code of Conduct

Please read and follow our [Code of Conduct](CODE_OF_CONDUCT.md) to ensure a welcoming environment for all contributors.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Set up the development environment
4. Create a feature branch
5. Make your changes
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.70 or later
- Git
- 8GB RAM minimum (16GB recommended)
- Optional: Docker for containerized testing

### Initial Setup

```bash
# Clone your fork
git clone https://github.com/YOUR-USERNAME/talaria.git
cd talaria

# Add upstream remote
git remote add upstream https://github.com/talaria/talaria.git

# Install Rust toolchain components
rustup component add rustfmt clippy

# Install development tools
cargo install cargo-watch cargo-audit cargo-tarpaulin cargo-machete

# Build the project
cargo build --all

# Run tests to verify setup
cargo test --all
```

### Environment Variables

Configure your development environment:

```bash
# Required for development
export TALARIA_HOME="$HOME/.talaria-dev"
export TALARIA_LOG="debug"

# Optional for testing
export TALARIA_PRESERVE_ON_FAILURE=1
export TALARIA_LAMBDA_VERBOSE=1
```

## How to Contribute

### Types of Contributions

#### Bug Fixes
- Check existing issues to avoid duplicates
- Create a minimal reproduction case
- Include system information
- Submit a fix with tests

#### Features
- Discuss major features in an issue first
- Break large features into smaller PRs
- Include documentation and tests
- Update relevant examples

#### Performance Improvements
- Include benchmark results
- Test on multiple datasets
- Document the optimization approach
- Ensure no functionality regression

#### Documentation
- Fix typos and clarify explanations
- Add examples and tutorials
- Improve API documentation
- Translate documentation

### Finding Issues to Work On

Look for issues labeled:
- `good first issue` - Suitable for newcomers
- `help wanted` - Community help needed
- `documentation` - Documentation improvements
- `performance` - Performance optimizations

## Development Process

### 1. Create a Branch

```bash
# Update main branch
git checkout main
git pull upstream main

# Create feature branch
git checkout -b feature/your-feature-name

# Or for bugs
git checkout -b fix/issue-description
```

### 2. Make Changes

Follow the architecture:
- `talaria-core` - Core types and configuration
- `talaria-bio` - Bioinformatics algorithms
- `talaria-storage` - Storage backends
- `talaria-sequoia` - Content-addressed storage
- `talaria-tools` - External tool integration
- `talaria-utils` - Utilities and helpers
- `talaria-cli` - Command-line interface

### 3. Commit Changes

This project follows the [Conventional Commits](https://www.conventionalcommits.org/) specification. Please adhere to this standard for all commit messages.

#### Commit Message Format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

#### Examples

```bash
# Good
git commit -m "feat(sequoia): add parallel chunk processing"
git commit -m "fix(cli): correct memory leak in reduce command"
git commit -m "docs(api): update reducer documentation"
git commit -m "refactor!: restructure module hierarchy"

# Bad
git commit -m "Fixed stuff"
git commit -m "Update"
git commit -m "ADDED NEW FEATURE"
```

#### Commit Types

- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation only changes
- `style:` Changes that don't affect code meaning (whitespace, formatting)
- `refactor:` Code change that neither fixes a bug nor adds a feature
- `perf:` Performance improvement
- `test:` Test addition/modification
- `build:` Changes to build system or dependencies
- `ci:` CI configuration changes
- `chore:` Maintenance tasks
- `revert:` Reverts a previous commit

#### Guidelines

- Use imperative mood ("add" not "adds" or "added")
- Don't capitalize first letter after colon
- No period at the end of the subject line
- Keep subject under 72 characters
- Add `!` after type for breaking changes: `feat!:` or `refactor!:`
- Reference issues/PRs in footer when applicable

For more details, see the full [Commit Message Guidelines](CLAUDE.md#commit-message-guidelines) in CLAUDE.md.

## Code Style

### Rust Guidelines

Follow Rust best practices:

```rust
// Use descriptive names
let sequence_count = sequences.len();  // Good
let n = sequences.len();               // Bad

// Prefer iterators over loops
sequences.iter()
    .filter(|s| s.length > MIN_LENGTH)
    .map(|s| process_sequence(s))
    .collect();

// Handle errors appropriately
let result = operation()?;  // In functions returning Result
if let Err(e) = operation() {
    tracing::error!("Operation failed: {}", e);
}

// Document public APIs
/// Reduces a FASTA database using the specified strategy.
///
/// # Arguments
/// * `input` - Path to input FASTA file
/// * `ratio` - Target reduction ratio (0.0-1.0)
///
/// # Returns
/// Reduced sequences or error
pub fn reduce(input: &Path, ratio: f64) -> Result<Vec<Sequence>> {
    // Implementation
}
```

### Formatting

Always format code before committing:

```bash
# Format code
cargo fmt --all

# Check formatting in CI
cargo fmt --all -- --check

# Run clippy for lints
cargo clippy --all-targets --all-features -- -D warnings

# Fix clippy suggestions
cargo clippy --fix
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test --all

# Run specific module tests
cargo test -p talaria-sequoia

# Run with output for debugging
cargo test -- --nocapture

# Run ignored tests (require setup)
cargo test -- --ignored

# Run benchmarks
cargo bench
```

### Writing Tests

Add tests for:
- New functionality
- Bug fixes (regression tests)
- Edge cases
- Error conditions

Example test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_reduction() {
        let sequences = vec![
            Sequence::new("seq1", b"ACGT"),
            Sequence::new("seq2", b"ACGT"),
            Sequence::new("seq3", b"TGCA"),
        ];

        let reduced = reduce_sequences(&sequences, 0.5).unwrap();

        assert_eq!(reduced.len(), 2);
        assert!(reduced.iter().any(|s| s.id == "seq3"));
    }

    #[test]
    #[should_panic(expected = "invalid ratio")]
    fn test_invalid_ratio() {
        reduce_sequences(&[], 1.5).unwrap();
    }
}
```

### Coverage

Check test coverage:

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out html --output-dir coverage

# Open report
open coverage/tarpaulin-report.html
```

## Documentation

### Code Documentation

Document all public APIs:

```rust
/// Represents a biological sequence with metadata.
///
/// # Examples
///
/// ```
/// use talaria_bio::Sequence;
///
/// let seq = Sequence::new("id1", b"ACGT");
/// assert_eq!(seq.length(), 4);
/// ```
pub struct Sequence {
    /// Unique identifier
    pub id: String,
    /// Sequence data
    pub data: Vec<u8>,
}
```

### Building Documentation

```bash
# Build and open documentation
cargo doc --all --open

# Build with private items
cargo doc --all --document-private-items

# Check documentation examples
cargo test --doc
```

### User Documentation

Update mdBook documentation:

```bash
cd docs
mdbook serve --open

# Build for production
mdbook build
```

## Pull Request Process

### Before Submitting

1. **Update from upstream**
   ```bash
   git fetch upstream
   git rebase upstream/main
   ```

2. **Run quality checks**
   ```bash
   ./scripts/check-quality.sh
   ```
   Or manually:
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features
   cargo test --all
   cargo doc --all
   ```

3. **Update documentation**
   - Add/update API documentation
   - Update README if needed
   - Add examples for new features

4. **Update CHANGELOG**
   - Add entry under "Unreleased"
   - Follow Keep a Changelog format

### PR Checklist

- [ ] Tests pass locally (`cargo test --all`)
- [ ] Code formatted (`cargo fmt --all`)
- [ ] Clippy warnings resolved (`cargo clippy`)
- [ ] Documentation updated
- [ ] CHANGELOG entry added
- [ ] Commits are logical and well-described
- [ ] PR description explains the change
- [ ] Linked to relevant issue (if applicable)

### Submitting the PR

1. Push to your fork
2. Create PR from your fork to upstream main
3. Fill out the PR template
4. Wait for CI to pass
5. Address review feedback
6. Squash commits if requested

### Review Process

- PRs require at least one approval
- CI must pass (tests, formatting, clippy)
- Maintainers may request changes
- Be responsive to feedback
- Be patient - reviews take time

## Reporting Issues

### Bug Reports

Include:
- Talaria version (`talaria --version`)
- Operating system and version
- Rust version (`rustc --version`)
- Steps to reproduce
- Expected vs actual behavior
- Error messages and logs
- Sample data (if possible)

### Feature Requests

Include:
- Use case description
- Current workarounds (if any)
- Proposed solution
- Alternative solutions considered
- Impact on existing functionality

### Security Issues

**DO NOT** open public issues for security vulnerabilities.
See [SECURITY.md](SECURITY.md) for the security reporting process.

## Development Tips

### Debugging

```bash
# Enable debug logging
export TALARIA_LOG=debug

# Enable backtrace for errors
export RUST_BACKTRACE=1

# Preserve workspace for inspection
export TALARIA_PRESERVE_ON_FAILURE=1

# Use GDB/LLDB
rust-gdb target/debug/talaria
rust-lldb target/debug/talaria
```

### Performance Profiling

```bash
# CPU profiling with flamegraph
cargo install flamegraph
cargo build --release
flamegraph target/release/talaria reduce -i input.fasta

# Memory profiling with Valgrind
valgrind --tool=massif target/release/talaria reduce -i input.fasta
ms_print massif.out.<pid>

# Benchmark specific functions
cargo bench --bench reduction_benchmark
```

### Useful Commands

```bash
# Watch for changes and rebuild
cargo watch -x build

# Find unused dependencies
cargo machete

# Check for security vulnerabilities
cargo audit

# Update dependencies
cargo update

# Clean build artifacts
cargo clean
```

## Getting Help

- Read the [documentation](docs/)
- Check [existing issues](https://github.com/talaria/talaria/issues)
- Ask in [discussions](https://github.com/talaria/talaria/discussions)
- Join our community chat (if available)

## Recognition

Contributors are recognized in:
- [CONTRIBUTORS.md](CONTRIBUTORS.md)
- Release notes
- Project documentation

Thank you for contributing to Talaria!