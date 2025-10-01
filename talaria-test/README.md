# Talaria Test Module

## Overview

The `talaria-test` module provides comprehensive testing utilities and fixtures for the Talaria bioinformatics system. It offers a standardized testing framework with isolated environments, mock data generators, and test helpers that ensure reliable and reproducible testing across all Talaria components.

### Key Features

- **Test Environments**: Isolated temporary environments for each test
- **Fixture Generation**: Mock biological sequences and database structures
- **Test Storage**: In-memory and temporary storage backends for testing
- **Assertion Helpers**: Specialized assertions for biological data
- **Performance Testing**: Benchmarking utilities for sequence operations
- **Integration Testing**: End-to-end test scenarios
- **Mock External Tools**: Test doubles for external dependencies

## Architecture

### Module Organization

```
talaria-test/
├── src/
│   ├── environment.rs    # Test environment management
│   ├── fixtures/         # Test data generators
│   │   ├── sequences.rs  # Mock sequence generation
│   │   ├── taxonomy.rs   # Taxonomy test data
│   │   └── databases.rs  # Database fixtures
│   ├── storage.rs        # Test storage implementations
│   ├── assertions.rs     # Custom assertion helpers
│   ├── mocks/           # Mock implementations
│   └── lib.rs           # Module exports
```

## Usage

### Test Environment

The `TestEnvironment` provides isolated temporary directories and configuration:

```rust
use talaria_test::TestEnvironment;

#[test]
fn test_with_isolated_environment() {
    let env = TestEnvironment::new().unwrap();

    // Get isolated directories
    let db_dir = env.databases_dir();
    let seq_dir = env.sequences_dir();

    // Write test files
    env.write_file("test.fasta", b">seq1\nACGT").unwrap();

    // Environment is automatically cleaned up on drop
}
```

### Sequence Fixtures

Generate mock biological sequences for testing:

```rust
use talaria_test::fixtures::{
    generate_random_sequence,
    generate_sequences_with_taxonomy,
    create_fasta_content
};

#[test]
fn test_sequence_processing() {
    // Generate random DNA sequence
    let dna = generate_random_sequence(1000, SequenceType::DNA);

    // Generate protein sequence
    let protein = generate_random_sequence(300, SequenceType::Protein);

    // Generate sequences with taxonomy
    let sequences = generate_sequences_with_taxonomy(
        100,  // count
        TaxonId(9606),  // Human
        500..1500  // length range
    );

    // Create FASTA content
    let fasta = create_fasta_content(&sequences);
}
```

### Storage Fixtures

Test storage backends with predictable behavior:

```rust
use talaria_test::{TestStorage, StorageFixture};

#[test]
fn test_storage_operations() {
    let env = TestEnvironment::new().unwrap();

    // Create test storage
    let mut storage = TestStorage::new(&env).unwrap();

    // Store test sequences
    let hash = storage.store_sequence("ACGT", ">test").unwrap();

    // Verify storage
    assert!(storage.contains(&hash));

    // Use pre-populated fixture
    let fixture = StorageFixture::with_bacterial_sequences(&env).unwrap();
    assert_eq!(fixture.sequences().len(), 3);
}
```

### Taxonomy Fixtures

Test data for taxonomic operations:

```rust
use talaria_test::fixtures::{
    create_mock_taxonomy,
    create_taxonomic_lineage,
    TaxonomyFixture
};

#[test]
fn test_taxonomy_operations() {
    // Create mock taxonomy tree
    let taxonomy = create_mock_taxonomy();

    // Get lineage for E. coli
    let lineage = create_taxonomic_lineage(TaxonId(562));
    assert_eq!(lineage.len(), 7);  // Domain to species

    // Use complete fixture
    let fixture = TaxonomyFixture::ncbi_subset();
    let node = fixture.get_node(TaxonId(9606)).unwrap();
    assert_eq!(node.scientific_name, "Homo sapiens");
}
```

### Custom Assertions

Specialized assertions for biological data:

