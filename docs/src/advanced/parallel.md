# Parallel Processing

Advanced parallel and concurrent processing strategies for maximizing throughput on multi-core systems.

## Parallelization Architecture

### Threading Model

```rust
use rayon::prelude::*;
use std::sync::Arc;
use crossbeam::channel;

pub struct ParallelProcessor {
    thread_pool: rayon::ThreadPool,
    chunk_size: usize,
    work_stealing: bool,
}

impl ParallelProcessor {
    pub fn new(num_threads: usize) -> Result<Self> {
        let thread_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name(|idx| format!("talaria-worker-{}", idx))
            .build()?;
        
        Ok(Self {
            thread_pool,
            chunk_size: 1000,
            work_stealing: true,
        })
    }
    
    pub fn process_parallel<T>(&self, items: Vec<T>) -> Vec<Result<T>>
    where
        T: Send + Sync + 'static,
    {
        self.thread_pool.install(|| {
            items.into_par_iter()
                .chunks(self.chunk_size)
                .flat_map(|chunk| {
                    chunk.into_iter()
                        .map(|item| self.process_item(item))
                        .collect::<Vec<_>>()
                })
                .collect()
        })
    }
}
```

### Work Distribution

```rust
use dashmap::DashMap;
use parking_lot::RwLock;

pub struct WorkDistributor {
    tasks: Arc<RwLock<VecDeque<Task>>>,
    results: Arc<DashMap<usize, Result>>,
    workers: Vec<JoinHandle<()>>,
}

impl WorkDistributor {
    pub fn distribute(&self, num_workers: usize) {
        let (tx, rx) = crossbeam::channel::bounded(num_workers * 2);
        
        // Producer thread
        let producer = thread::spawn(move || {
            while let Some(task) = self.get_next_task() {
                tx.send(task).unwrap();
            }
        });
        
        // Worker threads
        for _ in 0..num_workers {
            let rx = rx.clone();
            let results = Arc::clone(&self.results);
            
            let worker = thread::spawn(move || {
                while let Ok(task) = rx.recv() {
                    let result = process_task(task);
                    results.insert(task.id, result);
                }
            });
            
            self.workers.push(worker);
        }
    }
}
```

## Data Parallelism

### Parallel Iteration

```rust
use rayon::prelude::*;

pub fn parallel_reduction(sequences: &[Sequence]) -> Vec<Reference> {
    sequences.par_iter()
        .chunks(1000)
        .map(|chunk| {
            // Process chunk in parallel
            chunk.par_iter()
                .filter(|seq| seq.length() > MIN_LENGTH)
                .map(|seq| compute_similarity(seq))
                .collect::<Vec<_>>()
        })
        .flatten()
        .collect()
}

pub fn parallel_alignment(queries: &[Sequence], references: &[Sequence]) -> Vec<Alignment> {
    queries.par_iter()
        .flat_map(|query| {
            references.par_iter()
                .map(|reference| align(query, reference))
                .collect::<Vec<_>>()
        })
        .collect()
}
```

### SIMD Parallelism

```rust
use packed_simd::{u8x32, f32x8};

pub fn simd_sequence_comparison(seq1: &[u8], seq2: &[u8]) -> u32 {
    let mut matches = 0u32;
    let chunks = seq1.chunks_exact(32).zip(seq2.chunks_exact(32));
    
    for (chunk1, chunk2) in chunks {
        let v1 = u8x32::from_slice_unaligned(chunk1);
        let v2 = u8x32::from_slice_unaligned(chunk2);
        let mask = v1.eq(v2);
        matches += mask.select(u8x32::splat(1), u8x32::splat(0)).wrapping_sum() as u32;
    }
    
    // Handle remainder
    let remainder1 = &seq1[seq1.len() & !31..];
    let remainder2 = &seq2[seq2.len() & !31..];
    matches += remainder1.iter()
        .zip(remainder2.iter())
        .filter(|(a, b)| a == b)
        .count() as u32;
    
    matches
}
```

## Task Parallelism

### Pipeline Architecture

```rust
use tokio::sync::mpsc;
use futures::stream::{Stream, StreamExt};

pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

#[async_trait]
trait Stage: Send + Sync {
    async fn process(&self, input: Data) -> Result<Data>;
}

impl Pipeline {
    pub async fn run(&self, input: impl Stream<Item = Data>) -> impl Stream<Item = Result<Data>> {
        let (tx, mut rx) = mpsc::channel(100);
        
        // Chain stages
        let mut stream = Box::pin(input);
        for stage in &self.stages {
            stream = Box::pin(stream.then(move |data| async move {
                stage.process(data).await
            }));
        }
        
        // Collect results
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                let _ = tx.send(result).await;
            }
        });
        
        rx
    }
}
```

