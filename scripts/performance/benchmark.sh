#!/bin/bash

# Talaria Performance Benchmark Suite
#
# This script runs comprehensive performance benchmarks for the download
# and chunking pipeline, comparing against baseline expectations.

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}        Talaria Performance Benchmark Suite${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must run from Talaria project root${NC}"
    exit 1
fi

# Parse command line arguments
PROFILE="release"
FILTER=""
SAVE_BASELINE=false
COMPARE_BASELINE=false
OUTPUT_FORMAT="pretty"

while [[ $# -gt 0 ]]; do
    case $1 in
        --debug)
            PROFILE="debug"
            shift
            ;;
        --filter)
            FILTER="$2"
            shift 2
            ;;
        --save-baseline)
            SAVE_BASELINE=true
            shift
            ;;
        --compare-baseline)
            COMPARE_BASELINE=true
            shift
            ;;
        --json)
            OUTPUT_FORMAT="json"
            shift
            ;;
        --csv)
            OUTPUT_FORMAT="csv"
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --debug          Run with debug build (slower)"
            echo "  --filter REGEX   Only run benchmarks matching regex"
            echo "  --save-baseline  Save results as baseline"
            echo "  --compare-baseline Compare against saved baseline"
            echo "  --json          Output results as JSON"
            echo "  --csv           Output results as CSV"
            echo "  --help          Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# System information
echo -e "${YELLOW}System Information:${NC}"
echo "  CPU Cores: $(nproc)"
echo "  Memory: $(free -h | awk '/^Mem:/ {print $2}')"
echo "  Storage: $(df -h . | awk 'NR==2 {print $4 " available"}')"
echo "  Rust: $(rustc --version)"
echo "  Profile: $PROFILE"
echo

# Build if necessary
echo -e "${YELLOW}Building Talaria ($PROFILE mode)...${NC}"
if [ "$PROFILE" == "release" ]; then
    cargo build --release --quiet
else
    cargo build --quiet
fi

# Set up results directory
RESULTS_DIR="target/benchmark-results"
mkdir -p "$RESULTS_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULT_FILE="$RESULTS_DIR/benchmark_${TIMESTAMP}.txt"

# Run herald benchmarks
echo -e "${YELLOW}Running HERALD benchmarks...${NC}"
echo
cargo bench -p talaria-herald --bench herald_benchmarks -- ${FILTER} --noplot | tee "$RESULT_FILE"

# Run download/chunking benchmarks
echo
echo -e "${YELLOW}Running download/chunking benchmarks...${NC}"
echo
cargo bench -p talaria-herald --bench download_chunking_bench -- ${FILTER} --noplot | tee -a "$RESULT_FILE"

# Run integration performance tests
echo
echo -e "${YELLOW}Running performance regression tests...${NC}"
echo
cargo test -p talaria-herald --test performance_test --release -- --nocapture | tee -a "$RESULT_FILE"

# Analyze results
echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                    Performance Analysis${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo

# Extract key metrics
FASTA_THROUGHPUT=$(grep -oP 'fasta_reading/\d+seqs.*?time:.*?\[[\d.]+ [mnu]s ([\d.]+ [mnu]s) [\d.]+ [mnu]s\]' "$RESULT_FILE" | head -1 || echo "N/A")
CHUNKING_THROUGHPUT=$(grep -oP 'chunking_throughput/\d+seqs.*?time:.*?\[[\d.]+ [mnu]s ([\d.]+ [mnu]s) [\d.]+ [mnu]s\]' "$RESULT_FILE" | head -1 || echo "N/A")
DEDUP_THROUGHPUT=$(grep -oP 'deduplication_perf/\d+dup.*?time:.*?\[[\d.]+ [mnu]s ([\d.]+ [mnu]s) [\d.]+ [mnu]s\]' "$RESULT_FILE" | head -1 || echo "N/A")

# Check for performance regression test results
THROUGHPUT_TEST=$(grep -oP 'Throughput: [\d.]+ sequences/second' "$RESULT_FILE" | head -1 || echo "N/A")
MEMORY_TEST=$(grep -oP 'Memory used: [\d.]+ MB' "$RESULT_FILE" | head -1 || echo "N/A")

echo -e "${GREEN}Key Performance Metrics:${NC}"
echo "  FASTA Reading: $FASTA_THROUGHPUT"
echo "  Chunking: $CHUNKING_THROUGHPUT"
echo "  Deduplication: $DEDUP_THROUGHPUT"
echo "  Throughput Test: $THROUGHPUT_TEST"
echo "  Memory Usage: $MEMORY_TEST"
echo

# Check against baseline if requested
if [ "$COMPARE_BASELINE" == true ]; then
    BASELINE_FILE="$RESULTS_DIR/baseline.txt"
    if [ -f "$BASELINE_FILE" ]; then
        echo -e "${YELLOW}Comparing against baseline...${NC}"

        # Simple comparison - in production, use proper statistical analysis
        diff -u "$BASELINE_FILE" "$RESULT_FILE" | head -20 || true

        echo
        echo -e "${GREEN}Comparison complete${NC}"
    else
        echo -e "${RED}No baseline found at $BASELINE_FILE${NC}"
    fi
fi

# Save as baseline if requested
if [ "$SAVE_BASELINE" == true ]; then
    cp "$RESULT_FILE" "$RESULTS_DIR/baseline.txt"
    echo -e "${GREEN}Saved results as baseline${NC}"
fi

# Generate report in requested format
case $OUTPUT_FORMAT in
    json)
        echo
        echo -e "${YELLOW}Generating JSON report...${NC}"
        cat > "$RESULTS_DIR/report_${TIMESTAMP}.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "profile": "$PROFILE",
  "system": {
    "cpu_cores": $(nproc),
    "memory_gb": $(free -g | awk '/^Mem:/ {print $2}')
  },
  "results": {
    "fasta_throughput": "$FASTA_THROUGHPUT",
    "chunking_throughput": "$CHUNKING_THROUGHPUT",
    "dedup_throughput": "$DEDUP_THROUGHPUT"
  }
}
EOF
        echo -e "${GREEN}JSON report saved to $RESULTS_DIR/report_${TIMESTAMP}.json${NC}"
        ;;
    csv)
        echo
        echo -e "${YELLOW}Generating CSV report...${NC}"
        echo "timestamp,profile,cpu_cores,memory_gb,fasta_throughput,chunking_throughput,dedup_throughput" > "$RESULTS_DIR/report_${TIMESTAMP}.csv"
        echo "$TIMESTAMP,$PROFILE,$(nproc),$(free -g | awk '/^Mem:/ {print $2}'),$FASTA_THROUGHPUT,$CHUNKING_THROUGHPUT,$DEDUP_THROUGHPUT" >> "$RESULTS_DIR/report_${TIMESTAMP}.csv"
        echo -e "${GREEN}CSV report saved to $RESULTS_DIR/report_${TIMESTAMP}.csv${NC}"
        ;;
esac

# Summary
echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✓ Benchmark complete!${NC}"
echo "  Results saved to: $RESULT_FILE"

# Check for performance issues
if grep -q "below minimum" "$RESULT_FILE"; then
    echo -e "${RED}⚠ Performance issues detected - review results${NC}"
    exit 1
else
    echo -e "${GREEN}✓ All performance tests passed${NC}"
fi

echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"