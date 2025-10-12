# Building from Source

Complete guide for building Talaria from source, including dependencies, build configurations, and troubleshooting.

## Prerequisites

### Required Tools

| Tool | Minimum Version | Purpose |
|------|----------------|---------|
| Rust | 1.75.0 | Compiler and toolchain |
| Cargo | 1.75.0 | Build system and package manager |
| Git | 2.0 | Version control |
| C Compiler | GCC 7+ / Clang 6+ | Native dependencies |

### Optional Tools

| Tool | Purpose |
|------|---------|
| Docker | Container builds |
| Make | Build automation |
| CMake | External dependencies |
| pkg-config | Library discovery |

### System Dependencies

#### Linux (Ubuntu/Debian)

```bash
# Essential build tools
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    cmake \
    git

# Optional dependencies
sudo apt-get install -y \
    libclang-dev \
    liblz4-dev \
    libzstd-dev \
    libbz2-dev
```

#### Linux (Fedora/RHEL)

```bash
# Essential build tools
sudo dnf install -y \
    gcc \
    gcc-c++ \
    make \
    pkgconfig \
    openssl-devel \
    cmake \
    git

# Optional dependencies
sudo dnf install -y \
    clang-devel \
    lz4-devel \
    libzstd-devel \
    bzip2-devel
```

#### macOS

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Using Homebrew
brew install \
    cmake \
    pkg-config \
    openssl \
    lz4 \
    zstd
```

#### Windows

```powershell
# Using Chocolatey
choco install git
choco install cmake
choco install visualstudio2022-workload-vctools

# Or using winget
winget install Git.Git
winget install Kitware.CMake
winget install Microsoft.VisualStudio.2022.BuildTools
```

## Getting the Source

### Clone Repository

```bash
# Clone with HTTPS
git clone https://github.com/yourusername/talaria.git
cd talaria

# Or clone with SSH
git clone git@github.com:yourusername/talaria.git
cd talaria
```

### Workspace Structure

```
talaria/
├── Cargo.toml              # Workspace configuration
├── Cargo.lock              # Dependency lock file
│
├── talaria-core/           # Shared utilities
│   ├── Cargo.toml
│   └── src/
│
├── talaria-bio/            # Bioinformatics library
│   ├── Cargo.toml
│   └── src/
│
├── talaria-storage/        # Storage backends
│   ├── Cargo.toml
│   └── src/
│
├── talaria-herald/           # HERALD system
│   ├── Cargo.toml
│   └── src/
│
├── talaria-tools/          # External tools
│   ├── Cargo.toml
│   └── src/
│
├── talaria-cli/            # CLI application
│   ├── Cargo.toml
│   └── src/
│
├── tests/                  # Integration tests
├── docs/                   # Documentation
├── scripts/                # Build scripts
└── .github/                # CI/CD workflows
```

## Building

### Workspace Build Commands

```bash
# Build all crates in workspace (debug mode)
cargo build

# Build all crates in workspace (release mode)
cargo build --release

# Build specific crate
cargo build -p talaria-cli --release

# Build with all features
cargo build --release --all-features

# Build and run tests
cargo test --workspace

# Build documentation
cargo doc --workspace --no-deps --open
```

### Individual Crate Builds

```bash
# Build only the CLI
cd talaria-cli && cargo build --release

# Build only the HERALD library
cd talaria-herald && cargo build --release

# Build as library (no CLI)
cargo build -p talaria-herald -p talaria-bio -p talaria-storage
```

### Build Profiles

#### Development Profile

```toml
# Cargo.toml
[profile.dev]
opt-level = 0
debug = true
debug-assertions = true
overflow-checks = true
lto = false
panic = 'unwind'
incremental = true
codegen-units = 256
```

#### Release Profile

```toml
[profile.release]
opt-level = 3
debug = false
debug-assertions = false
overflow-checks = false
lto = "thin"
panic = 'abort'
incremental = false
codegen-units = 1
strip = true
```

#### Optimized Profile

```toml
[profile.release-with-debug]
inherits = "release"
strip = false
debug = true
```

### Feature Flags

Each crate has its own features. Key features:

#### talaria-cli Features
| Feature | Description | Default |
|---------|-------------|---------|
| `default` | Standard CLI features | ✓ |
| `interactive` | Terminal UI | ✓ |
| `html-report` | HTML report generation | ✓ |

#### talaria-herald Features
| Feature | Description | Default |
|---------|-------------|---------|
| `default` | Core HERALD features | ✓ |
| `cloud` | Cloud storage support | ✗ |
| `distributed` | Distributed processing | ✗ |

#### talaria-bio Features
| Feature | Description | Default |
|---------|-------------|---------|
| `default` | Core bio features | ✓ |
| `simd` | SIMD acceleration | ✓ |
| `mmap` | Memory-mapped I/O | ✓ |

```bash
# Build with specific features
cargo build --release --features "cloud distributed"

