#!/bin/bash
# Visualize Talaria flame trace output
#
# This script converts the folded stack trace format from tracing-flame
# into viewable SVG flamegraphs and flamecharts
#
# Prerequisites:
#   cargo install inferno
#
# Usage:
#   ./scripts/visualize-trace.sh flame-*.folded
#   TALARIA_FLAME=1 talaria reduce input.fasta && ./scripts/visualize-trace.sh flame-*.folded

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if inferno is installed
if ! command -v inferno-flamegraph &> /dev/null; then
    echo -e "${RED}Error: inferno-flamegraph not found${NC}"
    echo ""
    echo "Please install inferno first:"
    echo "  cargo install inferno"
    echo ""
    echo "This provides the tools needed to generate flamegraphs from trace data."
    exit 1
fi

# Check arguments
if [ $# -eq 0 ]; then
    echo -e "${YELLOW}Usage:${NC} $0 <trace.folded>"
    echo ""
    echo "Example workflow:"
    echo "  1. Enable flame tracing: ${GREEN}export TALARIA_FLAME=1${NC}"
    echo "  2. Optional - Set trace level: ${GREEN}export TALARIA_FLAME_LEVEL=trace${NC}"
    echo "     Levels: info (minimal), debug (default), trace (comprehensive)"
    echo "  3. Run talaria command: ${GREEN}talaria reduce input.fasta -o output.fasta${NC}"
    echo "  4. Generate visualization: ${GREEN}$0 flame-*.folded${NC}"
    echo ""
    echo "Available folded files in current directory:"
    ls -la flame-*.folded 2>/dev/null || echo "  (no flame-*.folded files found)"
    exit 1
fi

INPUT_FILE="$1"

# Validate input file
if [ ! -f "$INPUT_FILE" ]; then
    echo -e "${RED}Error: File not found: $INPUT_FILE${NC}"
    exit 1
fi

# Check if file has content
if [ ! -s "$INPUT_FILE" ]; then
    echo -e "${YELLOW}Warning: Input file is empty: $INPUT_FILE${NC}"
    echo "Make sure tracing spans were properly instrumented in the code."
    exit 1
fi

# Generate output filenames
BASENAME="${INPUT_FILE%.folded}"
FLAMEGRAPH_SVG="${BASENAME}-flamegraph.svg"
FLAMECHART_SVG="${BASENAME}-flamechart.svg"

echo "Processing: $INPUT_FILE"
echo ""

# Generate flamegraph (call stack view)
echo -n "Generating flamegraph..."
if cat "$INPUT_FILE" | inferno-flamegraph > "$FLAMEGRAPH_SVG" 2>/dev/null; then
    echo -e " ${GREEN}✓${NC}"
    echo "  Created: $FLAMEGRAPH_SVG"
else
    echo -e " ${RED}✗${NC}"
    echo "  Failed to generate flamegraph"
fi

# Generate flamechart (timeline view)
echo -n "Generating flamechart..."
if cat "$INPUT_FILE" | inferno-flamegraph --flamechart > "$FLAMECHART_SVG" 2>/dev/null; then
    echo -e " ${GREEN}✓${NC}"
    echo "  Created: $FLAMECHART_SVG"
else
    echo -e " ${RED}✗${NC}"
    echo "  Failed to generate flamechart"
fi

# Show file sizes
echo ""
echo "File sizes:"
ls -lh "$INPUT_FILE" "$FLAMEGRAPH_SVG" "$FLAMECHART_SVG" 2>/dev/null | awk '{print "  " $9 ": " $5}'

# Offer to open in browser
echo ""
echo -e "${GREEN}Visualizations generated successfully!${NC}"
echo ""
echo "To view the results:"
echo "  Flamegraph (call stack): firefox $FLAMEGRAPH_SVG"
echo "  Flamechart (timeline):   firefox $FLAMECHART_SVG"
echo ""

# Try to detect available browser
if command -v xdg-open &> /dev/null; then
    read -p "Open flamegraph in browser now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        xdg-open "$FLAMEGRAPH_SVG"
    fi
elif command -v open &> /dev/null; then  # macOS
    read -p "Open flamegraph in browser now? (y/n) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        open "$FLAMEGRAPH_SVG"
    fi
fi