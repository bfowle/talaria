# talaria-core

Core utilities and shared types for the Talaria ecosystem.

## Overview

This crate provides fundamental functionality shared across all Talaria components:

- **Error Types**: Unified error handling with `TalariaError`
- **Path Management**: Centralized path configuration respecting environment variables
- **Configuration**: System-wide configuration management
- **Version Management**: Semantic versioning utilities

## Features

### Path Management
```rust
use talaria_core::{talaria_home, talaria_databases_dir};

// Respects TALARIA_HOME environment variable
let home = talaria_home();
let databases = talaria_databases_dir();
```

### Configuration
```rust
use talaria_core::{Config, load_config, save_config};

// Load from file or environment
let config = load_config("config.toml")?;

// Modify and save
config.reduction.target_ratio = 0.3;
save_config("config.toml", &config)?;
```

### Error Handling
```rust
use talaria_core::{TalariaError, TalariaResult};

fn process_data() -> TalariaResult<String> {
    // Unified error handling across all crates
    Ok("Success".to_string())
}
```

## Environment Variables

- `TALARIA_HOME`: Base directory for all Talaria data (default: `$HOME/.talaria`)
- `TALARIA_DATA_DIR`: Data directory (default: `$TALARIA_HOME`)
- `TALARIA_DATABASES_DIR`: Database storage (default: `$TALARIA_DATA_DIR/databases`)
- `TALARIA_TOOLS_DIR`: External tools (default: `$TALARIA_DATA_DIR/tools`)
- `TALARIA_CACHE_DIR`: Cache directory (default: `$TALARIA_DATA_DIR/cache`)
- `TALARIA_WORKSPACE_DIR`: Temporal workspace (default: `/tmp/talaria`)

## Usage

Add to your `Cargo.toml`:
```toml
[dependencies]
talaria-core = { path = "../talaria-core" }
```

## License

MIT