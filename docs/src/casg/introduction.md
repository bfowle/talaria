# What is CASG?

The Content-Addressed Sequence Graph (CASG) is Talaria's revolutionary storage system that makes biological databases smarter, faster, and more efficient.

## The Problem

Every day, biological databases like UniProt and NCBI nr receive thousands of updates. With traditional systems, even a tiny change means re-downloading the entire database—often hundreds of gigabytes. That's like re-downloading an entire movie collection because one scene changed in one movie.

## The CASG Solution

CASG treats biological databases differently. Instead of seeing them as giant monolithic files, CASG:

- **Breaks databases into smart chunks** based on biological relationships
- **Identifies each chunk uniquely** using cryptographic hashes (like fingerprints)
- **Only downloads what changed** during updates
- **Verifies everything** to ensure data integrity

## Real-World Impact

With CASG, a typical UniProt update that would normally require downloading 85GB might only need 100MB—that's 99.9% less bandwidth. For research teams, this means:

- ✓ **Faster updates** - Minutes instead of hours
- ✓ **Less storage** - Keep multiple versions without multiplying space
- ✓ **Perfect reproducibility** - Know exactly which version was used
- ✓ **Lower costs** - Reduced bandwidth and storage expenses

## How It Works (Simple Version)

Think of CASG like a smart filing system:

1. **Content Addressing**: Each piece of data gets a unique ID based on its content (not its name)
2. **Deduplication**: Identical sequences are stored only once, no matter how many times they appear
3. **Smart Updates**: Only new or changed data needs to be downloaded
4. **Verification**: Every piece can be verified as authentic using its ID

## Who Benefits?

- **Researchers** get faster access to updated databases
- **Bioinformaticians** spend less time managing data
- **IT Teams** see reduced storage and bandwidth costs
- **Science** benefits from better reproducibility

## Next Steps

- New to CASG? Start with [Getting Started](./getting-started.md)
- Want to understand the concepts? Read [Core Concepts](./concepts.md)
- Ready to see it in action? Check out [Common Workflows](./workflows.md)
- Curious about the theory? See our [Academic Whitepaper](../whitepapers/casg-architecture.md)