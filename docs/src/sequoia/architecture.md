# SEQUOIA Architecture

> **Note**: The comprehensive SEQUOIA architecture whitepaper has been moved to provide better organization of our documentation.

## Quick Links

- **[Read the Full Academic Whitepaper](../whitepapers/sequoia-architecture.md)** - Complete technical deep-dive with citations and formal analysis

## For Different Audiences

### New to SEQUOIA?
Start with these beginner-friendly guides:
- [What is SEQUOIA?](./introduction.md) - Simple introduction
- [Core Concepts](./concepts.md) - Key ideas explained clearly
- [Getting Started](./getting-started.md) - Hands-on tutorial

### Want Technical Details?
- [How SEQUOIA Works](./how-it-works.md) - Visual step-by-step explanation
- [Storage Overview](./overview.md) - Technical implementation details
- [API Reference](./api-reference.md) - Programming interface

### Research & Theory
- [Academic Whitepaper](../whitepapers/sequoia-architecture.md) - Formal treatment with mathematical proofs
- [Performance Metrics](./performance.md) - Empirical measurements and benchmarks
- [Case Studies](./case-studies.md) - Real-world deployments

## Architecture Summary

The SEQUOIA (Sequence Query Optimization with Indexed Architecture) architecture fundamentally reimagines biological database storage through:

### Core Principles
1. **Canonical Sequence Storage** - Each unique sequence stored exactly once, identified by content hash
2. **Packed Storage Backend** - Sequences stored in 64MB pack files to avoid filesystem overhead
3. **Sequence-Level Deduplication** - True cross-database deduplication at the sequence level
4. **Multi-Representation Support** - Same sequence can have different headers from different databases
5. **Chunk Manifests** - Lightweight references to sequences, not containers of sequences
6. **Content Addressing** - Data identified by cryptographic hash of sequence content only
7. **Merkle DAG Structure** - Hierarchical organization with cryptographic proofs
8. **Bi-Temporal Versioning** - Independent tracking of sequence and taxonomy changes
9. **Taxonomic Chunking** - Biology-aware organization via manifests
10. **Canonical Delta Compression** - Delta computed once per sequence pair, reused everywhere

### Key Benefits
- **True Cross-Database Deduplication** - Same sequence in UniProt and NCBI stored once
- **50,000+ sequences/second** import performance with packed storage
- **50-200× bandwidth reduction** for database updates
- **80-95% storage savings** when managing multiple related databases
- **10× faster imports** for databases with overlapping content
- **Minimal filesystem overhead** - Millions of sequences in hundreds of pack files
- **Cryptographic verification** of all data
- **Perfect reproducibility** for research
- **Database-agnostic delta compression** - Deltas computed once, used everywhere

For the complete technical analysis including mathematical proofs, performance evaluations, and detailed architectural decisions, please refer to the **[full academic whitepaper](../whitepapers/sequoia-architecture.md)**.