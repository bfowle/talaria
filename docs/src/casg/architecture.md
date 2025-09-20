# CASG Architecture

> **Note**: The comprehensive CASG architecture whitepaper has been moved to provide better organization of our documentation.

## Quick Links

- **[Read the Full Academic Whitepaper](../whitepapers/casg-architecture.md)** - Complete technical deep-dive with citations and formal analysis

## For Different Audiences

### New to CASG?
Start with these beginner-friendly guides:
- [What is CASG?](./introduction.md) - Simple introduction
- [Core Concepts](./concepts.md) - Key ideas explained clearly
- [Getting Started](./getting-started.md) - Hands-on tutorial

### Want Technical Details?
- [How CASG Works](./how-it-works.md) - Visual step-by-step explanation
- [Storage Overview](./overview.md) - Technical implementation details
- [API Reference](./api-reference.md) - Programming interface

### Research & Theory
- [Academic Whitepaper](../whitepapers/casg-architecture.md) - Formal treatment with mathematical proofs
- [Performance Metrics](./performance.md) - Empirical measurements and benchmarks
- [Case Studies](./case-studies.md) - Real-world deployments

## Architecture Summary

The CASG (Content-Addressed Sequence Graph) architecture fundamentally reimagines biological database storage through:

### Core Principles
1. **Content Addressing** - Data identified by cryptographic hash, not arbitrary names
2. **Merkle DAG Structure** - Hierarchical organization with cryptographic proofs
3. **Bi-Temporal Versioning** - Independent tracking of sequence and taxonomy changes
4. **Taxonomic Chunking** - Biology-aware data organization
5. **Delta Compression** - Evolutionary relationship-based storage optimization

### Key Benefits
- **50-200× bandwidth reduction** for database updates
- **2-3× storage improvement** through deduplication
- **Cryptographic verification** of all data
- **Perfect reproducibility** for research

For the complete technical analysis including mathematical proofs, performance evaluations, and detailed architectural decisions, please refer to the **[full academic whitepaper](../whitepapers/casg-architecture.md)**.