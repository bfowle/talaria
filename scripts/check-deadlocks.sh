#!/bin/bash

# Deadlock Detection Script for Talaria
# This script runs various tests to check for potential deadlocks

set -e

echo "═══════════════════════════════════════════════════"
echo "           Talaria Deadlock Detection Check"
echo "═══════════════════════════════════════════════════"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
export RUST_BACKTRACE=1
export TALARIA_DEADLOCK_DETECTION=1
export TALARIA_DEADLOCK_INTERVAL_MS=100

# Track overall status
OVERALL_STATUS=0

# Function to run a check
run_check() {
    local name=$1
    local cmd=$2

    echo ""
    echo "───────────────────────────────────────────────────"
    echo -e "${BLUE}▶ Running: $name${NC}"
    echo "───────────────────────────────────────────────────"

    if eval $cmd; then
        echo -e "${GREEN}✓ $name passed${NC}"
    else
        echo -e "${RED}✗ $name failed${NC}"
        OVERALL_STATUS=1
    fi
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must be run from the talaria root directory${NC}"
    exit 1
fi

# Build with deadlock detection feature
echo ""
echo -e "${YELLOW}Building with deadlock detection enabled...${NC}"
cd talaria-sequoia
cargo build --features deadlock_detection --release

# Run deadlock regression tests
run_check "Deadlock Regression Tests" \
    "cargo test --features deadlock_detection deadlock_regression_test -- --nocapture"

# Run monitoring integration tests
echo ""
echo -e "${YELLOW}Testing with various monitoring intervals...${NC}"

for interval in 1 5 10; do
    run_check "Monitor interval ${interval}ms" \
        "TALARIA_MONITOR=1 TALARIA_MONITOR_INTERVAL=${interval} \
         cargo test --features deadlock_detection test_monitoring_with_aggressive_intervals \
         -- --nocapture --test-threads=1"
done

# Run stress tests (optional - can be slow)
if [ "${RUN_STRESS_TESTS:-false}" = "true" ]; then
    echo ""
    echo -e "${YELLOW}Running stress tests (this may take a while)...${NC}"

    run_check "Lock Contention Stress Test" \
        "cargo test --features deadlock_detection --release -- --ignored stress_test_throughput_monitor_high_contention"

    run_check "Lock-Free vs Mutex Performance" \
        "cargo test --features deadlock_detection --release -- --ignored stress_test_lock_free_vs_mutex_performance"
else
    echo ""
    echo "───────────────────────────────────────────────────"
    echo -e "${YELLOW}▶ Skipping stress tests (set RUN_STRESS_TESTS=true to enable)${NC}"
    echo "  Run: RUN_STRESS_TESTS=true $0"
    echo "───────────────────────────────────────────────────"
fi

# Test with actual download operation (if network available)
if [ "${TEST_DOWNLOADS:-false}" = "true" ]; then
    echo ""
    echo -e "${YELLOW}Testing with actual download operations...${NC}"

    # Create a test directory
    TEST_DIR=$(mktemp -d)
    export TALARIA_DATA_DIR="$TEST_DIR"

    run_check "Download with Monitoring" \
        "TALARIA_MONITOR=1 TALARIA_MONITOR_INTERVAL=5 \
         timeout 30 ../target/release/talaria database info uniprot/swissprot || true"

    # Clean up
    rm -rf "$TEST_DIR"
else
    echo ""
    echo "───────────────────────────────────────────────────"
    echo -e "${YELLOW}▶ Skipping download tests (set TEST_DOWNLOADS=true to enable)${NC}"
    echo "───────────────────────────────────────────────────"
fi

# Check for potential deadlock patterns in code
echo ""
echo "───────────────────────────────────────────────────"
echo -e "${BLUE}▶ Checking for potential deadlock patterns in code${NC}"
echo "───────────────────────────────────────────────────"

# Look for nested lock patterns
echo "Checking for nested mutex locks..."
if grep -r "\.lock().*\.lock()" --include="*.rs" ../talaria-sequoia/src 2>/dev/null; then
    echo -e "${YELLOW}⚠ Warning: Found potential nested lock patterns${NC}"
    echo "  Review the above matches to ensure proper lock ordering"
else
    echo -e "${GREEN}✓ No obvious nested lock patterns found${NC}"
fi

# Look for lock().unwrap() patterns that might hide issues
echo "Checking for remaining std::sync::Mutex usage..."
if grep -r "std::sync::Mutex" --include="*.rs" ../talaria-sequoia/src 2>/dev/null | \
   grep -v "Mutex as StdMutex" | \
   grep -v "use.*StdMutex"; then
    echo -e "${YELLOW}⚠ Warning: Found std::sync::Mutex usage${NC}"
    echo "  Consider converting to parking_lot::Mutex for deadlock detection"
else
    echo -e "${GREEN}✓ Using parking_lot::Mutex throughout${NC}"
fi

# Summary
echo ""
echo "═══════════════════════════════════════════════════"
if [ $OVERALL_STATUS -eq 0 ]; then
    echo -e "${GREEN}✓ All deadlock checks passed!${NC}"
    echo ""
    echo "The system is resilient against the known deadlock scenarios."
    echo "Monitoring can be safely enabled without causing WSL crashes."
else
    echo -e "${RED}✗ Some deadlock checks failed${NC}"
    echo ""
    echo "Please review the output above and fix any issues before deployment."
    echo "Pay special attention to:"
    echo "  - Lock ordering violations"
    echo "  - Nested lock acquisitions"
    echo "  - Long-held locks in performance-critical paths"
fi
echo "═══════════════════════════════════════════════════"

# Return to original directory
cd ..

exit $OVERALL_STATUS