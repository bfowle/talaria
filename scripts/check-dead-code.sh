#!/bin/bash

# Talaria Dead Code Detection Script
# Comprehensive analysis to find unused code, files, and modules

set -e

echo "═══════════════════════════════════════════════════"
echo "             Talaria Dead Code Analysis"
echo "═══════════════════════════════════════════════════"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create temp directory for analysis
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Running Compiler Dead Code Detection"
echo "───────────────────────────────────────────────────"

# Run with strict dead code warnings (skip benchmarks if they have issues)
RUSTFLAGS="-W dead_code -W unused -W unreachable_code" cargo check --lib --bins --tests 2>&1 > $TEMP_DIR/dead_code.txt || true

# Count warnings safely
DEAD_CODE_COUNT=0
if [ -f "$TEMP_DIR/dead_code.txt" ]; then
    DEAD_CODE_COUNT=$(grep "warning:" "$TEMP_DIR/dead_code.txt" | wc -l | tr -d ' ')

    # Show first few warnings if any exist
    if [ "$DEAD_CODE_COUNT" -gt 0 ]; then
        echo -e "${YELLOW}Found $DEAD_CODE_COUNT dead code warnings:${NC}"
        grep -A2 "warning:" "$TEMP_DIR/dead_code.txt" | head -20
    else
        echo -e "${GREEN}✓ No dead code warnings from compiler${NC}"
    fi
else
    echo -e "${GREEN}✓ No dead code warnings from compiler${NC}"
fi

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Finding Unused Public APIs"
echo "───────────────────────────────────────────────────"

# Check for unused dependencies using cargo-machete (fast)
echo "Checking for unused dependencies..."
if command -v cargo-machete &> /dev/null; then
    cargo machete 2>&1 | tee $TEMP_DIR/machete.txt || true
    UNUSED_DEPS=$(grep -c "unused" $TEMP_DIR/machete.txt 2>/dev/null || echo "0")
    if [ "$UNUSED_DEPS" -gt 0 ]; then
        echo -e "${YELLOW}Found unused dependencies (see above)${NC}"
    else
        echo -e "${GREEN}✓ No unused dependencies found${NC}"
    fi
else
    echo -e "${YELLOW}cargo-machete not found${NC}"
    echo "  Install with: cargo install cargo-machete"
fi

echo ""

# Check for unreachable public items (faster version)
echo "Checking for unreachable public items..."
RUSTFLAGS="-W unreachable-pub" cargo check --lib --message-format=short 2>&1 > $TEMP_DIR/unreachable.txt &
CHECK_PID=$!

# Wait up to 10 seconds for check to complete
for i in {1..10}; do
    if ! ps -p $CHECK_PID > /dev/null; then
        break
    fi
    sleep 1
done

# Kill if still running
if ps -p $CHECK_PID > /dev/null; then
    kill $CHECK_PID 2>/dev/null || true
    echo -e "${YELLOW}Check timed out - skipping unreachable pub analysis${NC}"
else
    UNREACHABLE_COUNT=$(grep -c "warning: unreachable" $TEMP_DIR/unreachable.txt 2>/dev/null || echo "0")
    if [ "$UNREACHABLE_COUNT" -gt 0 ]; then
        echo -e "${YELLOW}Found $UNREACHABLE_COUNT unreachable public items:${NC}"
        grep "warning: unreachable" $TEMP_DIR/unreachable.txt | head -5 || true
    else
        echo -e "${GREEN}✓ No unreachable public items detected${NC}"
    fi
fi

# Check if nightly is available for cargo-udeps
if rustup toolchain list | grep -q nightly; then
    if command -v cargo-udeps &> /dev/null || cargo +nightly udeps --help &> /dev/null 2>&1; then
        echo ""
        echo "Running cargo-udeps for precise dependency analysis..."
        cargo +nightly udeps --all-targets 2>&1 | head -20 || true
    fi
fi

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Analyzing Module Usage"
echo "───────────────────────────────────────────────────"

