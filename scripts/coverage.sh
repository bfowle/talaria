#!/bin/bash

# Talaria Code Coverage Script
# Generate test coverage reports using cargo-llvm-cov

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
OUTPUT_FORMAT="terminal"
OPEN_REPORT=false
CRATE=""
CLEAN_FIRST=false
SHOW_UNCOVERED=false

# Function to print usage
usage() {
    echo "Usage: $0 [OPTIONS] [CRATE]"
    echo ""
    echo "Generate test coverage reports for Talaria"
    echo ""
    echo "OPTIONS:"
    echo "  --html          Generate HTML report (default: terminal output)"
    echo "  --lcov          Generate lcov report"
    echo "  --json          Generate JSON report"
    echo "  --open          Open HTML report in browser (implies --html)"
    echo "  --clean         Clean coverage data first"
    echo "  --show-missing  Show uncovered lines"
    echo "  -h, --help      Show this help message"
    echo ""
    echo "CRATE:"
    echo "  Specific crate to test (e.g., talaria-sequoia, talaria-core)"
    echo "  If omitted, runs coverage for entire workspace"
    echo ""
    echo "EXAMPLES:"
    echo "  $0                           # Full workspace coverage to terminal"
    echo "  $0 --html                    # Generate HTML report for workspace"
    echo "  $0 --html --open             # Generate and open HTML report"
    echo "  $0 talaria-sequoia           # Coverage for specific crate"
    echo "  $0 talaria-core --html       # HTML coverage for talaria-core"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --html)
            OUTPUT_FORMAT="html"
            shift
            ;;
        --lcov)
            OUTPUT_FORMAT="lcov"
            shift
            ;;
        --json)
            OUTPUT_FORMAT="json"
            shift
            ;;
        --open)
            OUTPUT_FORMAT="html"
            OPEN_REPORT=true
            shift
            ;;
        --clean)
            CLEAN_FIRST=true
            shift
            ;;
        --show-missing)
            SHOW_UNCOVERED=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        -*)
            echo "Unknown option: $1"
            usage
            ;;
        *)
            CRATE="$1"
            shift
            ;;
    esac
done

echo "═══════════════════════════════════════════════════"
echo "             Talaria Coverage Report"
echo "═══════════════════════════════════════════════════"

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo -e "${YELLOW}cargo-llvm-cov not found. Installing...${NC}"
    cargo install cargo-llvm-cov

    # Also install llvm-tools-preview
    rustup component add llvm-tools-preview
fi

# Clean if requested
if [ "$CLEAN_FIRST" = true ]; then
    echo -e "${BLUE}Cleaning previous coverage data...${NC}"
    cargo llvm-cov clean --workspace
fi

# Build the command
CMD="cargo llvm-cov"

# Add package flag if specific crate requested
if [ -n "$CRATE" ]; then
    echo -e "${BLUE}Running coverage for crate: ${CRATE}${NC}"
    CMD="$CMD --package $CRATE"
else
    echo -e "${BLUE}Running coverage for entire workspace${NC}"
    CMD="$CMD --workspace"
fi

# Add output format
case $OUTPUT_FORMAT in
    html)
        CMD="$CMD --html"
        echo -e "${BLUE}Generating HTML report...${NC}"
        ;;
    lcov)
        CMD="$CMD --lcov --output-path target/coverage.lcov"
        echo -e "${BLUE}Generating LCOV report...${NC}"
        ;;
    json)
        CMD="$CMD --json --output-path target/coverage.json"
        echo -e "${BLUE}Generating JSON report...${NC}"
        ;;
    terminal)
        # Default terminal output
        if [ "$SHOW_UNCOVERED" = true ]; then
            CMD="$CMD --show-missing-lines"
        fi
        ;;
esac

# Add common flags
CMD="$CMD --all-features"

# Run the coverage command
echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Running: $CMD"
echo "───────────────────────────────────────────────────"
echo ""

# Execute coverage
if eval $CMD; then
    echo ""
    echo -e "${GREEN}✓ Coverage report generated successfully${NC}"

    # Handle post-generation actions
    case $OUTPUT_FORMAT in
        html)
            REPORT_DIR="target/llvm-cov/html"
            echo -e "${GREEN}HTML report saved to: $REPORT_DIR/index.html${NC}"

            if [ "$OPEN_REPORT" = true ]; then
                # Try to open in browser
                if command -v xdg-open &> /dev/null; then
                    xdg-open "$REPORT_DIR/index.html"
                elif command -v open &> /dev/null; then
                    open "$REPORT_DIR/index.html"
                else
                    echo -e "${YELLOW}Could not open browser. Please open manually: $REPORT_DIR/index.html${NC}"
                fi
            fi
            ;;
        lcov)
            echo -e "${GREEN}LCOV report saved to: target/coverage.lcov${NC}"
            echo "You can upload this to codecov.io or similar services"
            ;;
        json)
            echo -e "${GREEN}JSON report saved to: target/coverage.json${NC}"
            ;;
    esac

    # Show summary statistics for terminal output
    if [ "$OUTPUT_FORMAT" = "terminal" ]; then
        echo ""
        echo "───────────────────────────────────────────────────"
        echo "Coverage Summary:"
        echo "───────────────────────────────────────────────────"

        # Extract and display summary from the output
        cargo llvm-cov report --summary-only 2>/dev/null || true
    fi
else
    echo -e "${RED}✗ Coverage generation failed${NC}"
    exit 1
fi

echo ""
echo "═══════════════════════════════════════════════════"
echo "Additional Commands:"
echo "───────────────────────────────────────────────────"
echo "• Generate detailed line coverage:"
echo "  $0 --show-missing"
echo ""
echo "• Coverage for specific test:"
echo "  cargo llvm-cov --test <test_name>"
echo ""
echo "• Coverage excluding tests:"
echo "  cargo llvm-cov --lib"
echo ""
echo "• Clean coverage data:"
echo "  cargo llvm-cov clean"
echo "═══════════════════════════════════════════════════"