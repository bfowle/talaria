# Custom Databases

Talaria supports adding and managing custom databases from local FASTA files, allowing teams to maintain their own private sequence collections alongside public databases.

## Adding a Custom Database

Use the `database add` command to add a FASTA file as a custom database:

```bash
# Basic usage - adds to custom/filename
talaria database add -i /path/to/sequences.fasta

# Specify a custom name
talaria database add -i sequences.fasta --name "team-proteins"

# Use custom source and dataset names
talaria database add -i sequences.fasta \
  --source "myteam" \
  --dataset "proteins-v2" \
  --description "Team protein database v2"

# Keep the original file (copy instead of move)
talaria database add -i valuable.fasta --copy

# Replace existing database
talaria database add -i updated.fasta --name "team-proteins" --replace
```

## Directory Structure

Custom databases are stored in the same versioned structure as public databases:

```
~/.talaria/databases/data/
├── custom/                    # Default source for custom databases
│   ├── team-proteins/
│   │   ├── 2024-01-15/       # Version (date added)
│   │   │   ├── team-proteins.fasta
│   │   │   └── metadata.json
│   │   └── current -> 2024-01-15
│   └── project-db/
│       └── ...
├── myteam/                    # Custom source name
│   └── proteins-v2/
│       └── ...
├── uniprot/                   # Public databases
└── ncbi/
```

## Using Custom Databases

Once added, custom databases work exactly like public databases:

### Reduce
```bash
# Reduce a custom database
talaria reduce custom/team-proteins -r 0.3
talaria reduce myteam/proteins-v2 -r 0.25
```

### Validate
```bash
# Validate a reduction
talaria validate custom/team-proteins:30-percent
```

### Reconstruct
```bash
# Reconstruct sequences
talaria reconstruct custom/team-proteins:30-percent
```

### List and Info
```bash
# List all databases (custom databases shown with [custom] indicator)
talaria database list

# Get information about a custom database
talaria database info custom/team-proteins
talaria database info myteam/proteins-v2
```

## Metadata

Each custom database includes metadata tracking:
- Original filename
- Date added
- Sequence count
- File size
- Description
- Version identifier

View metadata with:
```bash
talaria database info custom/team-proteins
```

## Team Collaboration

Custom databases enable team collaboration scenarios:

1. **Shared Custom Databases**: Teams can maintain private sequence collections
2. **Project-Specific Databases**: Create databases for specific projects
3. **Version Control**: Track changes over time with dated versions
4. **Local Development**: Test with small custom databases before scaling

## Best Practices

1. **Naming Conventions**: Use descriptive names that indicate the content
   - Good: `human-kinases`, `covid-variants-2024`
   - Avoid: `test`, `data`, `sequences`

2. **Source Organization**: Group related databases under custom sources
   - `myteam/proteins-v1`, `myteam/proteins-v2`
   - `project-x/candidates`, `project-x/controls`

3. **Version Management**: Add new versions instead of replacing
   ```bash
   # Add updated version (creates new dated directory)
   talaria database add -i updated.fasta --name "team-proteins"
   ```

4. **Documentation**: Use descriptions to document database contents
   ```bash
   talaria database add -i sequences.fasta \
     --description "Human kinase domains extracted from UniProt 2024-01"
   ```

## Limitations

- Custom databases must be in FASTA format
- The `database update` command skips custom databases (no remote source)
- Custom databases are local to the machine (not automatically synced)

## Future Enhancements

Planned features for custom databases:
- Support for other formats (GenBank, EMBL) with automatic conversion
- Team sharing via git repositories or network shares
- Semantic versioning instead of date-based versions
- Import from URLs or cloud storage