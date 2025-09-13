/// Parallel processing utilities

pub fn configure_thread_pool(threads: usize) -> Result<(), rayon::ThreadPoolBuildError> {
    let threads = if threads == 0 {
        num_cpus::get()
    } else {
        threads
    };
    
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()
}

pub fn chunk_size_for_parallelism(total_items: usize, threads: usize) -> usize {
    let threads = if threads == 0 {
        rayon::current_num_threads()
    } else {
        threads
    };
    
    // Aim for at least 10 items per thread, but not more than 1000 per chunk
    let ideal_chunk = total_items / (threads * 10);
    ideal_chunk.clamp(10, 1000)
}