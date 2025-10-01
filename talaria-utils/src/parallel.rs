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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_get_available_cores() {
        let cores = get_available_cores();
        assert!(cores > 0, "Should detect at least one CPU core");
        assert!(cores <= 256, "Unrealistic number of cores detected");
    }

    #[test]
    fn test_chunk_size_for_parallelism_basic() {
        // Test with various input sizes
        let chunk = chunk_size_for_parallelism(100, 4);
        assert!(chunk >= 10 && chunk <= 1000);

        let chunk = chunk_size_for_parallelism(10000, 4);
        assert!(chunk >= 10 && chunk <= 1000);

        let chunk = chunk_size_for_parallelism(50, 4);
        assert_eq!(chunk, 10); // Should clamp to minimum
    }

    #[test]
    fn test_chunk_size_for_parallelism_auto_threads() {
        // Test with 0 threads (auto-detect)
        let chunk = chunk_size_for_parallelism(1000, 0);
        assert!(chunk >= 10 && chunk <= 1000);
    }

    #[test]
    fn test_chunk_size_for_parallelism_edge_cases() {
        // Very small dataset
        assert_eq!(chunk_size_for_parallelism(5, 4), 10);

        // Very large dataset
        let chunk = chunk_size_for_parallelism(1_000_000, 4);
        assert_eq!(chunk, 1000); // Should clamp to maximum

        // Single thread
        let chunk = chunk_size_for_parallelism(100, 1);
        assert!(chunk >= 10 && chunk <= 1000);
    }

    #[test]
    fn test_should_parallelize() {
        // Should parallelize when above threshold
        assert!(should_parallelize(1000, 100));

        // Should not parallelize when below threshold
        assert!(!should_parallelize(50, 100));

        // Should not parallelize when exactly at threshold
        assert!(!should_parallelize(100, 100));
    }

    #[test]
    #[serial]
    fn test_configure_thread_pool_with_specific_count() {
        // Configure with specific thread count
        let result = configure_thread_pool(4);

        // Note: This might fail if the global thread pool is already initialized
        // In production code, this is typically only called once at startup
        if result.is_ok() {
            assert_eq!(rayon::current_num_threads(), 4);
        }
    }

    #[test]
    #[serial]
    fn test_configure_thread_pool_auto_detect() {
        // Configure with 0 (auto-detect)
        let result = configure_thread_pool(0);

        if result.is_ok() {
            assert_eq!(rayon::current_num_threads(), num_cpus::get());
        }
    }

    #[test]
    fn test_chunk_size_calculation_consistency() {
        // Ensure chunk size calculation is consistent
        let chunk1 = chunk_size_for_parallelism(5000, 8);
        let chunk2 = chunk_size_for_parallelism(5000, 8);
        assert_eq!(chunk1, chunk2, "Same inputs should produce same chunk size");
    }

    #[test]
    fn test_chunk_size_scales_with_threads() {
        // More threads should generally lead to smaller chunks
        let chunk_2_threads = chunk_size_for_parallelism(10000, 2);
        let chunk_8_threads = chunk_size_for_parallelism(10000, 8);

        // With more threads, chunks should be smaller (or hit the min bound)
        assert!(chunk_8_threads <= chunk_2_threads);
    }

    // Property-based test using deterministic pseudo-random
    #[test]
    fn test_chunk_size_bounds() {
        // Test with various pseudo-random inputs
        let mut seed = 42usize;
        for _ in 0..100 {
            // Simple LCG for pseudo-random numbers
            seed = (seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
            let total_items = (seed % 1_000_000) + 1;

            seed = (seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
            let threads = (seed % 128) + 1;

            let chunk = chunk_size_for_parallelism(total_items, threads);
            assert!(
                chunk >= 10 && chunk <= 1000,
                "Chunk size {} out of bounds for {} items with {} threads",
                chunk,
                total_items,
                threads
            );
        }
    }

    #[test]
    fn test_chunk_size_property_exhaustive() {
        // More exhaustive property test
        let test_cases = vec![
            (1, 1),
            (10, 1),
            (100, 1),
            (1000, 1),
            (1, 10),
            (10, 10),
            (100, 10),
            (1000, 10),
            (10000, 1),
            (10000, 4),
            (10000, 8),
            (10000, 16),
            (100000, 1),
            (100000, 8),
            (100000, 32),
            (1000000, 1),
            (1000000, 16),
            (1000000, 64),
        ];

        for (items, threads) in test_cases {
            let chunk = chunk_size_for_parallelism(items, threads);
            assert!(
                chunk >= 10 && chunk <= 1000,
                "Failed for {} items with {} threads: chunk = {}",
                items,
                threads,
                chunk
            );
        }
    }
}
