# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Distributed processing support for cluster environments
- GPU acceleration for sequence alignment (experimental)
- Support for NCBI BLAST+ aligner integration
- Cloud storage backends (S3, GCS, Azure Blob)
- Interactive TUI mode for database exploration
- HTML report generation with D3.js visualizations
- Bi-temporal versioning in HERALD
- Cross-database deduplication
- Canonical delta encoding for better compression

### Changed
- Reorganized module structure for better maintainability
- Improved memory efficiency for large databases
- Enhanced progress reporting with ETA calculations
- Updated dependencies to latest versions

### Fixed
- Memory leak in long-running reduction operations
- Incorrect taxonomy assignment for certain RefSeq entries
- Race condition in parallel chunk processing

## [0.1.0] - 2024-01-15

### Added
- Initial release of Talaria
- Core reduction engine with reference selection
- HERALD content-addressed storage system
- Support for LAMBDA, DIAMOND, MMseqs2, and Kraken aligners
- Taxonomy-aware clustering algorithms
- Delta encoding for non-reference sequences
- Merkle DAG verification for data integrity
- Temporal database queries
- UniProt and RefSeq database integration
- Comprehensive CLI with subcommands
- Progress bars and statistics reporting
- Workspace management with automatic cleanup
- Configuration file support (TOML format)
- Docker container support
- Comprehensive test suite
- Documentation with mdBook

### Features
- **Reduction Pipeline**: Intelligent sequence selection using graph algorithms
- **HERALD Storage**: Content-addressed storage with SHA256 hashing
- **Aligner Support**: Optimized output for multiple alignment tools
- **Database Management**: Download, update, and manage biological databases
- **Performance**: 3-5x faster than traditional approaches
- **Memory Efficiency**: Streaming architecture for large datasets
- **Quality Metrics**: Built-in validation and coverage analysis

### Module Overview
- `talaria-core`: Core types and configuration management
- `talaria-bio`: Bioinformatics algorithms and FASTA handling
- `talaria-storage`: Storage backend abstractions
- `talaria-herald`: Content-addressed storage implementation
- `talaria-tools`: External tool integration
- `talaria-utils`: Display and workspace utilities
- `talaria-cli`: Command-line interface

### Performance Benchmarks
- UniProt SwissProt reduction: 12 minutes (565K sequences)
- 69% index size reduction with 99.8% coverage maintained
- 2.7x faster query performance with reduced indices

## [0.0.1-alpha] - 2023-12-01

### Added
- Proof of concept implementation
- Basic FASTA reduction functionality
- Simple reference selection algorithm
- Command-line interface prototype
- Initial LAMBDA aligner integration

### Known Issues
- Limited to single-threaded execution
- No progress reporting
- Basic error handling
- Memory inefficient for large databases

---

## Versioning Policy

This project follows Semantic Versioning:
- **Major version** (X.0.0): Incompatible API changes
- **Minor version** (0.X.0): Backwards-compatible functionality additions
- **Patch version** (0.0.X): Backwards-compatible bug fixes

## Deprecation Policy

Features marked as deprecated will be maintained for at least two minor versions before removal. Deprecation warnings will be clearly documented in release notes and logged at runtime.

## Support Policy

- **Current version**: Full support with bug fixes and security updates
- **Previous minor version**: Security updates only
- **Older versions**: Community support only

## Release Schedule

- **Minor releases**: Quarterly (Q1, Q2, Q3, Q4)
- **Patch releases**: As needed for critical fixes
- **Major releases**: Annually or as needed

---

[Unreleased]: https://github.com/talaria/talaria/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/talaria/talaria/compare/v0.0.1-alpha...v0.1.0
[0.0.1-alpha]: https://github.com/talaria/talaria/releases/tag/v0.0.1-alpha