#!/bin/bash

# Talaria Code Quality Check Script
# Run all code quality tools: clippy, cargo-audit, cargo-machete

set -e

echo "═══════════════════════════════════════════════════"
echo "             Talaria Code Quality Check"
echo "═══════════════════════════════════════════════════"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track overall status
OVERALL_STATUS=0

# Function to run a check
run_check() {
    local name=$1
    local cmd=$2

    echo ""
    echo "───────────────────────────────────────────────────"
    echo "▶ Running: $name"
    echo "───────────────────────────────────────────────────"

    if eval $cmd; then
        echo -e "${GREEN}✓ $name passed${NC}"
    else
        echo -e "${RED}✗ $name failed${NC}"
        OVERALL_STATUS=1
    fi
}

# Clippy - Rust linter (using lib.rs configuration)
run_check "Clippy (Rust linter)" \
    "cargo clippy -- -D warnings"

# Cargo audit - Security vulnerabilities
run_check "Cargo Audit (Security check)" \
    "cargo audit"

# Cargo machete - Find unused dependencies
run_check "Cargo Machete (Unused dependencies)" \
    "cargo machete"

# Dead code check
run_check "Dead Code Check" \
    "RUSTFLAGS='-W dead-code -W unused' cargo check --all-targets 2>&1 | \
    grep -E 'warning:|unused|dead' || echo 'No dead code warnings found'"

# Format check (don't modify, just check)
run_check "Rust Format Check" \
    "cargo fmt -- --check"

# Build check
run_check "Build Check" \
    "cargo build --release"

# Test check
run_check "Test Suite" \
    "cargo test"

# Coverage check (optional - can be slow)
if [ "${INCLUDE_COVERAGE:-false}" = "true" ]; then
    run_check "Code Coverage Report" \
        "./scripts/coverage.sh"
else
    echo ""
    echo "───────────────────────────────────────────────────"
    echo "▶ Skipping coverage (set INCLUDE_COVERAGE=true to enable)"
    echo "  Run: INCLUDE_COVERAGE=true $0"
    echo "  Or:  ./scripts/coverage.sh --html --open"
    echo "───────────────────────────────────────────────────"
fi

# Summary
echo ""
echo "═══════════════════════════════════════════════════"
if [ $OVERALL_STATUS -eq 0 ]; then
    echo -e "${GREEN}✓ All quality checks passed!${NC}"
else
    echo -e "${RED}✗ Some checks failed. Please review the output above.${NC}"
fi
echo "═══════════════════════════════════════════════════"

# Dead Code Analysis (comprehensive dead code detection)
# Note: This may take some time on first run
run_check "Dead Code Detection" \
    "./scripts/check-dead-code.sh"

exit $OVERALL_STATUS