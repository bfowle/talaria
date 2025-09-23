//! Parallel processing utilities

use anyhow::Result;

/// Configure the global thread pool
pub fn configure_thread_pool(threads: usize) -> Result<()> {
    let threads = if threads == 0 {
        num_cpus::get()
    } else {
        threads
    };

    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global()?;

    Ok(())
}

/// Calculate optimal chunk size for parallel processing
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

/// Get the number of available CPU cores
pub fn get_available_cores() -> usize {
    num_cpus::get()
}

/// Check if we should use parallel processing based on item count
pub fn should_parallelize(item_count: usize, threshold: usize) -> bool {
    item_count > threshold && rayon::current_num_threads() > 1
}