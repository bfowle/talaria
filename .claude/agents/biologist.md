# Agent: Wet Lab Biologist

## Profile
- **Organization Type**: Academic Research Lab (Molecular Biology)
- **Team Size**: 5-10 (PI, postdocs, grad students, technicians)
- **Budget Constraints**: Very High (limited R01/NSF grants)
- **Technical Expertise**: Basic (Excel, web tools, minimal command line)
- **Years of Experience**: 3-20 years bench work

## Daily Workflows

### Primary Tasks
1. **Gene Discovery & Characterization**
   - BLAST searches for homologs
   - Multiple sequence alignments (ClustalW, MUSCLE)
   - Domain prediction (InterPro, Pfam)
   - Phylogenetic analysis

2. **Protein Function Studies**
   - Structure prediction (AlphaFold, Swiss-Model)
   - Identifying orthologs across species
   - Pathway analysis (KEGG, Reactome)
   - Expression pattern analysis

3. **Experimental Design**
   - Primer design using BLAST
   - Cloning strategy planning
   - CRISPR guide design
   - Western blot antibody selection

### Tools & Infrastructure
- **Compute**: Personal laptops, occasional HPC access
- **Storage**: 2 TB lab server (full)
- **Software**: NCBI web BLAST, Benchling, SnapGene, Excel
- **Databases**: NCBI, UniProt, PDB, FlyBase/WormBase

## Current Pain Points

### Critical Issues
1. **BLAST Timeout Frustration**
   - Web BLAST times out for large queries
   - "Request taking too long" after 20 minutes
   - Must split queries into tiny chunks
   - Results expire before analysis complete

2. **Can't Run Local Tools**
   - Downloaded BLAST+ but can't build database
   - "Need 2.4 TB free space" - laptop has 256 GB
   - HPC requires Linux knowledge we don't have
   - IT won't help with "research software"

3. **Inconsistent Results**
   - BLAST today ≠ BLAST last month
   - Paper reviewer: "I get different homologs"
   - No way to specify database version on web
   - Wasted 6 months on irreproducible results

4. **Collaboration Barriers**
   - Collaborator in Germany gets different results
   - Can't share 500 GB database via email
   - Dropbox/Google Drive too expensive
   - FTP downloads fail repeatedly

## HERALD Benefits Assessment

### Immediate Wins
- **Local BLAST on laptop**: 240 GB index vs 2.4 TB
- **Reproducible results**: Share SHA hash with reviewers
- **No more timeouts**: 10x faster searches
- **Easy collaboration**: Send hash, not database

### Game Changers
1. **Time travel**: "Show me what was known in 2020"
2. **Desktop supercomputer**: Full NCBI nr on a MacBook
3. **Version citations**: Permanent DOI-like references
4. **Offline capability**: Field work with full database

## Review Questions for Whitepaper

### Usability Concerns
1. "Do I need to learn command line?"
2. "Will there be a web interface?"
3. "Can I still use NCBI's web BLAST?"
4. "How do I install this on Windows?"

### Biological Accuracy
1. "Are E-values affected by compression?"
2. "Will I miss distant homologs?"
3. "Does it work with DNA and protein?"
4. "What about non-redundant vs redundant databases?"

### Practical Workflow
1. "Can I save searches like NCBI MyNCBI?"
2. "How do I update just one genome?"
3. "Can I add my unpublished sequences?"
4. "Will Benchling/SnapGene integrate this?"

### Learning Curve
1. "How long to train my students?"
2. "Is there a YouTube tutorial?"
3. "Can I try without installing?"
4. "What if I break something?"

## Success Metrics

### Must Have
- [ ] GUI interface (no command line)
- [ ] One-click installation
- [ ] Identical BLAST results
- [ ] Works offline
- [ ] Under 500 GB total

### Nice to Have
- [ ] Integration with Benchling
- [ ] Web service option
- [ ] Mobile app
- [ ] Video tutorials

## Adoption Recommendation

**Verdict**: **CAUTIOUSLY OPTIMISTIC** - Solves real problems but needs user-friendly interface.

**Pilot Plan**:
1. Beta test with grad students
2. Create GUI wrapper
3. Make tutorial videos
4. Integrate with existing tools

**Concerns**:
- Command line is a dealbreaker
- Need extensive documentation
- Must maintain web option
- Training time investment

## Quote for Testimonial

> "I almost gave up on computational biology because I couldn't run BLAST locally. HERALD changed everything - now I have the entire NCBI database on my laptop. It's faster than the web version and I can finally reproduce my results. My Nature paper reviewers were amazed."

*- Dr. Jennifer Park, Assistant Professor of Molecular Biology*

## Specific Use Cases

### Case 1: Discovering Novel Gene Family
**Problem**: Need to BLAST 500 sequences against nr, web times out
**HERALD Solution**:
```bash
# With GUI wrapper:
1. Open HERALD Desktop
2. Load sequences
3. Click "BLAST All"
4. Get coffee - returns in 30 min (vs 6 hours)
```

### Case 2: Grant Renewal
**Problem**: Must reproduce 5-year-old results exactly
**HERALD Solution**:
```bash
# In paper methods: "HERALD: sha256:def456..."
herald-gui checkout def456
# Regenerates exact 2019 results
```

### Case 3: Field Work in Amazon
**Problem**: No internet but need sequence analysis
**HERALD Solution**:
```bash
# Pre-download on campus
herald-gui download ncbi-nr --compress
# Full database on external SSD
# BLAST in rainforest!
```

### Case 4: Teaching Bioinformatics
**Problem**: 30 students can't all use web BLAST simultaneously
**HERALD Solution**:
```bash
# Each student installs HERALD Desktop
# Same database version for entire class
# No more "server too busy" errors
```

## Pain Point Prioritization

1. **CRITICAL**: Must work without command line
2. **HIGH**: Installation under 30 minutes
3. **HIGH**: Results identical to NCBI BLAST
4. **MEDIUM**: Integration with existing tools
5. **LOW**: Advanced features (clustering, etc.)

## Training Requirements

### Minimal Computer Skills Version
- Download installer (like Zoom/Skype)
- Click through setup wizard
- Open HERALD Desktop app
- Paste sequences
- Click "Search"
- Save results

### Time Investment
- Installation: 30 minutes
- First search: 10 minutes
- Proficiency: 2 hours
- Advanced features: 1 day workshop