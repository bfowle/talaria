# Installation

Talaria can be installed through multiple methods depending on your needs and platform.

## System Requirements

### Minimum Requirements
- **CPU**: x86_64 or ARM64 processor
- **RAM**: 4 GB (8 GB recommended for large datasets)
- **Disk**: 500 MB for binary + space for databases
- **OS**: Linux, macOS, or Windows (via WSL2)

### Prerequisites
- Rust 1.70+ (for building from source)
- Git (for cloning repository)
- C compiler (gcc/clang for native dependencies)

## Installation Methods

### Binary Installation (Recommended)

#### Linux/macOS
```bash
# Download the latest release
curl -L https://github.com/bfowle/talaria/releases/latest/download/talaria-$(uname -s)-$(uname -m) -o talaria
chmod +x talaria
sudo mv talaria /usr/local/bin/

# Verify installation
talaria --version
```

#### Windows (WSL2)
```bash
# Inside WSL2 terminal
curl -L https://github.com/bfowle/talaria/releases/latest/download/talaria-Linux-x86_64 -o talaria
chmod +x talaria
sudo mv talaria /usr/local/bin/
```

### Package Managers

#### Homebrew (macOS/Linux)
```bash
brew tap andromeda-tech/talaria
brew install talaria
```

#### Cargo (Cross-platform)
```bash
cargo install talaria
```

#### Conda
```bash
conda install -c bioconda talaria
```

### Building from Source

#### Clone and Build
```bash
# Clone the repository
git clone https://github.com/bfowle/talaria.git
cd talaria

# Build in release mode
cargo build --release

# Install to system
sudo cp target/release/talaria /usr/local/bin/

# Or install via cargo
cargo install --path .
```

#### Development Build
```bash
# Clone with full history
git clone --recursive https://github.com/bfowle/talaria.git
cd talaria

# Install development dependencies
rustup component add rustfmt clippy
cargo install mdbook mdbook-mermaid

# Build with all features
cargo build --all-features

# Run tests
cargo test
```

## Platform-Specific Notes

### Linux
- Ensure `glibc` >= 2.31 for pre-built binaries
- For MUSL-based systems (Alpine), build from source

### macOS
- Apple Silicon (M1/M2) users should use the `aarch64` binary
- Intel Macs use the `x86_64` binary
- May require allowing unsigned binaries in Security settings

### Windows
- Native Windows support via WSL2 only
- Ensure WSL2 is properly configured with Ubuntu 20.04+
- Performance is best with files stored in WSL2 filesystem

## Docker Installation

```dockerfile
# Official Docker image
docker pull ghcr.io/andromeda-tech/talaria:latest

# Run with local directory mounted
docker run -v $(pwd):/data ghcr.io/andromeda-tech/talaria:latest \
    reduce -i /data/input.fasta -o /data/output.fasta
```

### Docker Compose
```yaml
version: '3.8'
services:
  talaria:
    image: ghcr.io/andromeda-tech/talaria:latest
    volumes:
      - ./data:/data
      - ./config:/config
    environment:
      - TALARIA_THREADS=8
      - RUST_LOG=info
```

## Configuration

### Environment Variables
```bash
# Set number of threads
export TALARIA_THREADS=8

# Set log level
export RUST_LOG=talaria=debug

# Custom config location
export TALARIA_CONFIG=/path/to/config.toml
```

### Initial Setup
```bash
# Create config directory
mkdir -p ~/.config/talaria

# Generate default configuration
talaria config init

# Download reference databases (interactive)
talaria download --interactive
```

## Verification

### Basic Test
```bash
# Check version
talaria --version

# Run help
talaria --help

# Quick test with sample data
curl -L https://github.com/bfowle/talaria/raw/main/tests/data/sample.fasta -o sample.fasta
talaria reduce -i sample.fasta -o reduced.fasta
talaria stats reduced.fasta
```

### Performance Test
```bash
# Download test dataset
talaria download --database uniprot --dataset swissprot

# Run reduction benchmark
talaria reduce \
    -i uniprot_sprot.fasta \
    -o sprot_reduced.fasta \
    --aligner lambda \
    --threads 8 \
    --verbose
```

## Troubleshooting

### Common Issues

#### Permission Denied
```bash
# Fix permissions
chmod +x talaria
# Or use sudo for system install
sudo mv talaria /usr/local/bin/
```

#### Library Not Found
```bash
# Linux: Install dependencies
sudo apt-get update
sudo apt-get install libssl-dev pkg-config

# macOS: Use Homebrew
brew install openssl pkg-config
```

#### Out of Memory
```bash
# Increase memory limits
ulimit -v unlimited

# Use memory-efficient mode
talaria reduce --optimize-memory ...
```

### Getting Help
- GitHub Issues: https://github.com/bfowle/talaria/issues
- Documentation: https://andromeda-tech.github.io/talaria/
- Discord: https://discord.gg/talaria

## Next Steps

- Read the [Quick Start](quick-start.md) guide
- Explore [Basic Usage](basic-usage.md)
- Configure for your [specific aligner](../workflows/)