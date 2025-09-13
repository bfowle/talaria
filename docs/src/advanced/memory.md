# Memory Management

Advanced memory management techniques for handling large-scale sequence databases efficiently.

## Memory Architecture

### Memory Hierarchy

Talaria optimizes for modern memory hierarchies:

```
L1 Cache (32-256 KB) - Per-core, fastest
    ↓
L2 Cache (256 KB-1 MB) - Per-core, fast
    ↓
L3 Cache (8-32 MB) - Shared, moderate
    ↓
Main Memory (GB-TB) - DRAM, slower
    ↓
Storage (TB-PB) - SSD/HDD, slowest
```

### Memory Layout

```rust
// Optimized sequence storage layout
pub struct SequenceBuffer {
    // Hot data (frequently accessed)
    headers: Vec<CompactHeader>,     // 16 bytes per sequence
    lengths: Vec<u32>,               // 4 bytes per sequence
    offsets: Vec<u64>,               // 8 bytes per sequence
    
    // Cold data (rarely accessed)
    sequences: MmapVec<u8>,          // Memory-mapped sequences
    metadata: Option<Box<Metadata>>, // Optional metadata
}
```

## Memory-Mapped I/O

### Basic Memory Mapping

```rust
use memmap2::{Mmap, MmapOptions};
use std::fs::File;

pub struct MappedFasta {
    mmap: Mmap,
    index: Vec<(usize, usize)>, // (offset, length) pairs
}

impl MappedFasta {
    pub fn new(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        
        // Build index for fast random access
        let index = Self::build_index(&mmap);
        
        Ok(Self { mmap, index })
    }
    
    pub fn get_sequence(&self, idx: usize) -> &[u8] {
        let (offset, length) = self.index[idx];
        &self.mmap[offset..offset + length]
    }
}
```

### Advanced Memory Mapping

```toml
[memory.mapping]
# Memory mapping configuration
use_memory_mapping = true
mmap_threshold_mb = 100     # Files larger than this use mmap
populate_on_map = false     # Pre-fault pages
huge_pages = true          # Use huge pages (2MB/1GB)
numa_aware = true          # NUMA-aware mapping
```

## Memory Pooling

### Object Pools

```rust
use parking_lot::Mutex;
use std::sync::Arc;

pub struct AlignmentPool {
    pool: Arc<Mutex<Vec<AlignmentMatrix>>>,
    max_size: usize,
}

impl AlignmentPool {
    pub fn acquire(&self, rows: usize, cols: usize) -> PooledMatrix {
        let mut pool = self.pool.lock();
        
        let matrix = pool.iter()
            .position(|m| m.capacity() >= rows * cols)
            .map(|idx| pool.swap_remove(idx))
            .unwrap_or_else(|| AlignmentMatrix::new(rows, cols));
        
        PooledMatrix::new(matrix, Arc::clone(&self.pool))
    }
}
```

### Arena Allocation

```rust
pub struct SequenceArena {
    chunks: Vec<Vec<u8>>,
    current: Vec<u8>,
    chunk_size: usize,
}

impl SequenceArena {
    pub fn alloc_sequence(&mut self, seq: &[u8]) -> ArenaRef {
        if self.current.len() + seq.len() > self.chunk_size {
            let chunk = std::mem::replace(
                &mut self.current,
                Vec::with_capacity(self.chunk_size)
            );
            self.chunks.push(chunk);
        }
        
        let offset = self.current.len();
        self.current.extend_from_slice(seq);
        
        ArenaRef {
            chunk: self.chunks.len(),
            offset,
            length: seq.len(),
        }
    }
}
```

## Cache Optimization

### Cache-Friendly Data Structures

```rust
// Structure of Arrays (SoA) for better cache utilization
pub struct SequenceDataSoA {
    ids: Vec<u64>,
    lengths: Vec<u32>,
    gc_contents: Vec<f32>,
    complexities: Vec<f32>,
}

// Array of Structures (AoS) - less cache friendly
pub struct SequenceDataAoS {
    sequences: Vec<SequenceInfo>,
}

pub struct SequenceInfo {
    id: u64,
    length: u32,
    gc_content: f32,
    complexity: f32,
}
```

### Prefetching Strategies

```rust
use std::intrinsics;

pub fn process_sequences_prefetch(sequences: &[Sequence]) {
    const PREFETCH_DISTANCE: usize = 8;
    
    for i in 0..sequences.len() {
        // Prefetch future data
        if i + PREFETCH_DISTANCE < sequences.len() {
            unsafe {
                intrinsics::prefetch_read_data(
                    &sequences[i + PREFETCH_DISTANCE] as *const _ as *const i8,
                    3 // Temporal locality hint
                );
            }
        }
        
        // Process current sequence
        process_sequence(&sequences[i]);
    }
}
```

## Streaming Processing

### Stream-Based Architecture