```rust
use talaria_test::assertions::*;

#[test]
fn test_sequence_properties() {
    let seq = "ACGTACGTNN";

    // Assert sequence properties
    assert_valid_dna(seq);
    assert_sequence_length(seq, 10);
    assert_gc_content(seq, 0.4..0.6);

    // Assert FASTA format
    let fasta = ">seq1\nACGT\n>seq2\nTGCA";
    assert_valid_fasta(fasta);
    assert_fasta_count(fasta, 2);
}
```

### Performance Testing

Utilities for benchmarking:

```rust
use talaria_test::performance::{measure_throughput, PerformanceTimer};

#[test]
fn test_performance() {
    let timer = PerformanceTimer::start();

    // Perform operations
    for _ in 0..1000 {
        process_sequence();
    }

    let elapsed = timer.elapsed();
    assert!(elapsed.as_secs() < 5);

    // Measure throughput
    let throughput = measure_throughput(|| {
        process_sequence()
    }, 1000);

    println!("Throughput: {} ops/sec", throughput);
}
```

### Mock External Tools

Test doubles for external dependencies:

```rust
use talaria_test::mocks::{MockAligner, MockDatabase};

#[test]
fn test_with_mock_aligner() {
    let mut aligner = MockAligner::new();

    // Configure expected behavior
    aligner.expect_align()
        .with("ACGT", "ACGA")
        .returns(AlignmentResult {
            score: 0.95,
            // ...
        });

    // Use in test
    let result = aligner.align("ACGT", "ACGA");
    assert_eq!(result.score, 0.95);
}
```

## Test Organization

### Unit Tests

Place unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use talaria_test::*;

    #[test]
    fn test_function() {
        let env = TestEnvironment::new().unwrap();
        // Test implementation
    }
}
```

### Integration Tests

Create integration tests in `tests/` directory:

```rust
// tests/integration_test.rs
use talaria_test::*;

#[test]
fn test_full_workflow() {
    let env = TestEnvironment::new().unwrap();
    let fixture = DatabaseFixture::uniprot_sample(&env);

    // Test complete workflow
}
```

### Benchmark Tests

Create benchmarks in `benches/` directory:

```rust
// benches/performance.rs
use criterion::{criterion_group, criterion_main, Criterion};
use talaria_test::fixtures::*;

fn benchmark_sequence_processing(c: &mut Criterion) {
    let sequences = generate_random_sequences(1000);

    c.bench_function("process_sequences", |b| {
        b.iter(|| process_sequences(&sequences))
    });
}

criterion_group!(benches, benchmark_sequence_processing);
criterion_main!(benches);
```

## Best Practices

1. **Use TestEnvironment**: Always use `TestEnvironment` for isolated testing
2. **Clean Resources**: Ensure proper cleanup with RAII patterns
3. **Predictable Data**: Use fixtures for consistent test data
4. **Test Isolation**: Each test should be independent
5. **Performance Bounds**: Set reasonable performance expectations
6. **Mock External Dependencies**: Don't rely on external services in tests

## Common Patterns

### Testing with Temporary Files

```rust
#[test]
fn test_file_operations() {
    let env = TestEnvironment::new().unwrap();
    let file = env.temp_file("test.txt");

    std::fs::write(&file, b"content").unwrap();

    // File is automatically cleaned up
}
```

### Testing Error Conditions

```rust
#[test]
fn test_error_handling() {
    let env = TestEnvironment::new().unwrap();
    let storage = TestStorage::new(&env).unwrap();

    // Test missing data
    let result = storage.get(&SHA256Hash::zero());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), ErrorKind::NotFound);
}
```

### Testing Async Code

```rust
#[tokio::test]
async fn test_async_operation() {
    let env = TestEnvironment::new().unwrap();

    let result = async_operation().await;
    assert!(result.is_ok());
}
```

## Dependencies

- `tempfile`: Temporary directory management
- `rand`: Random data generation
- `proptest`: Property-based testing
- `criterion`: Benchmarking
- `mockall`: Mock object generation
- `serial_test`: Serialized test execution

## Contributing

When adding test utilities:
1. Ensure they're generic and reusable
2. Document usage with examples
3. Add tests for the test utilities themselves
4. Consider performance implications
5. Maintain backwards compatibility

## License

Part of the Talaria project. See the main repository for license information.