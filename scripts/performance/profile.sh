#!/bin/bash

# Talaria Performance Profiling Script
#
# Uses perf and flamegraph to profile Talaria's performance
# and identify bottlenecks in the code.

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}        Talaria Performance Profiler${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo

# Check dependencies
check_dependency() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}Error: $1 is not installed${NC}"
        echo "Please install $1 to continue"
        return 1
    fi
    return 0
}

echo -e "${YELLOW}Checking dependencies...${NC}"
DEPS_OK=true
check_dependency "perf" || DEPS_OK=false
check_dependency "cargo" || DEPS_OK=false

if [ "$DEPS_OK" = false ]; then
    echo
    echo "Install missing dependencies:"
    echo "  Ubuntu/Debian: sudo apt-get install linux-tools-common linux-tools-generic"
    echo "  Fedora: sudo dnf install perf"
    echo "  Arch: sudo pacman -S perf"
    exit 1
fi

# Check for flamegraph
if [ ! -d "$HOME/.cargo/bin" ] || [ ! -f "$HOME/.cargo/bin/flamegraph" ]; then
    echo -e "${YELLOW}Installing flamegraph...${NC}"
    cargo install flamegraph
fi

# Parse arguments
COMMAND=""
PROFILE_TIME=30
OUTPUT_DIR="target/profile"
PROFILE_TYPE="cpu"

while [[ $# -gt 0 ]]; do
    case $1 in
        --command)
            COMMAND="$2"
            shift 2
            ;;
        --time)
            PROFILE_TIME="$2"
            shift 2
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --memory)
            PROFILE_TYPE="memory"
            shift
            ;;
        --io)
            PROFILE_TYPE="io"
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --command CMD    Command to profile (default: benchmark)"
            echo "  --time SECONDS   Profile duration (default: 30)"
            echo "  --output DIR     Output directory (default: target/profile)"
            echo "  --memory         Profile memory allocations"
            echo "  --io            Profile I/O operations"
            echo "  --help          Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Create output directory
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Build with release and debug symbols
echo -e "${YELLOW}Building Talaria with debug symbols...${NC}"
RUSTFLAGS="-C debuginfo=2" cargo build --release

# Set default command if not provided
if [ -z "$COMMAND" ]; then
    # Create a test file for profiling
    TEST_FILE="/tmp/talaria_profile_test.fasta"
    echo -e "${YELLOW}Generating test data...${NC}"
    cat > "$TEST_FILE" <<EOF
>seq_001 Test sequence 1
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
TGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCA
>seq_002 Test sequence 2
GGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCCGGCC
TTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAATTAA
EOF

    # Repeat the sequences to make a larger file
    for i in {1..10000}; do
        cat >> "$TEST_FILE" <<EOF
>seq_${i} Test sequence ${i}
ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT
TGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCATGCA
EOF
    done

    COMMAND="./target/release/talaria reduce -i $TEST_FILE -o /tmp/talaria_profile_out.fasta"
fi

echo -e "${YELLOW}Command to profile:${NC} $COMMAND"
echo