# Build without default features
cargo build --release --no-default-features --features "core"
```

## Installation

### Install from Workspace

```bash
# Install the CLI binary
cargo install --path talaria-cli

# Install with specific features
cargo install --path talaria-cli --features "cloud"
```

### System-Wide Installation

```bash
# Build optimized binary
cargo build --release -p talaria-cli

# Copy to system PATH
sudo cp target/release/talaria /usr/local/bin/

# Or create symlink
sudo ln -s $(pwd)/target/release/talaria /usr/local/bin/talaria
```

### Using as Library

Add to your project's `Cargo.toml`:

```toml
[dependencies]
talaria-bio = { git = "https://github.com/yourusername/talaria" }
talaria-herald = { git = "https://github.com/yourusername/talaria" }

# Or from local path
talaria-bio = { path = "../talaria/talaria-bio" }
talaria-herald = { path = "../talaria/talaria-herald" }
```

## Testing

### Running Tests

```bash
# Run all tests (unit + integration)
cargo test --workspace

# Run tests for specific crate
cargo test -p talaria-herald

# Run integration tests only
cargo test --test '*'

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_chunking

# Run benchmarks
cargo bench
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --workspace

# Open report
open tarpaulin-report.html
```

## Docker Build

### Building Docker Image

```dockerfile
# Dockerfile
FROM rust:1.75 AS builder

WORKDIR /app
COPY . .
RUN cargo build --release -p talaria-cli

FROM ubuntu:22.04
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/talaria /usr/local/bin/
ENTRYPOINT ["talaria"]
```

```bash
# Build image
docker build -t talaria:latest .

# Run container
docker run --rm talaria:latest reduce --help
```

## Cross-Compilation

### Setup Cross

```bash
# Install cross
cargo install cross

# Build for Linux x86_64
cross build --release --target x86_64-unknown-linux-gnu

# Build for Linux ARM64
cross build --release --target aarch64-unknown-linux-gnu

# Build for macOS (from Linux)
cross build --release --target x86_64-apple-darwin
```

## Troubleshooting

### Common Issues

#### Linking Errors

```bash
# Linux: Install missing libraries
sudo apt-get install libssl-dev pkg-config

# macOS: Set OpenSSL path
export OPENSSL_DIR=$(brew --prefix openssl)
export PKG_CONFIG_PATH=$OPENSSL_DIR/lib/pkgconfig
```

#### Out of Memory

```bash
# Reduce parallel jobs
cargo build -j 2

# Or set in config
export CARGO_BUILD_JOBS=2
```

#### Slow Compilation

```bash
# Use sccache for caching
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux)
sudo apt install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
```

### Performance Optimization

```bash
# CPU-specific optimizations
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Profile-guided optimization
cargo build --release
./target/release/talaria reduce -i test.fasta -o /dev/null
cargo build --release --profile pgo
```

## Development Setup

### IDE Setup

#### VS Code

```json
// .vscode/settings.json
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.target": "x86_64-unknown-linux-gnu"
}
```

#### IntelliJ/CLion

1. Install Rust plugin
2. Open project root
3. Configure toolchain in Settings → Rust

### Pre-commit Hooks

```bash
# Install pre-commit
pip install pre-commit

# Setup hooks
cat > .pre-commit-config.yaml << EOF
repos:
  - repo: local
    hooks:
      - id: fmt
        name: cargo fmt
        entry: cargo fmt --all -- --check
        language: system
        pass_filenames: false
      - id: clippy
        name: cargo clippy
        entry: cargo clippy --workspace -- -D warnings
        language: system
        pass_filenames: false
      - id: test
        name: cargo test
        entry: cargo test --workspace
        language: system
        pass_filenames: false
EOF

pre-commit install
```

## Continuous Integration

### GitHub Actions

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace
      - run: cargo test --workspace
      - run: cargo clippy --workspace -- -D warnings
```

## See Also

- [Architecture](architecture.md) - System design
- [Contributing](contributing.md) - Development guidelines
- [Testing](../testing.md) - Testing guide
- [Performance](../advanced/performance.md) - Optimization tips