# Find potentially unused modules
echo "Checking module imports..."
for module in src/*/; do
    if [ -d "$module" ]; then
        module_name=$(basename "$module")
        # Skip main modules
        if [[ "$module_name" != "cli" && "$module_name" != "main" && "$module_name" != "bin" ]]; then
            usage_count=$(grep -r "use.*${module_name}::" src --include="*.rs" 2>/dev/null | grep -v "^${module}" | wc -l)
            if [ $usage_count -eq 0 ]; then
                echo -e "${RED}✗ Module '$module_name' appears unused${NC}"
            elif [ $usage_count -lt 3 ]; then
                echo -e "${YELLOW}⚠ Module '$module_name' has limited usage ($usage_count imports)${NC}"
            fi
        fi
    fi
done

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Finding Orphaned Files"
echo "───────────────────────────────────────────────────"

# Find .rs files not referenced in any mod.rs or via #[path]
echo "Checking for orphaned files..."
find src -name "*.rs" -type f | while read -r file; do
    filename=$(basename "$file" .rs)
    dirname=$(dirname "$file")

    # Skip mod.rs, lib.rs, main.rs
    if [[ "$filename" != "mod" && "$filename" != "lib" && "$filename" != "main" ]]; then
        # Check if this file is referenced in its parent mod.rs
        parent_mod="$dirname/mod.rs"
        is_referenced=false

        if [ -f "$parent_mod" ]; then
            # Check for standard mod declaration
            if grep -q "mod $filename" "$parent_mod" 2>/dev/null; then
                is_referenced=true
            # Check for pub use or reexport
            elif grep -q "pub.*$filename" "$parent_mod" 2>/dev/null; then
                is_referenced=true
            fi
        fi

        # Also check for #[path] attribute references anywhere in the codebase
        if [ "$is_referenced" = false ]; then
            if grep -r "#\[path.*\"$filename.rs\"\]" src --include="*.rs" 2>/dev/null | grep -q .; then
                is_referenced=true
            fi
        fi

        if [ "$is_referenced" = false ]; then
            echo -e "${YELLOW}⚠ File may be orphaned: $file${NC}"
        fi
    fi
done

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Analyzing Test Coverage"
echo "───────────────────────────────────────────────────"

# Find modules without tests
echo "Checking for modules without tests..."
for src_file in $(find src -name "*.rs" -type f | grep -v test); do
    if ! grep -q "#\[cfg(test)\]" "$src_file" 2>/dev/null; then
        # Check if there's a corresponding test file
        test_file="${src_file%.rs}_test.rs"
        if [ ! -f "$test_file" ]; then
            filename=$(basename "$src_file")
            if [[ "$filename" != "mod.rs" && "$filename" != "lib.rs" && "$filename" != "main.rs" ]]; then
                echo -e "${YELLOW}No tests found for: $src_file${NC}"
            fi
        fi
    fi
done | head -10

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Finding Duplicate/Similar Code"
echo "───────────────────────────────────────────────────"

# Find files with same names that might be duplicates
echo "Checking for duplicate file names..."
find src -name "*.rs" -type f | xargs -I {} basename {} | sort | uniq -d | while read -r dup; do
    echo -e "${YELLOW}Duplicate filename '$dup' found in:${NC}"
    find src -name "$dup" -type f | while read -r path; do
        echo "  - $path"
    done
done

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Struct/Enum Usage Analysis"
echo "───────────────────────────────────────────────────"

# Find potentially unused public structs/enums
echo "Analyzing public type usage..."
grep -r "^pub struct\|^pub enum" src --include="*.rs" | head -20 | while IFS=: read -r file line; do
    # Extract the type name
    type_name=$(echo "$line" | sed -E 's/pub (struct|enum) ([A-Za-z0-9_]+).*/\2/')

    # Count usage (excluding the definition file)
    usage_count=$(grep -r "\b$type_name\b" src --include="*.rs" | grep -v "^$file:" | wc -l)

    if [ $usage_count -eq 0 ]; then
        echo -e "${RED}✗ Type '$type_name' defined in $file appears unused${NC}"
    elif [ $usage_count -lt 2 ]; then
        echo -e "${YELLOW}⚠ Type '$type_name' has minimal usage ($usage_count references)${NC}"
    fi
done

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Feature Flag Analysis"
echo "───────────────────────────────────────────────────"

# Check for unused feature flags
if grep -q "^\[features\]" Cargo.toml; then
    echo "Checking feature flag usage..."

    # Use cargo-unused-features if available
    if command -v unused-features &> /dev/null; then
        echo "Running cargo-unused-features analysis..."
        unused-features analyze 2>&1 | head -20 || true
        unused-features prune --dry-run 2>&1 | head -20 || true
    else
        # Fallback to manual checking
        grep -A 20 "^\[features\]" Cargo.toml | grep "^[a-z]" | cut -d= -f1 | tr -d ' ' | while read -r feature; do
            usage_count=$(grep -r "cfg.*feature.*$feature" src --include="*.rs" | wc -l)
            if [ $usage_count -eq 0 ]; then
                echo -e "${YELLOW}⚠ Feature flag '$feature' appears unused${NC}"
            fi
        done
    fi
else
    echo "No feature flags defined"
fi

echo ""
echo "───────────────────────────────────────────────────"
echo "▶ Summary Report"
echo "───────────────────────────────────────────────────"

# Generate summary
TOTAL_FILES=$(find src -name "*.rs" -type f | wc -l)
TOTAL_LINES=$(find src -name "*.rs" -type f -exec wc -l {} + | tail -1 | awk '{print $1}')
PUB_ITEMS=$(grep -r "^pub " src --include="*.rs" | wc -l)

echo "Codebase Statistics:"
echo "  Total Rust files: $TOTAL_FILES"
echo "  Total lines of code: $TOTAL_LINES"
echo "  Public items: $PUB_ITEMS"

if [ "$DEAD_CODE_COUNT" -gt 0 ]; then
    echo ""
    echo -e "${YELLOW}⚠ Found potential dead code issues${NC}"
    echo "  Run 'cargo clippy' and check warnings for details"
    echo "  Consider removing unused code to improve maintainability"
else
    echo ""
    echo -e "${GREEN}✓ No major dead code issues detected${NC}"
fi

echo ""
echo "═══════════════════════════════════════════════════"
echo "Tip: Use 'RUSTFLAGS=\"-D dead_code\" cargo build' to"
echo "enforce no dead code in your builds"
echo "═══════════════════════════════════════════════════"