### Concurrent I/O

```rust
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

pub async fn concurrent_file_processing(paths: Vec<PathBuf>) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(10)); // Limit concurrent files
    
    let tasks = paths.into_iter().map(|path| {
        let sem = Arc::clone(&semaphore);
        
        tokio::spawn(async move {
            let _permit = sem.acquire().await?;
            process_file(path).await
        })
    });
    
    // Wait for all tasks
    let results = futures::future::join_all(tasks).await;
    
    for result in results {
        result??;
    }
    
    Ok(())
}
```

## Thread Pools

### Custom Thread Pool

```rust
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Job>,
}

impl ThreadPool {
    pub fn new(size: usize, affinity: Option<Vec<usize>>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        
        let workers = (0..size)
            .map(|id| {
                let receiver = Arc::clone(&receiver);
                Worker::new(id, receiver, affinity.as_ref().map(|a| a[id]))
            })
            .collect();
        
        ThreadPool { workers, sender }
    }
    
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender.send(job).unwrap();
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>, cpu: Option<usize>) -> Worker {
        let thread = thread::spawn(move || {
            // Set CPU affinity if specified
            if let Some(cpu) = cpu {
                set_cpu_affinity(cpu);
            }
            
            loop {
                let job = receiver.lock().unwrap().recv();
                
                match job {
                    Ok(job) => job(),
                    Err(_) => break,
                }
            }
        });
        
        Worker {
            id,
            thread: Some(thread),
        }
    }
}
```

### Work Stealing

```rust
use crossbeam::deque::{Injector, Stealer, Worker};

pub struct WorkStealingPool {
    global: Arc<Injector<Task>>,
    workers: Vec<WorkerThread>,
}

struct WorkerThread {
    local: Worker<Task>,
    stealers: Vec<Stealer<Task>>,
    global: Arc<Injector<Task>>,
}

impl WorkerThread {
    fn run(&mut self) {
        loop {
            // Try local queue first
            if let Some(task) = self.local.pop() {
                process_task(task);
                continue;
            }
            
            // Try stealing from others
            for stealer in &self.stealers {
                if let Some(task) = stealer.steal().success() {
                    process_task(task);
                    continue;
                }
            }
            
            // Try global queue
            if let Some(task) = self.global.steal().success() {
                process_task(task);
                continue;
            }
            
            // No work available, yield
            thread::yield_now();
        }
    }
}
```

## Synchronization

### Lock-Free Data Structures

```rust
use crossbeam::queue::ArrayQueue;
use atomic::{Atomic, Ordering};

pub struct LockFreeCache<T> {
    queue: ArrayQueue<T>,
    size: Atomic<usize>,
}

impl<T> LockFreeCache<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: ArrayQueue::new(capacity),
            size: Atomic::new(0),
        }
    }
    
    pub fn insert(&self, item: T) -> bool {
        if self.queue.push(item).is_ok() {
            self.size.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }
    
    pub fn get(&self) -> Option<T> {
        self.queue.pop().map(|item| {
            self.size.fetch_sub(1, Ordering::SeqCst);
            item
        })
    }
}
```

### Parallel Reduction

```rust
use std::sync::atomic::{AtomicU64, Ordering};

pub struct ParallelAccumulator {
    partials: Vec<AtomicU64>,
    num_threads: usize,
}

impl ParallelAccumulator {
    pub fn new(num_threads: usize) -> Self {
        let partials = (0..num_threads)
            .map(|_| AtomicU64::new(0))
            .collect();
        
        Self {
            partials,
            num_threads,
        }
    }
    
    pub fn add(&self, thread_id: usize, value: u64) {
        self.partials[thread_id].fetch_add(value, Ordering::Relaxed);
    }
    
    pub fn sum(&self) -> u64 {
        self.partials.iter()
            .map(|partial| partial.load(Ordering::Relaxed))
            .sum()
    }
}
```

## GPU Acceleration

### CUDA Integration

