#!/bin/bash
# Local CI testing using act (https://github.com/nektos/act)
# This script helps test GitHub Actions workflows locally before pushing

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Help message
show_help() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] [JOB]

Test GitHub Actions workflows locally using act.

OPTIONS:
    -l, --list              List all available jobs
    -d, --dry-run          Show what would run without executing
    -v, --verbose          Enable verbose output
    -s, --sequential       Run matrix jobs sequentially (slower but avoids conflicts)
    -w, --workflow FILE    Specify workflow file (e.g., ci.yml)
    -h, --help             Show this help message

JOBS (can specify one or run common presets):
    check                  Run formatting and linting checks
    test                   Run test suite (ubuntu-latest/stable only by default)
    test-full              Run full test matrix (all OS/Rust combos, use with --sequential)
    build                  Run release build
    all                    Run check + test + build (recommended before pushing)

EXAMPLES:
    # List all available jobs
    $(basename "$0") --list

    # Quick test (single OS/Rust combo, recommended for local dev)
    $(basename "$0") test

    # Full test matrix sequentially (all OS/Rust combinations)
    $(basename "$0") test-full --sequential

    # Dry run to see what would execute
    $(basename "$0") --dry-run test

    # Run all critical jobs before pushing (quick mode)
    $(basename "$0") all

    # Run all jobs with full matrix (slower, more thorough)
    $(basename "$0") all --sequential

NOTES:
    - First run will download Docker images (~5.5GB)
    - Subsequent runs are much faster due to caching
    - Default 'test' runs only ubuntu-latest/stable (fast, sufficient for most cases)
    - Use 'test-full --sequential' to test all OS/Rust combinations locally
    - Sequential mode prevents file conflicts when running matrix jobs
    - GitHub CI always runs full matrix on separate runners

EOF
}

# Parse arguments
DRY_RUN=""
VERBOSE=""
WORKFLOW=""
SEQUENTIAL=""
JOB=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -l|--list)
            echo -e "${BLUE}Available GitHub Actions jobs:${NC}"
            act -l
            exit 0
            ;;
        -d|--dry-run)
            DRY_RUN="--dryrun"
            shift
            ;;
        -v|--verbose)
            VERBOSE="--verbose"
            shift
            ;;
        -s|--sequential)
            SEQUENTIAL="true"
            shift
            ;;
        -w|--workflow)
            WORKFLOW="-W .github/workflows/$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            JOB="$1"
            shift
            ;;
    esac
done

# Function to run act with common options
run_act() {
    local job=$1
    local extra_args=$2

    echo -e "${BLUE}→ Running job: ${job}${NC}"

    if [ -n "$DRY_RUN" ]; then
        echo -e "${YELLOW}[DRY RUN MODE]${NC}"
    fi

    # Run act with the specified job
    act $DRY_RUN $VERBOSE $WORKFLOW -j "$job" $extra_args
}

# Function to run test job in quick mode (single matrix combo)
run_test_quick() {
    echo -e "${BLUE}→ Running test (ubuntu-latest/stable only)${NC}"
    echo -e "${YELLOW}  (Use 'test-full --sequential' for all OS/Rust combinations)${NC}"

    if [ -n "$DRY_RUN" ]; then
        echo -e "${YELLOW}[DRY RUN MODE]${NC}"
    fi

    # Run only ubuntu-latest with stable Rust
    act $DRY_RUN $VERBOSE $WORKFLOW -j "test" --matrix os:ubuntu-latest --matrix rust:stable
}

# Function to run test job sequentially through all matrix combinations
run_test_full() {
    echo -e "${BLUE}→ Running test suite (all OS/Rust combinations sequentially)${NC}"

    local combinations=(
        "ubuntu-latest:stable"
        "ubuntu-latest:beta"
        "macos-latest:stable"
        "macos-latest:beta"
    )

    local total=${#combinations[@]}
    local current=0

    for combo in "${combinations[@]}"; do
        current=$((current + 1))
        IFS=':' read -r os rust <<< "$combo"

        echo ""
        echo -e "${BLUE}=== Test Matrix [$current/$total]: $os / $rust ===${NC}"

        if [ -n "$DRY_RUN" ]; then
            echo -e "${YELLOW}[DRY RUN MODE]${NC}"
        fi

        act $DRY_RUN $VERBOSE $WORKFLOW -j "test" --matrix os:$os --matrix rust:$rust

        if [ $? -ne 0 ]; then
            echo -e "${RED}✗ Test failed for $os / $rust${NC}"
            return 1
        fi

        echo -e "${GREEN}✓ Test passed for $os / $rust${NC}"
    done

    echo ""
    echo -e "${GREEN}✓ All test matrix combinations passed!${NC}"
}

# Main execution
case "$JOB" in
    "")
        echo -e "${RED}Error: No job specified${NC}"
        echo "Use --list to see available jobs, or --help for usage"
        exit 1
        ;;

    "all")
        echo -e "${GREEN}Running all critical CI jobs locally...${NC}"
        if [ -n "$SEQUENTIAL" ]; then
            echo -e "${YELLOW}(Sequential mode: Full test matrix)${NC}"
        else
            echo -e "${YELLOW}(Quick mode: Single test matrix combo)${NC}"
        fi
        echo ""

        echo -e "${BLUE}=== Step 1/3: Check (formatting & linting) ===${NC}"
        run_act "check" "$WORKFLOW"
        echo ""

        echo -e "${BLUE}=== Step 2/3: Test Suite ===${NC}"
        if [ -n "$SEQUENTIAL" ]; then
            run_test_full
        else
            run_test_quick
        fi
        echo ""

        echo -e "${BLUE}=== Step 3/3: Build ===${NC}"
        run_act "build" "$WORKFLOW"
        echo ""

        echo -e "${GREEN}✓ All critical CI jobs passed!${NC}"
        echo "Your code is ready to push."
        ;;

    "test")
        if [ -n "$SEQUENTIAL" ]; then
            run_test_full
        else
            run_test_quick
        fi
        ;;

    "test-full")
        if [ -n "$SEQUENTIAL" ]; then
            run_test_full
        else
            echo -e "${YELLOW}Warning: 'test-full' without --sequential will run all matrix jobs in parallel${NC}"
            echo -e "${YELLOW}This may cause file system conflicts. Recommended: use --sequential flag${NC}"
            echo ""
            run_act "test" "$WORKFLOW"
        fi
        ;;

    "check"|"build"|"coverage"|"security"|"unused-deps"|"release")
        run_act "$JOB" "$WORKFLOW"
        ;;

    *)
        # Try to run the specified job (might be a custom job name)
        echo -e "${YELLOW}Warning: '$JOB' is not a recognized preset${NC}"
        echo -e "${YELLOW}Attempting to run as custom job...${NC}"
        run_act "$JOB" "$WORKFLOW"
        ;;
esac
