//! Performance benchmarking module for HERALD

mod benchmark_suite;

// Re-export key benchmarks for external use
pub use benchmark_suite::{
    benchmark_import_speed,
    benchmark_taxonomy_query_latency,
    benchmark_bloom_filter,
    benchmark_update_check,
    benchmark_memory_usage,
    benchmark_bitemporal_queries,
    benchmark_summary,
};