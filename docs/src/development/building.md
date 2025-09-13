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

### Repository Structure

```
talaria/
├── Cargo.toml          # Main package manifest
├── Cargo.lock          # Dependency lock file
├── src/                # Source code
├── tests/              # Test files
├── benches/            # Benchmarks
├── docs/               # Documentation
├── scripts/            # Build scripts
└── .github/            # CI/CD workflows
```

## Building

### Standard Build

```bash
# Development build (debug mode)
cargo build

# Release build (optimized)
cargo build --release

# Build with all features
cargo build --release --all-features

# Build specific features
cargo build --release --features "gpu simd"
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
lto = "fat"
panic = 'abort'
incremental = false
codegen-units = 1
strip = true
```

#### Optimized Profile

```toml
[profile.optimized]
inherits = "release"
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

### Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `default` | Standard features | ✓ |
| `simd` | SIMD acceleration | ✓ |
| `parallel` | Parallel processing | ✓ |
| `compression` | Compression support | ✓ |
| `gpu` | GPU acceleration | ✗ |
| `distributed` | Distributed processing | ✗ |
| `python` | Python bindings | ✗ |

```bash
# Build with specific features
cargo build --release --features "gpu python"

# Build without default features
cargo build --release --no-default-features

# Build with all features
cargo build --release --all-features
```

## Platform-Specific Builds

### Linux Build

```bash
# Optimized for native CPU
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Static linking
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release

# Musl target (fully static)
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

### macOS Build

```bash
# Universal binary (Intel + ARM)
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Create universal binary
lipo -create \
    target/x86_64-apple-darwin/release/talaria \
    target/aarch64-apple-darwin/release/talaria \
    -output talaria-universal
```

### Windows Build

```powershell
# MSVC toolchain (default)
cargo build --release

# GNU toolchain
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu

# Static CRT linking
set RUSTFLAGS=-C target-feature=+crt-static
cargo build --release
```

### Cross-Compilation

```bash
# Install cross
cargo install cross

# Build for ARM64 Linux
cross build --release --target aarch64-unknown-linux-gnu

# Build for ARM32 Linux
cross build --release --target armv7-unknown-linux-gnueabihf

# Build for MIPS
cross build --release --target mips64-unknown-linux-gnuabi64
```

## Docker Build

### Standard Dockerfile

```dockerfile
# Build stage
FROM rust:1.75 as builder

WORKDIR /usr/src/talaria
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/talaria/target/release/talaria /usr/local/bin/

ENTRYPOINT ["talaria"]
```

### Multi-arch Build

```bash
# Setup buildx
docker buildx create --use

# Build for multiple platforms
docker buildx build \
    --platform linux/amd64,linux/arm64,linux/arm/v7 \
    --tag talaria:latest \
    --push .
```

## Advanced Build Options

### Link-Time Optimization (LTO)

```toml
[profile.release]
lto = "fat"  # Full LTO
# or
lto = "thin" # Thin LTO (faster builds)
```

### Profile-Guided Optimization (PGO)

```bash
# Step 1: Build with profiling
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" \
    cargo build --release

# Step 2: Run with representative workload
./target/release/talaria reduce -i sample.fasta -o output.fasta

# Step 3: Build with profile data
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data" \
    cargo build --release
```

### Custom Allocators

```toml
# Cargo.toml
[dependencies]
jemallocator = { version = "0.5", optional = true }
mimalloc = { version = "0.1", optional = true }

[features]
jemalloc = ["jemallocator"]
mimalloc = ["mimalloc"]
```

```rust
// src/main.rs
#[cfg(feature = "jemalloc")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

## Testing

### Run Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_alignment

# Run with output
cargo test -- --nocapture

# Run with multiple threads
cargo test -- --test-threads=4

# Run ignored tests
cargo test -- --ignored

# Run benchmarks
cargo bench
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage

# With specific features
cargo tarpaulin --features "gpu simd" --out Xml
```

## Building Documentation

```bash
# Build documentation
cargo doc

# Build and open in browser
cargo doc --open

# Build with private items
cargo doc --document-private-items

# Build for all dependencies
cargo doc --all --no-deps
```

## Continuous Integration

### GitHub Actions

```yaml
# .github/workflows/build.yml
name: Build

on: [push, pull_request]

jobs:
  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
        rust: [stable, beta, nightly]
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: ${{ matrix.rust }}
        override: true
    
    - name: Build
      run: cargo build --release --all-features
    
    - name: Test
      run: cargo test --all-features
```

## Troubleshooting

### Common Build Issues

#### 1. Linking Errors

**Problem**: Undefined references during linking

**Solution**:
```bash
# Clean build
cargo clean
cargo build

# Check for missing system libraries
pkg-config --libs openssl
```

#### 2. Out of Memory

**Problem**: Build fails with OOM

**Solution**:
```bash
# Reduce codegen units
CARGO_BUILD_JOBS=1 cargo build --release

# Or modify Cargo.toml
[profile.release]
codegen-units = 1
```

#### 3. Slow Builds

**Problem**: Compilation takes too long

**Solutions**:
```bash
# Use sccache
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux)
RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo build

# Incremental compilation
CARGO_INCREMENTAL=1 cargo build
```

#### 4. Feature Conflicts

**Problem**: Incompatible features

**Solution**:
```bash
# Check feature dependencies
cargo tree --features "feature1 feature2"

# Build with resolver v2
# In Cargo.toml:
[package]
resolver = "2"
```

## Build Scripts

### Makefile

```makefile
.PHONY: all build release test clean

all: build

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

bench:
	cargo bench

clean:
	cargo clean

install: release
	cargo install --path .

docker:
	docker build -t talaria .
```

### Build Script (build.rs)

```rust
// build.rs
use std::env;

fn main() {
    // Set version from git
    if let Ok(output) = std::process::Command::new("git")
        .args(&["describe", "--tags", "--always"])
        .output()
    {
        let git_version = String::from_utf8(output.stdout).unwrap();
        println!("cargo:rustc-env=GIT_VERSION={}", git_version);
    }
    
    // Link native libraries
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=ssl");
        println!("cargo:rustc-link-lib=crypto");
    }
}
```

## Performance Builds

### Maximum Performance

```bash
# CPU-specific optimizations
RUSTFLAGS="-C target-cpu=native -C opt-level=3" \
    cargo build --release

# With additional flags
RUSTFLAGS="-C target-cpu=native \
          -C opt-level=3 \
          -C lto=fat \
          -C embed-bitcode=yes \
          -C codegen-units=1 \
          -C inline-threshold=1000" \
    cargo build --release
```

### Binary Size Optimization

```bash
# Minimize binary size
RUSTFLAGS="-C opt-level=z" cargo build --release

# Strip symbols
strip target/release/talaria

# Or use cargo configuration
[profile.release]
opt-level = "z"
strip = true
panic = "abort"
```

## Distribution

### Creating Release Packages

```bash
# Create tarball
tar czf talaria-${VERSION}-${TARGET}.tar.gz \
    -C target/release talaria

# Create debian package
cargo install cargo-deb
cargo deb

# Create RPM package
cargo install cargo-rpm
cargo rpm build

# Create Windows installer
cargo install cargo-wix
cargo wix
```

## See Also

- [Architecture](architecture.md) - System design
- [Contributing](contributing.md) - Development guidelines
- [Installation](../user-guide/installation.md) - Installation methods
- [Configuration](../user-guide/configuration.md) - Runtime configuration