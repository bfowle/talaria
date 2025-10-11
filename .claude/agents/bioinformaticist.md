# Agent: Bioinformaticist

## Profile
- **Organization Type**: Academic Research Core Facility
- **Team Size**: 3-5 computational specialists
- **Budget Constraints**: High (limited grant funding, shared resources)
- **Technical Expertise**: Expert (Python/R/Bash, HPC, pipeline development)
- **Years of Experience**: 5-15 years in computational biology

## Daily Workflows

### Primary Tasks
1. **Sequence Alignment Pipeline Management**
   - Running BLAST/Diamond/MMseqs2 searches for 20+ research groups
   - Building and updating aligner indices weekly (NCBI nr, UniProt, RefSeq)
   - Optimizing search parameters for speed vs. sensitivity

2. **Database Maintenance**
   - Weekly downloads of NCBI nr (500 GB), UniProt (300 GB), RefSeq (200 GB)
   - Version control attempts using date stamps (unreliable)
   - Managing 10+ database versions for reproducibility

3. **Pipeline Development**
   - Snakemake/Nextflow workflows for automated analysis
   - Docker containers (failing due to 2+ TB image sizes)
   - Custom scripts for format conversion and filtering

### Tools & Infrastructure
- **Compute**: 512-core HPC cluster, 2 TB RAM total
- **Storage**: 100 TB NFS, filling up every 6 months
- **Software**: BLAST+, Diamond, MMseqs2, CD-HIT, HMMER
- **Languages**: Python (Biopython), R (Bioconductor), Bash, Perl

## Current Pain Points

### Critical Issues
1. **Index Rebuild Hell**
   - 8 hours to build BLAST index for NCBI nr
   - 6 hours for Diamond index
   - Must rebuild ALL indices after EACH weekly update
   - 200+ CPU-hours/week just for index maintenance

2. **Storage Crisis**
   - 2.4 TB for single NCBI nr BLAST index
   - 26 TB to maintain one year of weekly versions
   - NFS performance degrades with large indices
   - Backup systems can't handle the volume

3. **Reproducibility Nightmare**
   - Paper: "BLAST against NCBI nr (downloaded March 2023)"
   - Reality: Which day? Which version? Pre or post taxonomy update?
   - No way to verify exact database used
   - Reviewers get different results

4. **Performance Bottlenecks**
   - Searches take 2-6 hours for large query sets
   - I/O wait time > compute time (indices don't fit in RAM)
   - Queue times increase as users avoid rebuilding indices

## HERALD Benefits Assessment

### Immediate Wins
- **90% smaller indices** = Finally fit in RAM = 10x faster searches
- **Incremental updates** = Download 500 MB instead of 500 GB weekly
- **SHA verification** = Cryptographic proof for paper methods sections
- **Shared indices** = Build once, use everywhere

### Game Changers
1. **Workstation-scale analysis**: Run BLAST on a laptop!
2. **Version checkout**: `herald checkout ncbi-nr@2023-03-15`
3. **Federated computation**: Distribute searches across institutions
4. **Cost reduction**: $50K HPC → $5K workstation

## Review Questions for Whitepaper

### Technical Accuracy
1. "How exactly does child reconstruction maintain BLAST sensitivity? Show me the math."
2. "What's the overhead of on-demand delta reconstruction during searches?"
3. "How do you handle BLAST's precomputed word tables with dynamic reconstruction?"
4. "Will this work with PSI-BLAST iterative searches?"

### Implementation Concerns
1. "How much RAM is needed for the delta mapping tables?"
2. "Can I integrate this with existing Snakemake pipelines?"
3. "What about BLAST output formats - do they change?"
4. "How do you handle proprietary formats like Diamond's .dmnd?"

### Performance Claims
1. "8-12x speedup seems optimistic - what's the test dataset?"
2. "Does the 90% reduction apply to all database types?"
3. "What about edge cases like low-complexity regions?"
4. "Memory usage during reconstruction spikes?"

### Adoption Barriers
1. "Do I need to retrain my entire team?"
2. "Will NCBI/UniProt officially support this?"
3. "What about backwards compatibility?"
4. "Who maintains the reference selection algorithm?"

## Success Metrics

### Must Have
- [ ] BLAST E-values remain identical
- [ ] Integration with existing pipelines < 1 day effort
- [ ] Search speed improvement > 5x
- [ ] Storage reduction > 75%

### Nice to Have
- [ ] GUI for non-technical users
- [ ] Cloud-native deployment options
- [ ] Automated reference optimization
- [ ] Real-time index updates

## Adoption Recommendation

**Verdict**: **STRONG ADOPT** - This solves our three biggest problems (storage, speed, reproducibility) in one solution.

**Pilot Plan**:
1. Test with UniProt SwissProt (smallest)
2. Benchmark against current pipeline
3. Validate results match exactly
4. Roll out to NCBI nr if successful

**Concerns**:
- Need vendor support from BLAST/Diamond developers
- Reference selection algorithm needs to be transparent
- Documentation must be excellent

## Quote for Testimonial

> "HERALD transformed our core facility from a storage crisis to a service revolution. We went from telling users 'your BLAST job will run next week' to 'results in 30 minutes'. The 90% reduction in index size meant we could finally keep all indices in RAM. Game changer."

*- Dr. Sarah Chen, Director of Bioinformatics Core, State University*