```rust
use cuda_sys::*;

pub struct CudaAligner {
    device: i32,
    context: CUcontext,
    module: CUmodule,
}

impl CudaAligner {
    pub fn new(device_id: i32) -> Result<Self> {
        unsafe {
            cuInit(0);
            
            let mut device = 0;
            cuDeviceGet(&mut device, device_id);
            
            let mut context = std::ptr::null_mut();
            cuCtxCreate_v2(&mut context, 0, device);
            
            let mut module = std::ptr::null_mut();
            let ptx = include_str!("../kernels/alignment.ptx");
            cuModuleLoadData(&mut module, ptx.as_ptr() as *const _);
            
            Ok(Self {
                device: device_id,
                context,
                module,
            })
        }
    }
    
    pub fn align_batch(&self, sequences: &[Sequence]) -> Vec<Alignment> {
        // Transfer data to GPU
        let d_sequences = self.upload_sequences(sequences);
        
        // Launch kernel
        let block_size = 256;
        let grid_size = (sequences.len() + block_size - 1) / block_size;
        
        unsafe {
            let mut kernel = std::ptr::null_mut();
            cuModuleGetFunction(&mut kernel, self.module, b"align_kernel\0".as_ptr() as *const _);
            
            cuLaunchKernel(
                kernel,
                grid_size as u32, 1, 1,
                block_size as u32, 1, 1,
                0,
                std::ptr::null_mut(),
                &d_sequences as *const _ as *mut _,
                std::ptr::null_mut(),
            );
        }
        
        // Get results
        self.download_alignments(d_sequences)
    }
}
```

### OpenCL Support

```rust
use ocl::{ProQue, Buffer, Program};

pub struct OpenCLProcessor {
    pro_que: ProQue,
}

impl OpenCLProcessor {
    pub fn new() -> Result<Self> {
        let src = include_str!("../kernels/reduction.cl");
        
        let pro_que = ProQue::builder()
            .src(src)
            .dims(1024)
            .build()?;
        
        Ok(Self { pro_que })
    }
    
    pub fn process_batch(&self, data: &[f32]) -> Result<Vec<f32>> {
        let buffer = Buffer::builder()
            .queue(self.pro_que.queue().clone())
            .flags(ocl::flags::MEM_READ_WRITE)
            .len(data.len())
            .copy_host_slice(data)
            .build()?;
        
        let kernel = self.pro_que.kernel_builder("reduce")
            .arg(&buffer)
            .arg(data.len() as u32)
            .build()?;
        
        unsafe { kernel.enq()? }
        
        let mut result = vec![0.0f32; data.len()];
        buffer.read(&mut result).enq()?;
        
        Ok(result)
    }
}
```

## Configuration

### Thread Pool Configuration

```toml
[parallel.threadpool]
# Thread pool settings
num_threads = 0           # 0 = auto-detect
stack_size_mb = 8        # Stack size per thread
work_stealing = true      # Enable work stealing
yield_strategy = "spin"  # Options: spin, yield, park

# CPU affinity
pin_threads = true
affinity_mode = "compact" # Options: compact, scatter, numa
```

### Parallel Algorithm Settings

```toml
[parallel.algorithms]
# Chunk sizes for parallel processing
chunk_size = 1000
dynamic_chunking = true
min_chunk_size = 100
max_chunk_size = 10000

# Load balancing
load_balancing = "dynamic" # Options: static, dynamic, guided
steal_threshold = 0.5       # Work stealing threshold
```

### GPU Configuration

```toml
[parallel.gpu]
# GPU settings
use_gpu = false
gpu_device = 0
gpu_memory_gb = 8
batch_size = 1024

# CUDA settings
cuda_threads_per_block = 256
cuda_shared_memory_kb = 48
cuda_streams = 4

# OpenCL settings
opencl_platform = 0
opencl_work_group_size = 256
```

## Performance Optimization

### Thread Contention

```rust
use parking_lot::{RwLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct ContentionReducer {
    // Use RwLock for read-heavy workloads
    read_heavy: RwLock<HashMap<String, Vec<u8>>>,
    
    // Use sharded locks for write-heavy workloads
    write_heavy: Vec<Mutex<HashMap<String, Vec<u8>>>>,
    
    // Use atomics for simple flags
    flag: AtomicBool,
}

impl ContentionReducer {
    pub fn read_optimized(&self, key: &str) -> Option<Vec<u8>> {
        self.read_heavy.read().get(key).cloned()
    }
    
    pub fn write_optimized(&self, key: String, value: Vec<u8>) {
        let shard = hash(&key) % self.write_heavy.len();
        self.write_heavy[shard].lock().insert(key, value);
    }
}
```

### False Sharing

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

// Avoid false sharing with padding
#[repr(C, align(64))] // Cache line size
pub struct PaddedCounter {
    count: AtomicUsize,
    _padding: [u8; 56], // 64 - 8 = 56 bytes padding
}

