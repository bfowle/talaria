# talaria-tools

External tool integration layer for bioinformatics aligners and search tools.

## Overview

This crate provides a unified interface for integrating external bioinformatics tools:

- **Tool Management**: Automatic download, installation, and version management
- **Aligner Abstraction**: Common interface for multiple alignment tools
- **Configuration**: Tool-specific configuration management
- **Performance Optimization**: Tool-specific optimizations

## Supported Tools

### LAMBDA
```rust
use talaria_tools::{LambdaAligner, AlignmentConfig};

let mut aligner = LambdaAligner::new()?;

// Check availability
if !aligner.is_available() {
    aligner.install().await?;
}

// Configure
let config = AlignmentConfig {
    num_threads: 8,
    evalue: 1e-10,
    max_hits: 100,
    ..Default::default()
};
aligner.configure(config)?;

// Search
let results = aligner.search(&queries, &references)?;
```

### Tool Management
```rust
use talaria_tools::{ToolManager, Tool};

let manager = ToolManager::new()?;

// Install specific version
manager.install(Tool::Lambda, Some("v3.0.0")).await?;

// List installed tools
let tools = manager.list_installed()?;
for info in tools {
    println!("{}: {} at {}", info.name, info.version, info.path.display());
}

// Auto-update
manager.update_all().await?;
```

## Aligner Trait

All aligners implement the common `Aligner` trait:

```rust
use talaria_tools::{Aligner, AlignmentResult};

fn run_alignment<A: Aligner>(
    aligner: &mut A,
    queries: &[Sequence],
    references: &[Sequence],
) -> Result<Vec<AlignmentResult>> {
    // Works with any aligner
    aligner.search(queries, references)
}
```

## Available Tools

| Tool | Type | Best For |
|------|------|----------|
| LAMBDA | Protein | Fast protein searches |
| BLAST+ | Universal | Gold standard, slower |
| DIAMOND | Protein | Ultra-fast protein alignment |
| MMseqs2 | Universal | Sensitive profile searches |
| Kraken2 | Taxonomic | Metagenomic classification |

## Configuration

### Global Configuration
```rust
use talaria_tools::ToolManager;

let manager = ToolManager::with_config(config)?;
manager.set_default_tool(Tool::Diamond)?;
manager.set_install_dir("/opt/talaria/tools")?;
```

### Per-Tool Configuration
```rust
use talaria_tools::{ConfigurableAligner, AlignmentConfig};

let mut aligner = get_aligner(Tool::Diamond)?;
aligner.configure(AlignmentConfig {
    sensitivity: Sensitivity::Sensitive,
    output_format: OutputFormat::TSV,
    ..Default::default()
})?;
```

## Mock Aligner

For testing and development:

```rust
use talaria_tools::MockAligner;

let aligner = MockAligner::new();
// Always returns empty results, useful for testing pipelines
```

## Performance Features

- **Parallel execution** support for all tools
- **Memory management** for large databases
- **Temporary file** optimization
- **Index caching** for repeated searches
- **Batch processing** for multiple queries

## Installation

Tools can be installed automatically:

```rust
use talaria_tools::{ToolManager, Tool};

#[tokio::main]
async fn main() -> Result<()> {
    let manager = ToolManager::new()?;

    // Install all required tools
    for tool in [Tool::Lambda, Tool::Diamond, Tool::Blast] {
        if !manager.is_installed(tool)? {
            println!("Installing {}...", tool);
            manager.install(tool, None).await?;
        }
    }

    Ok(())
}
```

## Usage

Add to your `Cargo.toml`:
```toml
[dependencies]
talaria-tools = { path = "../talaria-tools" }
```

## License

MIT