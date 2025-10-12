# HERALD Architecture

> **Note**: The comprehensive HERALD architecture whitepaper has been moved to provide better organization of our documentation.

## Quick Links

- **[Read the Full Academic Whitepaper](../whitepapers/herald-architecture.md)** - Complete technical deep-dive with citations and formal analysis

## For Different Audiences

### New to HERALD?
Start with these beginner-friendly guides:
- [What is HERALD?](./introduction.md) - Simple introduction
- [Core Concepts](./concepts.md) - Key ideas explained clearly
- [Getting Started](./getting-started.md) - Hands-on tutorial

### Want Technical Details?
- [How HERALD Works](./how-it-works.md) - Visual step-by-step explanation
- [Storage Overview](./overview.md) - Technical implementation details
- [API Reference](./api-reference.md) - Programming interface

### Research & Theory
- [Academic Whitepaper](../whitepapers/herald-architecture.md) - Formal treatment with mathematical proofs
- [Performance Metrics](./performance.md) - Empirical measurements and benchmarks
- [Case Studies](./case-studies.md) - Real-world deployments

## Architecture Summary

The HERALD (Sequence Query Optimization with Indexed Architecture) architecture fundamentally reimagines biological database storage through:

### Core Principles
1. **Canonical Sequence Storage** - Each unique sequence stored exactly once, identified by content hash
2. **LSM-Tree Storage Engine** - High-performance write-optimized database for 100x faster operations
3. **Three-Tier Probabilistic Filters** - Cascading bloom/ribbon filters eliminate 99% of storage lookups
4. **Sequence-Level Deduplication** - True cross-database deduplication at the sequence level
5. **Multi-Representation Support** - Same sequence can have different headers from different databases
6. **Chunk Manifests** - Lightweight references to sequences, not containers of sequences
7. **Content Addressing** - Data identified by cryptographic hash of sequence content only
8. **Merkle DAG Structure** - Hierarchical organization with cryptographic proofs
9. **Bi-Temporal Versioning** - Independent tracking of sequence and taxonomy changes
10. **Taxonomic Chunking** - Biology-aware organization via manifests
11. **Canonical Delta Compression** - Delta computed once per sequence pair, reused everywhere

### Key Benefits
- **True Cross-Database Deduplication** - Same sequence in UniProt and NCBI stored once
- **50,000+ sequences/second** import performance with LSM-tree backend
- **100x faster deduplication** - Probabilistic filters reduce lookup time from 100μs to 1μs
- **50-200× bandwidth reduction** for database updates via incremental chunk downloads
- **80-95% storage savings** when managing multiple related databases
- **10× faster imports** for databases with overlapping content
- **100× performance improvement** - Handles billions of sequences efficiently
- **Bounded memory usage** - Configurable cache size (6-8GB typical) vs unbounded growth
- **Cryptographic verification** of all data
- **Perfect reproducibility** for research
- **Database-agnostic delta compression** - Deltas computed once, used everywhere
- **Remote chunk storage** - Download only changed chunks from S3/GCS/Azure

For the complete technical analysis including mathematical proofs, performance evaluations, and detailed architectural decisions, please refer to the **[full academic whitepaper](../whitepapers/herald-architecture.md)**.