# Run profiling based on type
case $PROFILE_TYPE in
    cpu)
        echo -e "${YELLOW}Running CPU profiling for ${PROFILE_TIME} seconds...${NC}"

        # Record with perf
        PERF_FILE="$OUTPUT_DIR/perf_${TIMESTAMP}.data"
        sudo perf record -F 99 -g -o "$PERF_FILE" -- timeout $PROFILE_TIME $COMMAND || true

        # Generate flame graph
        echo -e "${YELLOW}Generating flame graph...${NC}"
        FLAMEGRAPH_FILE="$OUTPUT_DIR/flamegraph_${TIMESTAMP}.svg"
        sudo perf script -i "$PERF_FILE" | \
            $HOME/.cargo/bin/flamegraph --perfdata "$PERF_FILE" > "$FLAMEGRAPH_FILE" 2>/dev/null || \
            sudo perf script -i "$PERF_FILE" | \
            perl -ne 'print if /^\s*[\da-f]+\s+[\da-f]+/' | \
            $HOME/.cargo/bin/inferno-collapse-perf | \
            $HOME/.cargo/bin/inferno-flamegraph > "$FLAMEGRAPH_FILE"

        echo -e "${GREEN}✓ Flame graph saved to: $FLAMEGRAPH_FILE${NC}"

        # Generate perf report
        echo -e "${YELLOW}Generating perf report...${NC}"
        REPORT_FILE="$OUTPUT_DIR/report_${TIMESTAMP}.txt"
        sudo perf report -i "$PERF_FILE" --stdio > "$REPORT_FILE"

        # Show top functions
        echo
        echo -e "${BLUE}Top 10 CPU-consuming functions:${NC}"
        head -20 "$REPORT_FILE" | grep -A 10 "Overhead"
        ;;

    memory)
        echo -e "${YELLOW}Running memory profiling...${NC}"

        # Use valgrind for memory profiling
        if command -v valgrind &> /dev/null; then
            MASSIF_FILE="$OUTPUT_DIR/massif_${TIMESTAMP}.out"
            valgrind --tool=massif --massif-out-file="$MASSIF_FILE" \
                     --time-unit=B --pages-as-heap=yes \
                     timeout $PROFILE_TIME $COMMAND || true

            # Generate memory usage report
            if command -v ms_print &> /dev/null; then
                ms_print "$MASSIF_FILE" > "$OUTPUT_DIR/memory_${TIMESTAMP}.txt"
                echo -e "${GREEN}✓ Memory profile saved to: $OUTPUT_DIR/memory_${TIMESTAMP}.txt${NC}"
            fi
        else
            echo -e "${RED}Valgrind not installed. Using basic memory tracking...${NC}"

            # Basic memory tracking with /usr/bin/time
            /usr/bin/time -v timeout $PROFILE_TIME $COMMAND 2>&1 | \
                tee "$OUTPUT_DIR/memory_basic_${TIMESTAMP}.txt"
        fi
        ;;

    io)
        echo -e "${YELLOW}Running I/O profiling...${NC}"

        # Use iostat for I/O monitoring
        if command -v iostat &> /dev/null; then
            iostat -x 1 $PROFILE_TIME > "$OUTPUT_DIR/iostat_${TIMESTAMP}.txt" &
            IOSTAT_PID=$!
        fi

        # Use strace for syscall analysis
        if command -v strace &> /dev/null; then
            STRACE_FILE="$OUTPUT_DIR/strace_${TIMESTAMP}.txt"
            strace -c -o "$STRACE_FILE" timeout $PROFILE_TIME $COMMAND || true

            echo
            echo -e "${BLUE}System call summary:${NC}"
            head -20 "$STRACE_FILE"
        fi

        # Stop iostat
        if [ ! -z "$IOSTAT_PID" ]; then
            kill $IOSTAT_PID 2>/dev/null || true
        fi
        ;;
esac

# Analyze results
echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}                    Profile Analysis${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"

# Identify bottlenecks
echo
echo -e "${YELLOW}Potential bottlenecks:${NC}"

if [ "$PROFILE_TYPE" == "cpu" ] && [ -f "$REPORT_FILE" ]; then
    # Check for hot functions
    HOT_FUNCTIONS=$(grep -E "^\s+[0-9]+\.[0-9]+%" "$REPORT_FILE" | head -5)
    if [ ! -z "$HOT_FUNCTIONS" ]; then
        echo "Hot functions (>5% CPU):"
        echo "$HOT_FUNCTIONS"
    fi

    # Check for lock contention
    if grep -q "mutex\|lock\|spin" "$REPORT_FILE"; then
        echo -e "${YELLOW}⚠ Lock contention detected${NC}"
    fi

    # Check for allocator overhead
    if grep -q "malloc\|free\|alloc" "$REPORT_FILE"; then
        echo -e "${YELLOW}⚠ High allocation overhead detected${NC}"
    fi
fi

# Optimization recommendations
echo
echo -e "${GREEN}Optimization recommendations:${NC}"

# Check profile results and suggest optimizations
if [ "$PROFILE_TYPE" == "cpu" ] && [ -f "$REPORT_FILE" ]; then
    if grep -q "hash\|sha256" "$REPORT_FILE"; then
        echo "  • Consider caching hash computations"
    fi
    if grep -q "parse\|read" "$REPORT_FILE"; then
        echo "  • Consider parallel FASTA parsing"
    fi
    if grep -q "compress\|decompress" "$REPORT_FILE"; then
        echo "  • Consider using faster compression algorithm"
    fi
fi

echo
echo -e "${GREEN}✓ Profiling complete!${NC}"
echo "  Results saved to: $OUTPUT_DIR"

# Open flamegraph if available
if [ "$PROFILE_TYPE" == "cpu" ] && [ -f "$FLAMEGRAPH_FILE" ]; then
    echo
    echo -e "${YELLOW}Opening flame graph in browser...${NC}"
    if command -v xdg-open &> /dev/null; then
        xdg-open "$FLAMEGRAPH_FILE"
    elif command -v open &> /dev/null; then
        open "$FLAMEGRAPH_FILE"
    else
        echo "View flame graph at: $FLAMEGRAPH_FILE"
    fi
fi

# Cleanup
rm -f "$TEST_FILE" /tmp/talaria_profile_out.fasta 2>/dev/null || true

echo -e "${BLUE}═══════════════════════════════════════════════════════════════════${NC}"