pub struct CounterArray {
    counters: Vec<PaddedCounter>,
}

impl CounterArray {
    pub fn increment(&self, thread_id: usize) {
        self.counters[thread_id].count.fetch_add(1, Ordering::Relaxed);
    }
}
```

## Debugging Parallel Code

### Race Condition Detection

```rust
#[cfg(debug_assertions)]
pub struct DebugLock<T> {
    data: Mutex<T>,
    owner: AtomicUsize,
    access_log: Mutex<Vec<AccessRecord>>,
}

#[cfg(debug_assertions)]
impl<T> DebugLock<T> {
    pub fn lock(&self) -> MutexGuard<T> {
        let thread_id = thread::current().id();
        
        // Log access attempt
        self.access_log.lock().unwrap().push(AccessRecord {
            thread_id,
            timestamp: Instant::now(),
            operation: "lock",
        });
        
        let guard = self.data.lock().unwrap();
        self.owner.store(thread_id.as_u64(), Ordering::SeqCst);
        
        guard
    }
}
```

### Deadlock Detection

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

pub struct DeadlockDetector {
    graph: Arc<Mutex<HashMap<ThreadId, Vec<ThreadId>>>>,
}

impl DeadlockDetector {
    pub fn check_deadlock(&self) -> bool {
        let graph = self.graph.lock().unwrap();
        
        // Perform cycle detection in wait-for graph
        for start in graph.keys() {
            if self.has_cycle(&graph, start, &mut HashSet::new()) {
                return true;
            }
        }
        
        false
    }
    
    fn has_cycle(&self, graph: &HashMap<ThreadId, Vec<ThreadId>>, 
                 node: &ThreadId, visited: &mut HashSet<ThreadId>) -> bool {
        if visited.contains(node) {
            return true;
        }
        
        visited.insert(*node);
        
        if let Some(neighbors) = graph.get(node) {
            for neighbor in neighbors {
                if self.has_cycle(graph, neighbor, visited) {
                    return true;
                }
            }
        }
        
        visited.remove(node);
        false
    }
}
```

## Benchmarking Parallel Code

### Scalability Testing

```rust
use criterion::{black_box, criterion_group, Criterion, BenchmarkId};

fn parallel_scaling_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_scaling");
    
    for num_threads in [1, 2, 4, 8, 16, 32] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            &num_threads,
            |b, &num_threads| {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(num_threads)
                    .build()
                    .unwrap();
                
                b.iter(|| {
                    pool.install(|| {
                        black_box(parallel_workload())
                    })
                });
            },
        );
    }
    
    group.finish();
}
```

### Contention Analysis

```rust
pub struct ContentionMonitor {
    lock_acquisitions: AtomicU64,
    lock_contentions: AtomicU64,
    wait_time_ns: AtomicU64,
}

impl ContentionMonitor {
    pub fn measure_contention<T, F>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        self.lock_acquisitions.fetch_add(1, Ordering::Relaxed);
        
        let result = f();
        
        let wait_time = start.elapsed().as_nanos() as u64;
        if wait_time > 1000 { // More than 1 microsecond
            self.lock_contentions.fetch_add(1, Ordering::Relaxed);
        }
        self.wait_time_ns.fetch_add(wait_time, Ordering::Relaxed);
        
        result
    }
    
    pub fn report(&self) -> ContentionReport {
        ContentionReport {
            total_acquisitions: self.lock_acquisitions.load(Ordering::Relaxed),
            contentions: self.lock_contentions.load(Ordering::Relaxed),
            avg_wait_ns: self.wait_time_ns.load(Ordering::Relaxed) / 
                        self.lock_acquisitions.load(Ordering::Relaxed),
        }
    }
}
```

## Best Practices

1. **Minimize Shared State**: Reduce contention
2. **Use Appropriate Granularity**: Balance overhead vs parallelism
3. **Avoid False Sharing**: Align to cache lines
4. **Profile First**: Measure before optimizing
5. **Consider NUMA**: Optimize for memory locality
6. **Handle Errors**: Graceful degradation in parallel code
7. **Test Thoroughly**: Race conditions are hard to reproduce
8. **Document Assumptions**: Thread safety requirements

## See Also

- [Performance Optimization](performance.md) - General performance tuning
- [Memory Management](memory.md) - Memory considerations for parallel code
- [Configuration](../user-guide/configuration.md) - Parallel processing settings
- [Benchmarks](../benchmarks/performance.md) - Parallel performance metrics