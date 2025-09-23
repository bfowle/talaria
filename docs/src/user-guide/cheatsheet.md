# Talaria Cheat Sheet

## Essential Commands (Copy & Paste Ready)

### First Time Setup
```bash
talaria sequoia init
talaria database download uniprot -d swissprot
```

### Daily Use
```bash
# Update database (only downloads changes)
talaria database download uniprot -d swissprot

# Reduce database
talaria reduce uniprot/swissprot -r 0.3 -o output.fasta

# View what you have
talaria database list
talaria sequoia stats
```

## Database Download Commands

```bash
# UniProt databases
talaria database download uniprot -d swissprot    # ~200MB
talaria database download uniprot -d trembl       # ~150GB
talaria database download uniprot -d uniref90     # ~20GB
talaria database download uniprot -d uniref50     # ~8GB

# NCBI databases
talaria database download ncbi -d nr              # ~100GB
talaria database download ncbi -d nt              # ~70GB
talaria database download ncbi -d taxonomy        # ~50MB

# Custom database
talaria database add -i myfile.fasta --source mylab --dataset proteins
```

## Reduction Commands

```bash
# Basic reduction (30% of original)
talaria reduce uniprot/swissprot -r 0.3 -o reduced.fasta

# Optimize for specific aligner
talaria reduce uniprot/swissprot -r 0.3 -a lambda -o lambda_db.fasta
talaria reduce uniprot/swissprot -r 0.25 -a diamond -o diamond_db.fasta
talaria reduce uniprot/swissprot -r 0.3 -a blast -o blast_db.fasta

# From file (not database)
talaria reduce -i input.fasta -o output.fasta -r 0.3
```

## Information Commands

```bash
# List databases
talaria database list

# Database info
talaria database info uniprot/swissprot

# List sequences
talaria database list-sequences uniprot/swissprot --limit 100
talaria database list-sequences uniprot/swissprot --ids-only

# SEQUOIA statistics
talaria sequoia stats
```

## Validation & Reconstruction

```bash
# Validate reduction quality
talaria validate uniprot/swissprot:30-percent

# Reconstruct original sequences
talaria reconstruct uniprot/swissprot:30-percent -o reconstructed.fasta
```

## Environment Variables

```bash
# Change database storage location (before init)
export TALARIA_DATABASES_DIR=/fast/ssd/talaria

# Use specific thread count
talaria -j 16 reduce uniprot/swissprot -r 0.3 -o output.fasta
```

## Common Workflows

### LAMBDA Workflow
```bash
talaria reduce uniprot/swissprot -r 0.3 -a lambda -o db.fasta
lambda3 mkindexp -d db.fasta
lambda3 searchp -q queries.fasta -d db.fasta -o results.m8
```

### BLAST Workflow
```bash
talaria reduce ncbi/nr -r 0.3 -a blast -o nr_reduced.fasta
makeblastdb -in nr_reduced.fasta -dbtype prot -out nr_blast
blastp -query queries.fasta -db nr_blast -out results.txt
```

### Diamond Workflow
```bash
talaria reduce uniprot/swissprot -r 0.25 -a diamond -o swiss_diamond.fasta
diamond makedb --in swiss_diamond.fasta --db swiss_diamond
diamond blastp -q queries.fasta -d swiss_diamond -o results.m8
```

## Quick Tips

- **Start with SwissProt** (~200MB) for testing, not nr (~100GB)
- **30% reduction** (`-r 0.3`) is a good starting point
- **Same download command** checks for updates automatically
- **Use `-a <aligner>`** to optimize for your specific tool
- **SEQUOIA only downloads changes** after initial download

## Getting Help

```bash
talaria --help
talaria reduce --help
talaria database --help
talaria database download --help
```