```rust
pub struct StreamProcessor {
    buffer_size: usize,
    prefetch_size: usize,
    process_fn: Box<dyn Fn(&[u8]) -> Result<()>>,
}

impl StreamProcessor {
    pub async fn process_file(&self, path: &Path) -> Result<()> {
        let file = tokio::fs::File::open(path).await?;
        let mut reader = BufReader::with_capacity(self.buffer_size, file);
        let mut buffer = Vec::with_capacity(self.prefetch_size);
        
        loop {
            buffer.clear();
            let bytes_read = reader.read_buf(&mut buffer).await?;
            
            if bytes_read == 0 {
                break;
            }
            
            (self.process_fn)(&buffer)?;
        }
        
        Ok(())
    }
}
```

### Chunked Processing

```toml
[memory.streaming]
# Streaming configuration
chunk_size = 10000        # Sequences per chunk
buffer_count = 3          # Triple buffering
read_ahead = true         # Prefetch next chunk
compress_chunks = false   # In-memory compression
```

## Garbage Collection

### Manual Memory Management

```rust
pub struct MemoryManager {
    allocated: AtomicUsize,
    limit: usize,
    gc_threshold: f64,
}

impl MemoryManager {
    pub fn should_gc(&self) -> bool {
        let current = self.allocated.load(Ordering::Relaxed);
        current as f64 > self.limit as f64 * self.gc_threshold
    }
    
    pub fn run_gc(&self, cache: &mut AlignmentCache) {
        // Clear least recently used entries
        let target_size = (self.limit as f64 * 0.7) as usize;
        cache.evict_to_size(target_size);
        
        // Compact memory
        self.compact_memory();
    }
    
    fn compact_memory(&self) {
        // Trigger system memory compaction
        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
    }
}
```

### Reference Counting

```rust
use std::rc::Rc;
use std::sync::Arc;

pub struct SharedSequence {
    data: Arc<Vec<u8>>,
    offset: usize,
    length: usize,
}

impl SharedSequence {
    pub fn substring(&self, start: usize, end: usize) -> Self {
        Self {
            data: Arc::clone(&self.data),
            offset: self.offset + start,
            length: end - start,
        }
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.data[self.offset..self.offset + self.length]
    }
}
```

## NUMA Optimization

### NUMA-Aware Allocation

```rust
#[cfg(target_os = "linux")]
pub struct NumaAllocator {
    node: i32,
}

#[cfg(target_os = "linux")]
impl NumaAllocator {
    pub fn alloc_on_node(&self, size: usize) -> *mut u8 {
        use libc::{numa_alloc_onnode, numa_node_size};
        
        unsafe {
            numa_alloc_onnode(size, self.node) as *mut u8
        }
    }
    
    pub fn bind_to_node(&self) {
        use libc::{numa_run_on_node, numa_set_membind};
        
        unsafe {
            numa_run_on_node(self.node);
            let mut nodemask = 0u64;
            nodemask |= 1 << self.node;
            numa_set_membind(&nodemask as *const _ as *const libc::c_void);
        }
    }
}
```

### NUMA Configuration

```toml
[memory.numa]
# NUMA settings
numa_aware = true
numa_nodes = 2
interleave = false        # Interleave memory across nodes
local_alloc = true        # Prefer local node allocation
migration = false         # Allow page migration
```

## Memory Compression

### In-Memory Compression

```rust
use lz4::{Decoder, EncoderBuilder};

pub struct CompressedBuffer {
    compressed: Vec<u8>,
    original_size: usize,
    compression_level: u32,
}

impl CompressedBuffer {
    pub fn compress(data: &[u8], level: u32) -> Result<Self> {
        let mut encoder = EncoderBuilder::new()
            .level(level)
            .build(Vec::new())?;
        
        encoder.write_all(data)?;
        let (compressed, result) = encoder.finish();
        result?;
        
        Ok(Self {
            compressed,
            original_size: data.len(),
            compression_level: level,
        })
    }
    
    pub fn decompress(&self) -> Result<Vec<u8>> {
        let mut decoder = Decoder::new(&self.compressed[..])?;
        let mut decompressed = Vec::with_capacity(self.original_size);
        decoder.read_to_end(&mut decompressed)?;
        Ok(decompressed)
    }
}
```

### Compression Strategies

```toml
[memory.compression]
# Compression settings
enable_compression = true
algorithm = "lz4"         # Options: lz4, zstd, snappy
level = 3                 # 1-9, higher = better ratio
threshold_kb = 64         # Compress chunks larger than this
async_compression = true  # Compress in background
```

## Memory Monitoring

### Runtime Monitoring

```rust
use sysinfo::{System, SystemExt};

pub struct MemoryMonitor {
    system: System,
    warning_threshold: f64,
    critical_threshold: f64,
}

impl MemoryMonitor {
    pub fn check_memory(&mut self) -> MemoryStatus {
        self.system.refresh_memory();
        
        let total = self.system.total_memory();
        let used = self.system.used_memory();
        let available = self.system.available_memory();
        
        let usage_percent = (used as f64 / total as f64) * 100.0;
        
        if usage_percent > self.critical_threshold {
            MemoryStatus::Critical { usage_percent, available }
        } else if usage_percent > self.warning_threshold {
            MemoryStatus::Warning { usage_percent, available }
        } else {
            MemoryStatus::Ok { usage_percent, available }
        }
    }
}
```

