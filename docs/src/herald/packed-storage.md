# Packed Storage Backend (Historical)

> **⚠️ DEPRECATED**: This document describes the original packed file storage architecture, which has been replaced by a high-performance LSM-tree storage engine with probabilistic filter optimization.
>
> **For current architecture**, see:
> - [RocksDB Storage Documentation](./rocksdb-storage.md) - Current implementation details
> - [Unified Architecture](./unified-architecture.md) - Overview of the LSM-tree + bloom filter system
> - [Architecture Whitepaper](../whitepapers/herald-architecture.md) - Comprehensive technical analysis

## Historical Context

The packed storage backend was HERALD's initial implementation, designed to prove the canonical sequence concept. While functional, it had significant performance limitations that became apparent at scale:

**Challenges with Packed Files:**
- **Unbounded memory growth**: In-memory indices grew to 18GB+ for large databases
- **Slow imports**: 50K sequences took 1-2 hours; UniRef50 would take 50-100 days
- **Individual file operations**: Each sequence required separate file I/O
- **No efficient deduplication checking**: Linear scan through indices

**Performance Comparison:**

| Operation | Packed Files | LSM-Tree | Improvement |
|-----------|-------------|----------|-------------|
| 50K sequences | 1-2 hours | 30-60 sec | 100x |
| UniRef50 (48M) | 50-100 days | 10-20 hours | 100x |
| Memory | Unbounded (18GB+) | Bounded (6-8GB) | Controlled |

## Legacy

The packed storage architecture successfully validated the core concepts of canonical sequence storage and content addressing, which remain fundamental to HERALD. The lessons learned from this implementation directly informed the design of the current LSM-tree architecture:

1. **Content addressing works** - Identifying sequences by hash enables true deduplication
2. **Separation of identity and representation** - Storing sequence content separately from metadata is key
3. **Scalability requires different architecture** - File-based storage doesn't scale to billions of sequences
4. **Memory must be bounded** - Production systems need predictable memory consumption

The current LSM-tree architecture with three-tier probabilistic filters preserves these validated concepts while providing the performance needed for production use at scale.

---

*For all current development and usage, refer to the RocksDB storage documentation linked above. This file is retained only for historical reference.*
