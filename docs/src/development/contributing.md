# Contributing

Welcome to the Talaria project! We appreciate your interest in contributing to this bioinformatics tool for sequence database reduction.

## Code of Conduct

### Our Pledge

We pledge to make participation in our project a harassment-free experience for everyone, regardless of age, body size, disability, ethnicity, gender identity, level of experience, nationality, personal appearance, race, religion, or sexual identity and orientation.

### Our Standards

**Positive behaviors include:**
- Using welcoming and inclusive language
- Being respectful of differing viewpoints
- Gracefully accepting constructive criticism
- Focusing on what is best for the community
- Showing empathy towards other community members

**Unacceptable behaviors include:**
- Trolling, insulting/derogatory comments, and personal attacks
- Public or private harassment
- Publishing others' private information
- Other conduct which could reasonably be considered inappropriate

## Getting Started

### Prerequisites

1. **Fork the Repository**
   ```bash
   # Fork via GitHub UI, then clone
   git clone https://github.com/yourusername/talaria.git
   cd talaria
   ```

2. **Set Up Development Environment**
   ```bash
   # Install Rust toolchain
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Install development tools
   rustup component add rustfmt clippy
   cargo install cargo-watch cargo-edit cargo-outdated
   ```

3. **Create Development Branch**
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/issue-description
   ```

## Development Workflow

### 1. Find an Issue

- Check [open issues](https://github.com/yourusername/talaria/issues)
- Look for `good first issue` or `help wanted` labels
- Comment on the issue to claim it
- Create a new issue if needed

### 2. Write Code

#### Code Style

```rust
// ✓ Good: Clear, documented functions
/// Calculates the alignment score between two sequences
/// 
/// # Arguments
/// * `seq1` - First sequence
/// * `seq2` - Second sequence
/// 
/// # Returns
/// Alignment score as f64
pub fn calculate_alignment_score(seq1: &[u8], seq2: &[u8]) -> f64 {
    // Implementation
}

// ✗ Bad: Unclear, undocumented
pub fn calc_score(s1: &[u8], s2: &[u8]) -> f64 {
    // Implementation
}
```

#### Naming Conventions

```rust
// Modules: snake_case
mod sequence_parser;

// Types: PascalCase
struct SequenceAlignment;
enum ReductionStrategy { }

// Functions/Variables: snake_case
fn parse_fasta_file() { }
let sequence_count = 42;

// Constants: SCREAMING_SNAKE_CASE
const MAX_SEQUENCE_LENGTH: usize = 1_000_000;

// Lifetimes: short, lowercase
fn process<'a>(data: &'a str) { }
```

### 3. Write Tests

#### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_parsing() {
        let input = ">seq1\nACGT\n";
        let result = parse_fasta(input);
        assert_eq!(result.unwrap().len(), 1);
        assert_eq!(result.unwrap()[0].sequence, b"ACGT");
    }

    #[test]
    #[should_panic(expected = "invalid sequence")]
    fn test_invalid_sequence() {
        let input = ">seq1\n123\n";
        parse_fasta(input).unwrap();
    }
}
```

#### Integration Tests

```rust
// tests/integration_test.rs
use talaria::reduce;

#[test]
fn test_full_reduction_pipeline() {
    let input = include_str!("fixtures/test.fasta");
    let config = ReductionConfig::default();
    let result = reduce(input, config);
    
    assert!(result.is_ok());
    assert!(result.unwrap().compression_ratio > 0.5);
}
```

### 4. Document Your Code

#### Documentation Comments

```rust
//! Module-level documentation
//! 
//! This module provides FASTA parsing functionality.

/// Function documentation
/// 
/// # Examples
/// 
/// ```
/// use talaria::parse_fasta;
/// 
/// let data = ">seq1\nACGT\n";
/// let sequences = parse_fasta(data).unwrap();
/// assert_eq!(sequences.len(), 1);
/// ```
/// 
/// # Errors
/// 
/// Returns `ParseError` if the input is malformed
pub fn parse_fasta(input: &str) -> Result<Vec<Sequence>, ParseError> {
    // Implementation
}
```

### 5. Format and Lint

```bash
# Format code
cargo fmt

# Check linting
cargo clippy -- -D warnings

# Fix clippy suggestions
cargo clippy --fix

# Check for security issues
cargo audit

# Update outdated dependencies
cargo outdated
```

## Commit Guidelines

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Test additions or fixes
- `build`: Build system changes
- `ci`: CI/CD changes
- `chore`: Maintenance tasks

### Examples

```bash
# Good commit messages
git commit -m "feat(reducer): add taxonomy-aware reduction strategy"
git commit -m "fix(parser): handle empty sequences in FASTA files"
git commit -m "docs(api): update alignment function documentation"
git commit -m "perf(alignment): optimize matrix allocation with pooling"

# Bad commit messages
git commit -m "fixed stuff"
git commit -m "WIP"
git commit -m "update"
```

### Commit Best Practices

1. **Atomic Commits**: One logical change per commit
2. **Present Tense**: Use "add" not "added"
3. **Imperative Mood**: "fix" not "fixes" or "fixed"
4. **Reference Issues**: Include issue numbers

```bash
git commit -m "fix(alignment): resolve memory leak in matrix pool

Fixes #123