### Memory Profiling

```bash
# Heap profiling with heaptrack
heaptrack talaria reduce -i input.fasta -o output.fasta
heaptrack_gui heaptrack.talaria.*.gz

# Valgrind memory analysis
valgrind --tool=massif --massif-out-file=massif.out talaria reduce -i input.fasta -o output.fasta
ms_print massif.out

# Memory leak detection
valgrind --leak-check=full --show-leak-kinds=all talaria reduce -i input.fasta -o output.fasta
```

## Low-Memory Mode

### Configuration

```toml
[memory.low_memory]
# Low memory mode settings
enabled = true
max_memory_mb = 2048      # Hard memory limit
streaming_only = true     # Force streaming mode
disable_cache = false     # Disable alignment cache
aggressive_gc = true      # Frequent garbage collection
swap_to_disk = true      # Use disk for overflow
```

### Implementation

```rust
pub struct LowMemoryProcessor {
    memory_limit: usize,
    temp_dir: PathBuf,
    current_usage: AtomicUsize,
}

impl LowMemoryProcessor {
    pub fn process_with_limit(&self, sequences: &[Sequence]) -> Result<()> {
        let chunk_size = self.calculate_chunk_size(sequences.len());
        
        for chunk in sequences.chunks(chunk_size) {
            // Check memory before processing
            if self.would_exceed_limit(chunk) {
                self.flush_to_disk()?;
            }
            
            // Process chunk
            self.process_chunk(chunk)?;
            
            // Aggressive cleanup
            self.cleanup_memory();
        }
        
        Ok(())
    }
    
    fn would_exceed_limit(&self, chunk: &[Sequence]) -> bool {
        let estimated_size = chunk.iter()
            .map(|s| s.estimated_memory_usage())
            .sum::<usize>();
        
        let current = self.current_usage.load(Ordering::Relaxed);
        current + estimated_size > self.memory_limit
    }
}
```

## Memory Safety

### Safe Abstractions

```rust
use std::pin::Pin;

pub struct PinnedBuffer {
    data: Pin<Box<[u8]>>,
}

impl PinnedBuffer {
    pub fn new(size: usize) -> Self {
        let data = vec![0u8; size].into_boxed_slice();
        Self {
            data: Pin::new(data),
        }
    }
    
    pub fn as_slice(&self) -> &[u8] {
        &*self.data
    }
    
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { self.data.as_mut().get_unchecked_mut() }
    }
}
```

### Bounds Checking

```rust
#[inline(always)]
pub fn safe_slice<'a>(data: &'a [u8], start: usize, end: usize) -> Option<&'a [u8]> {
    if start <= end && end <= data.len() {
        Some(&data[start..end])
    } else {
        None
    }
}

#[inline(always)]
pub fn checked_index(data: &[u8], index: usize) -> Option<u8> {
    data.get(index).copied()
}
```

## Best Practices

### Memory Efficiency Guidelines

1. **Use Memory Mapping**: For files > 100MB
2. **Enable Streaming**: For files > available RAM
3. **Pool Objects**: Reuse expensive allocations
4. **Cache Wisely**: Balance speed vs memory
5. **Monitor Usage**: Track memory in production
6. **Handle OOM**: Graceful degradation
7. **Profile Regularly**: Identify memory leaks
8. **Compress Data**: Trade CPU for memory

### Configuration Examples

#### High-Memory System

```toml
[memory]
max_memory_gb = 128
use_huge_pages = true
numa_aware = true
cache_size_gb = 32
prefetch_distance = 16
aggressive_gc = false
```

#### Low-Memory System

```toml
[memory]
max_memory_gb = 4
streaming_mode = true
cache_size_mb = 256
compression_enabled = true
swap_to_disk = true
aggressive_gc = true
```

#### Balanced Configuration

```toml
[memory]
max_memory_gb = 16
adaptive_mode = true
cache_size_gb = 4
compression_threshold_mb = 64
gc_threshold = 0.8
```

## Troubleshooting

### Common Issues

#### Out of Memory

**Symptoms**: Process killed, OOM errors

**Solutions**:
```bash
# Enable low-memory mode
talaria reduce --low-memory -i input.fasta -o output.fasta

# Limit memory usage
talaria reduce --max-memory 4G -i input.fasta -o output.fasta

# Use streaming
talaria reduce --stream -i input.fasta -o output.fasta
```

#### Memory Leaks

**Detection**:
```bash
# Check for leaks
valgrind --leak-check=full talaria reduce -i test.fasta -o out.fasta

# Monitor memory growth
talaria reduce --monitor-memory -i input.fasta -o output.fasta
```

#### Poor Cache Performance

**Symptoms**: High memory bandwidth, cache misses

**Solutions**:
```toml
[memory.cache]
# Optimize cache usage
prefetch_distance = 8
cache_line_size = 64
align_structures = true
pack_data = true
```

## See Also

- [Performance Optimization](performance.md) - Performance tuning
- [Parallel Processing](parallel.md) - Parallel memory access
- [Configuration](../user-guide/configuration.md) - Memory settings
- [Troubleshooting](../troubleshooting.md) - Memory issues