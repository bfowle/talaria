#!/bin/bash

# Build professional PDF from markdown using pandoc with custom template

INPUT="herald-architecture.md"
OUTPUT="herald-architecture.pdf"
TEMPLATE="pandoc-template.tex"

# Check if pandoc is installed
if ! command -v pandoc &> /dev/null; then
    echo "Error: pandoc is not installed"
    exit 1
fi

# Basic conversion with better formatting
echo "Building PDF with improved Unicode font support..."

# Check for professional academic fonts in order of preference
echo "Checking available fonts..."

# STIX Two - Most professional scientific publishing font
if fc-list | grep -q "STIX Two Text"; then
    echo "Using STIX Two fonts (professional scientific publishing standard)..."
    MAINFONT="STIX Two Text"
    MATHFONT="STIX Two Math"
    SANSFONT="TeX Gyre Heros"  # Or Liberation Sans if available
    MONOFONT="Liberation Mono"
    if fc-list | grep -q "Liberation Sans"; then
        SANSFONT="Liberation Sans"
    fi

# Libertinus - ACM standard, excellent for academic papers
elif fc-list | grep -q "Libertinus Serif"; then
    echo "Using Libertinus fonts (ACM publication standard)..."
    MAINFONT="Libertinus Serif"
    MATHFONT="Libertinus Math"
    SANSFONT="Libertinus Sans"
    MONOFONT="Libertinus Mono"

# Latin Modern - Enhanced Computer Modern, excellent Unicode support
elif fc-list | grep -q "Latin Modern Roman"; then
    echo "Using Latin Modern fonts (professional academic standard, excellent Unicode)..."
    MAINFONT="Latin Modern Roman"
    MATHFONT="Latin Modern Math"
    SANSFONT="Latin Modern Sans"
    MONOFONT="Latin Modern Mono"

# TeX Gyre Pagella - Elegant Palatino-like for readability
elif fc-list | grep -q "TeX Gyre Pagella"; then
    echo "Using TeX Gyre Pagella fonts (elegant academic style)..."
    MAINFONT="TeX Gyre Pagella"
    MATHFONT="TeX Gyre Pagella Math"
    SANSFONT="TeX Gyre Heros"
    MONOFONT="TeX Gyre Cursor"

# TeX Gyre Termes - IEEE standard alternative (Times-like)
# Note: Has limited Unicode subscript support, placed after Latin Modern
elif fc-list | grep -q "TeX Gyre Termes"; then
    echo "Using TeX Gyre Termes fonts (IEEE/Times-like standard)..."
    echo "Note: Some Unicode subscripts may not render correctly with TeX Gyre Termes."
    MAINFONT="TeX Gyre Termes"
    MATHFONT="TeX Gyre Termes Math"
    SANSFONT="TeX Gyre Heros"
    MONOFONT="TeX Gyre Cursor"

# DejaVu - Good Unicode support fallback
elif fc-list | grep -q "DejaVu Serif"; then
    echo "Using DejaVu fonts (Unicode fallback)..."
    MAINFONT="DejaVu Serif"
    MATHFONT=""  # No dedicated math font, will use defaults
    SANSFONT="DejaVu Sans"
    MONOFONT="DejaVu Sans Mono"

# Liberation - Metric-compatible with Times/Arial
elif fc-list | grep -q "Liberation Serif"; then
    echo "Using Liberation fonts (metric-compatible fallback)..."
    MAINFONT="Liberation Serif"
    MATHFONT=""  # No dedicated math font
    SANSFONT="Liberation Sans"
    MONOFONT="Liberation Mono"

# Final fallback - basic TeX fonts
else
    echo "Warning: No professional fonts found. Using basic TeX fonts..."
    echo "Recommend installing: sudo apt install fonts-stix fonts-texgyre fonts-libertinus lmodern"
    MAINFONT="TeX Gyre Termes"
    MATHFONT=""
    SANSFONT="TeX Gyre Heros"
    MONOFONT="TeX Gyre Cursor"
fi

# Build pandoc command with optional math font
PANDOC_CMD="pandoc \"$INPUT\" \
    --pdf-engine=lualatex \
    --from=markdown+tex_math_dollars+raw_tex \
    --to=latex \
    --output=\"$OUTPUT\" \
    --standalone \
    --number-sections \
    --toc \
    --toc-depth=3 \
    --variable documentclass=article \
    --variable fontsize=11pt \
    --variable geometry:margin=1in \
    --variable geometry:letterpaper \
    --variable colorlinks=true \
    --variable linkcolor=blue \
    --variable urlcolor=blue \
    --variable toccolor=black \
    --variable mainfont=\"$MAINFONT\" \
    --variable sansfont=\"$SANSFONT\" \
    --variable monofont=\"$MONOFONT\""

# Add math font if available
if [ -n "$MATHFONT" ]; then
    PANDOC_CMD="$PANDOC_CMD --variable mathfont=\"$MATHFONT\""
fi

# Continue with remaining options
PANDOC_CMD="$PANDOC_CMD \
    --highlight-style=tango \
    --variable header-includes=\"\\usepackage{unicode-math}\" \
    --variable header-includes=\"\\usepackage{microtype}\" \
    --variable header-includes=\"\\usepackage{parskip}\" \
    --variable header-includes=\"\\setlength{\\parindent}{0pt}\" \
    --variable header-includes=\"\\setlength{\\parskip}{0.8em}\" \
    --variable header-includes=\"\\usepackage{setspace}\" \
    --variable header-includes=\"\\onehalfspacing\" \
    --variable header-includes=\"\\usepackage{titlesec}\" \
    --variable header-includes=\"\\titlespacing*{\\section}{0pt}{3.5ex plus 1ex minus .2ex}{2.3ex plus .2ex}\" \
    --variable header-includes=\"\\titlespacing*{\\subsection}{0pt}{3.25ex plus 1ex minus .2ex}{1.5ex plus .2ex}\" \
    --metadata title=\"HERALD: Content-Addressed Storage for Efficient Biological Database Synchronization\" \
    --metadata author=\"Andromeda Tech, LLC\" \
    --metadata date=\"$(date +%Y-%m-%d)\""

# Execute the pandoc command
eval $PANDOC_CMD

echo "PDF generated: $OUTPUT"

# Alternative: Use custom template for maximum control
# Uncomment the following lines to use the custom template instead
# echo "Building PDF with custom template..."
# pandoc "$INPUT" \
#     --pdf-engine=lualatex \
#     --template="$TEMPLATE" \
#     --from=markdown+tex_math_dollars \
#     --to=latex \
#     --output="${OUTPUT%.pdf}-custom.pdf" \
#     --number-sections \
#     --metadata title="HERALD: Content-Addressed Storage for Efficient Biological Database Synchronization"