The alignment matrix pool was not properly releasing memory
when matrices were returned. This adds proper cleanup logic."
```

## Pull Request Process

### 1. Before Submitting

- ▶ Ensure all tests pass: `cargo test`
- ▶ Format code: `cargo fmt`
- ▶ Fix linting issues: `cargo clippy --fix`
- ▶ Update documentation if needed
- ▶ Add tests for new functionality
- ▶ Update CHANGELOG.md

### 2. PR Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing completed

## Checklist
- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] Tests added/updated
- [ ] No new warnings

## Related Issues
Fixes #123
Relates to #456
```

### 3. Review Process

1. **Automated Checks**: CI runs tests, linting, formatting
2. **Code Review**: Maintainer reviews code
3. **Feedback**: Address review comments
4. **Approval**: Get approval from maintainer
5. **Merge**: Squash and merge to main

## Testing Guidelines

### Test Coverage

```bash
# Generate coverage report
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --output-dir coverage

# Aim for >80% coverage
```

### Test Categories

1. **Unit Tests**: Test individual functions
2. **Integration Tests**: Test module interactions
3. **Property Tests**: Test invariants
4. **Benchmark Tests**: Test performance
5. **Fuzz Tests**: Test edge cases

### Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_alignment_properties(
        seq1 in "[ACGT]{1,100}",
        seq2 in "[ACGT]{1,100}"
    ) {
        let score1 = align(&seq1, &seq2);
        let score2 = align(&seq2, &seq1);
        
        // Alignment should be symmetric
        prop_assert_eq!(score1, score2);
        
        // Score should be non-negative
        prop_assert!(score1 >= 0.0);
    }
}
```

## Documentation

### API Documentation

```rust
/// Main reduction function
/// 
/// # Arguments
/// 
/// * `input` - Input FASTA sequences
/// * `config` - Reduction configuration
/// 
/// # Returns
/// 
/// * `Ok(ReducedSequences)` - Reduced sequences with metadata
/// * `Err(ReductionError)` - Error during reduction
/// 
/// # Example
/// 
/// ```
/// # use talaria::{reduce, ReductionConfig};
/// let sequences = ">seq1\nACGT\n>seq2\nGCTA\n";
/// let config = ReductionConfig::default();
/// let result = reduce(sequences, config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn reduce(input: &str, config: ReductionConfig) -> Result<ReducedSequences> {
    // Implementation
}
```

### User Documentation

- Update user guide for new features
- Add examples to cookbook
- Update configuration documentation
- Add troubleshooting entries

## Performance Guidelines

### Benchmarking

```rust
// benches/alignment_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn alignment_benchmark(c: &mut Criterion) {
    let seq1 = b"ACGTACGTACGT";
    let seq2 = b"ACGTACGTTCGT";
    
    c.bench_function("needleman_wunsch", |b| {
        b.iter(|| {
            align(black_box(seq1), black_box(seq2))
        });
    });
}

criterion_group!(benches, alignment_benchmark);
criterion_main!(benches);
```

### Performance PRs

1. Include benchmark results
2. Show before/after comparison
3. Explain optimization technique
4. Consider memory vs speed tradeoffs

## Security Guidelines

### Security Checklist

- ▶ No hardcoded credentials
- ▶ Input validation for all user data
- ▶ Safe handling of file paths
- ▶ No unsafe code without justification
- ▶ Dependencies audited with `cargo audit`

### Reporting Security Issues

**DO NOT** create public issues for security vulnerabilities.

Email: security@talaria-project.org

Include:
- Description of vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

## Release Process

### Version Numbering

We use [Semantic Versioning](https://semver.org/):
- MAJOR: Breaking changes
- MINOR: New features (backward compatible)
- PATCH: Bug fixes

### Release Checklist

1. ▶ Update version in Cargo.toml
2. ▶ Update CHANGELOG.md
3. ▶ Run full test suite
4. ▶ Update documentation
5. ▶ Create git tag
6. ▶ Build release binaries
7. ▶ Publish to crates.io
8. ▶ Create GitHub release

## Community

### Getting Help

- **Discord**: [Join our server](https://discord.gg/talaria)
- **Discussions**: [GitHub Discussions](https://github.com/talaria/discussions)
- **Stack Overflow**: Tag with `talaria-bio`

### Contributing Ideas

1. Open a discussion first
2. Get feedback from community
3. Create detailed proposal
4. Implement after approval

## Recognition

### Contributors

All contributors are recognized in:
- AUTHORS.md file
- GitHub contributors page
- Release notes

### Types of Contributions

- 💻 Code contributions
- 📖 Documentation improvements
- 🐛 Bug reports
- 💡 Feature suggestions
- 🔍 Code reviews
- 📢 Community support

## Development Tips

### Useful Commands

```bash
# Watch for changes and rebuild
cargo watch -x build

# Run tests on file change
cargo watch -x test

# Check specific feature
cargo check --features gpu

# Update dependencies
cargo update

# Clean build artifacts
cargo clean

# Generate dependency graph
cargo tree

# Check for unused dependencies
cargo machete
```

### IDE Setup

#### VS Code

```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": ["all"],
    "rust-analyzer.checkOnSave.command": "clippy",
    "editor.formatOnSave": true
}
```

#### IntelliJ IDEA

- Install Rust plugin
- Enable format on save
- Configure clippy as external linter

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT/Apache-2.0 dual license).

## Thank You!

Thank you for contributing to Talaria! Your efforts help make biological sequence analysis more efficient and accessible to researchers worldwide.

## See Also

- [Architecture](architecture.md) - System design
- [Building](building.md) - Build instructions
- [Code of Conduct](CODE_OF_CONDUCT.md) - Community guidelines
- [License](../LICENSE.md) - Project license