#!/usr/bin/env bash

# md-to-academic-pdf.sh
# Convert Markdown files with Mermaid diagrams to academic-quality PDFs
# Usage: ./md-to-academic-pdf.sh [options] <input.md>

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
TEMPLATE="eisvogel"
PDF_ENGINE="xelatex"
OUTPUT_DIR="."
CACHE_DIR="$HOME/.cache/md-to-pdf"
TEMP_DIR=""
VERBOSE=false
CLEAN_TEMP=true
SKIP_MERMAID=false

# Paper settings
PAPER_SIZE="a4paper"
FONT_SIZE="11pt"
MARGIN="1in"
FONT_FAMILY="libertine"
TOC=true
NUMBER_SECTIONS=true

# Function to print colored messages
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Show usage
usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS] <input.md>

Convert Markdown files with Mermaid diagrams to academic-quality PDFs.

OPTIONS:
    -h, --help              Show this help message
    -o, --output <path>     Output PDF path (default: same as input with .pdf)
    -t, --template <name>   LaTeX template: eisvogel (default), ieee, acm, plain
    -e, --engine <name>     PDF engine: xelatex (default), lualatex, pdflatex
    -s, --font-size <size>  Font size (default: 11pt)
    -m, --margin <size>     Page margins (default: 1in)
    -f, --font <family>     Font family (default: libertine)
    --paper <size>          Paper size: a4paper (default), letter, legal
    --no-toc                Disable table of contents
    --no-numbers            Disable section numbering
    --no-clean              Keep temporary files for debugging
    --skip-mermaid          Skip Mermaid diagram rendering
    -v, --verbose           Enable verbose output

TEMPLATES:
    eisvogel    Beautiful academic template with professional typography
    ieee        IEEE conference/journal format
    acm         ACM conference format
    plain       Clean, minimal academic style

EXAMPLES:
    # Basic conversion with Eisvogel template
    $(basename "$0") paper.md

    # IEEE format with custom output
    $(basename "$0") --template ieee -o ieee-paper.pdf paper.md

    # A4 paper with larger font
    $(basename "$0") --font-size 12pt --paper a4paper paper.md

    # Keep temp files for debugging
    $(basename "$0") --no-clean --verbose paper.md

EOF
}

# Check for required dependencies
check_dependencies() {
    local missing=()
    local warnings=()
    local has_latex=false

    log_info "Checking dependencies..."

    # Check pandoc
    if ! command -v pandoc &> /dev/null; then
        missing+=("pandoc")
    else
        local pandoc_version=$(pandoc --version | head -n1 | cut -d' ' -f2)
        log_success "✓ pandoc $pandoc_version"
    fi

    # Check for any LaTeX engine
    if command -v xelatex &> /dev/null; then
        log_success "✓ xelatex"
        has_latex=true
    fi
    if command -v lualatex &> /dev/null; then
        log_success "✓ lualatex"
        has_latex=true
    fi
    if command -v pdflatex &> /dev/null; then
        log_success "✓ pdflatex"
        has_latex=true
    fi

    if [[ "$has_latex" = false ]]; then
        missing+=("latex")
    fi

    # Check if the selected PDF engine exists
    local original_engine="$PDF_ENGINE"
    if [[ "$has_latex" = true ]] && ! command -v "$PDF_ENGINE" &> /dev/null; then
        log_warning "! $PDF_ENGINE not found, will try to use an available engine"
        # Try to find an alternative
        if command -v lualatex &> /dev/null; then
            PDF_ENGINE="lualatex"
            log_info "  Using lualatex instead"
        elif command -v xelatex &> /dev/null; then
            PDF_ENGINE="xelatex"
            log_info "  Using xelatex instead"
        elif command -v pdflatex &> /dev/null; then
            PDF_ENGINE="pdflatex"
            log_info "  Using pdflatex instead"
        fi

        # Show how to install the requested engine if it was xelatex
        if [[ "$original_engine" == "xelatex" ]]; then
            echo ""
            echo "  💡 To install xelatex specifically:"
            echo "     Ubuntu/Debian:"
            echo "       sudo apt install texlive-xetex"
            echo "     Fedora/RHEL:"
            echo "       sudo dnf install texlive-xetex"
            echo "     macOS:"
            echo "       # Already included in mactex/basictex"
        fi
    fi

    # Check mermaid-cli (optional)
    if ! command -v mmdc &> /dev/null; then
        warnings+=("mmdc")
        log_warning "○ mmdc not found - Mermaid diagrams will not be rendered"
    else
        log_success "✓ mermaid-cli"
    fi

    # Check for rsvg-convert (optional)
    if ! command -v rsvg-convert &> /dev/null; then
        warnings+=("rsvg-convert")
        log_warning "○ rsvg-convert not found - SVG images may not render properly"
    else
        log_success "✓ rsvg-convert"
    fi

    # Check for Node.js (for mmdc)
    if ! command -v node &> /dev/null && ! command -v mmdc &> /dev/null; then
        warnings+=("nodejs")
    fi

    # Show installation instructions if dependencies are missing
    if [[ ${#missing[@]} -gt 0 ]]; then
        echo ""
        log_error "═══════════════════════════════════════════════════════"
        log_error "Missing REQUIRED dependencies"
        log_error "═══════════════════════════════════════════════════════"

        for dep in "${missing[@]}"; do
            case $dep in
                pandoc)
                    echo ""
                    echo "  📦 Pandoc (document converter):"
                    echo "     Ubuntu/Debian:"
                    echo "       sudo apt update"
                    echo "       sudo apt install pandoc"
                    echo ""
                    echo "     Fedora/RHEL:"
                    echo "       sudo dnf install pandoc"
                    echo ""
                    echo "     macOS:"
                    echo "       brew install pandoc"
                    echo ""
                    echo "     Or download from: https://pandoc.org/installing.html"
                    ;;
                latex)
                    echo ""
                    echo "  📦 LaTeX (PDF generation):"
                    echo "     Ubuntu/Debian (recommended - full installation):"
                    echo "       sudo apt update"
                    echo "       sudo apt install texlive-full"
                    echo ""
                    echo "     Ubuntu/Debian (minimal - may need additional packages):"
                    echo "       sudo apt update"
                    echo "       sudo apt install texlive-xetex texlive-latex-recommended \\"
                    echo "                        texlive-fonts-recommended texlive-latex-extra \\"
                    echo "                        texlive-fonts-extra lmodern"
                    echo ""
                    echo "     Fedora/RHEL (full installation):"
                    echo "       sudo dnf install texlive-scheme-full"
                    echo ""
                    echo "     Fedora/RHEL (minimal with xelatex):"
                    echo "       sudo dnf install texlive-xetex texlive-latex texlive-collection-fontsrecommended"
                    echo ""
                    echo "     macOS:"
                    echo "       brew install --cask mactex"
                    echo "       # Or for minimal installation:"
                    echo "       brew install --cask basictex"
                    echo ""
                    echo "     Windows (WSL):"
                    echo "       # Use the Ubuntu/Debian instructions above"
                    ;;
            esac
        done

        echo ""
        log_error "Cannot proceed without required dependencies."
        exit 1
    fi

    # Show optional dependency instructions
    if [[ ${#warnings[@]} -gt 0 ]]; then
        echo ""
        log_info "═══════════════════════════════════════════════════════"
        log_info "Optional dependencies for enhanced features"
        log_info "═══════════════════════════════════════════════════════"

        if [[ " ${warnings[@]} " =~ " nodejs " ]] || [[ " ${warnings[@]} " =~ " mmdc " ]]; then
            echo ""
            echo "  📊 Mermaid diagram support:"
            echo "     First install Node.js:"
            echo "       Ubuntu/Debian:"
            echo "         curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -"
            echo "         sudo apt install nodejs"
            echo ""
            echo "       Fedora/RHEL:"
            echo "         sudo dnf install nodejs"
            echo ""
            echo "       macOS:"
            echo "         brew install node"
            echo ""
            echo "     Then install mermaid-cli:"
            echo "       npm install -g @mermaid-js/mermaid-cli"
        fi

        if [[ " ${warnings[@]} " =~ " rsvg-convert " ]]; then
            echo ""
            echo "  🖼️  SVG image support:"
            echo "     Ubuntu/Debian:"
            echo "       sudo apt install librsvg2-bin"
            echo ""
            echo "     Fedora/RHEL:"
            echo "       sudo dnf install librsvg2-tools"
            echo ""
            echo "     macOS:"
            echo "       brew install librsvg"
        fi

        echo ""
        log_info "Continuing without optional features..."
        echo ""
    fi
}

# Download and cache LaTeX templates
download_template() {
    local template_name="$1"
    local template_file="$CACHE_DIR/templates/${template_name}.latex"

    mkdir -p "$CACHE_DIR/templates"

    if [[ -f "$template_file" ]] && [[ ! "$VERBOSE" = true ]]; then
        log_info "Using cached template: $template_name"
        echo "$template_file"
        return
    fi

    log_info "Downloading template: $template_name"

    case $template_name in
        eisvogel)
            # Download and extract Eisvogel template
            local temp_archive="$CACHE_DIR/eisvogel.tar.gz"
            if ! curl -sL "https://github.com/Wandmalfarbe/pandoc-latex-template/releases/download/v3.2.0/Eisvogel.tar.gz" \
                    -o "$temp_archive"; then
                log_error "Failed to download Eisvogel template"
                exit 1
            fi
            # Extract the template
            tar -xzf "$temp_archive" -C "$CACHE_DIR/templates/" eisvogel.latex 2>/dev/null || {
                # Try alternate extraction
                tar -xzf "$temp_archive" -C "$CACHE_DIR/templates/" 2>/dev/null
                if [[ ! -f "$template_file" ]]; then
                    log_error "Failed to extract Eisvogel template"
                    exit 1
                fi
            }
            rm -f "$temp_archive"
            ;;
        ieee)
            curl -sL "https://raw.githubusercontent.com/stsewd/ieee-pandoc-template/master/template.latex" \
                -o "$template_file" || {
                    log_error "Failed to download IEEE template"
                    exit 1
                }
            ;;
        acm)
            # ACM template - simplified version
            cat > "$template_file" << 'EOF'
\documentclass[sigconf]{acmart}
$if(natbib)$
\usepackage{natbib}
\bibliographystyle{ACM-Reference-Format}
$endif$

\begin{document}
$if(title)$
\title{$title$}
$endif$
$if(author)$
\author{$for(author)$$author$$sep$, $endfor$}
$endif$
$if(date)$
\date{$date$}
$endif$
$if(title)$
\maketitle
$endif$
$if(abstract)$
\begin{abstract}
$abstract$
\end{abstract}
$endif$
$body$
$if(natbib)$
\bibliography{$bibliography$}
$endif$
\end{document}
EOF
            ;;
        plain)
            # Use pandoc's default template
            echo "default" > "$template_file"
            ;;
        *)
            log_error "Unknown template: $template_name"
            exit 1
            ;;
    esac

    log_success "Template downloaded: $template_name"
    echo "$template_file"
}

# Pre-render Mermaid diagrams to PNG
# Extract YAML metadata from HTML comments if present
extract_yaml_from_comments() {
    local input_file="$1"
    local output_file="$2"

    # Check if file has YAML in HTML comments
    if grep -q '^<!--$' "$input_file" && grep -q '^-->$' "$input_file"; then
        # Extract YAML from HTML comments and make it visible
        awk '
        BEGIN { in_comment = 0; found_yaml = 0 }
        /^<!--$/ {
            in_comment = 1
            next
        }
        in_comment && /^---$/ {
            found_yaml = 1
            print $0
            next
        }
        in_comment && found_yaml && /^---$/ {
            print $0
            found_yaml = 0
            next
        }
        in_comment && found_yaml {
            print $0
            next
        }
        /^-->$/ && in_comment {
            in_comment = 0
            next
        }
        !in_comment { print }
        ' "$input_file" > "$output_file"
    else
        cp "$input_file" "$output_file"
    fi
}

render_mermaid_diagrams() {
    local input_file="$1"
    local output_file="$2"
    local temp_dir="$3"

    if [[ "$SKIP_MERMAID" = true ]] || ! command -v mmdc &> /dev/null; then
        cp "$input_file" "$output_file"
        return
    fi

    log_info "Processing Mermaid diagrams..."

    # Create diagrams directory
    local diagram_dir="$temp_dir/diagrams"
    mkdir -p "$diagram_dir"

    # Count Mermaid diagrams using grep
    local diagram_count=$(grep -c '^```mermaid$' "$input_file" || true)

    if [[ $diagram_count -eq 0 ]]; then
        log_info "No Mermaid diagrams found"
        cp "$input_file" "$output_file"
        return
    fi

    log_info "Found $diagram_count Mermaid diagram(s) to process"

    # Process the file using awk instead of while loop
    awk -v dir="$diagram_dir" -v verbose="$VERBOSE" '
    BEGIN {
        diagram_num = 0
        in_mermaid = 0
        mermaid_content = ""
    }
    /^```mermaid$/ {
        in_mermaid = 1
        mermaid_content = ""
        diagram_num++
        if (verbose == "true") print "[INFO] Processing diagram", diagram_num
        next
    }
    in_mermaid && /^```$/ {
        in_mermaid = 0
        # Save mermaid content to file
        mermaid_file = dir "/diagram_" diagram_num ".mmd"
        print mermaid_content > mermaid_file
        close(mermaid_file)

        # Generate placeholder for image
        print "<!-- MERMAID_PLACEHOLDER_" diagram_num " -->"
        next
    }
    in_mermaid {
        if (mermaid_content == "")
            mermaid_content = $0
        else
            mermaid_content = mermaid_content "\n" $0
        next
    }
    { print }
    ' "$input_file" > "$temp_dir/temp_with_placeholders.md"

    # Now render each diagram and replace placeholders
    cp "$temp_dir/temp_with_placeholders.md" "$output_file"

    for ((i=1; i<=diagram_count; i++)); do
        local mermaid_file="$diagram_dir/diagram_${i}.mmd"
        local image_file="$diagram_dir/diagram_${i}.png"

        if [[ -f "$mermaid_file" ]]; then
            log_info "Rendering diagram $i..."

            # Create a monochrome config for black and white diagrams
            local config_file="$temp_dir/mermaid-config.json"
            cat > "$config_file" << 'MERMAID_CONFIG'
{
  "theme": "base",
  "themeVariables": {
    "primaryColor": "#f9f9f9",
    "primaryTextColor": "#000000",
    "primaryBorderColor": "#333333",
    "lineColor": "#333333",
    "secondaryColor": "#f0f0f0",
    "tertiaryColor": "#e0e0e0",
    "background": "#ffffff",
    "mainBkg": "#f9f9f9",
    "secondBkg": "#eeeeee",
    "tertiaryBkg": "#dddddd",
    "primaryBorderColor": "#333333",
    "secondaryBorderColor": "#666666",
    "tertiaryBorderColor": "#999999",
    "fontFamily": "Arial, sans-serif",
    "fontSize": "14px",
    "darkMode": false,
    "actorBkg": "#f9f9f9",
    "actorBorder": "#333333",
    "actorTextColor": "#000000",
    "actorLineColor": "#333333",
    "signalColor": "#000000",
    "signalTextColor": "#000000",
    "noteBkgColor": "#f0f0f0",
    "noteBorderColor": "#333333",
    "noteTextColor": "#000000",
    "labelBoxBkgColor": "#f9f9f9",
    "labelBoxBorderColor": "#333333",
    "labelTextColor": "#000000",
    "loopTextColor": "#000000",
    "nodeTextColor": "#000000",
    "nodeBkg": "#f9f9f9",
    "nodeBorder": "#333333",
    "clusterBkg": "#eeeeee",
    "clusterBorder": "#666666",
    "defaultLinkColor": "#333333",
    "edgeLabelBackground": "#ffffff",
    "titleColor": "#000000",
    "sectionBkgColor": "#f0f0f0",
    "altSectionBkgColor": "#e0e0e0",
    "sectionBkgColor2": "#e0e0e0",
    "taskBorderColor": "#333333",
    "taskBkgColor": "#f9f9f9",
    "taskTextColor": "#000000",
    "doneTaskBkgColor": "#cccccc",
    "doneTaskBorderColor": "#666666",
    "critBorderColor": "#666666",
    "critBkgColor": "#dddddd",
    "todayLineColor": "#666666",
    "fillType0": "#f9f9f9",
    "fillType1": "#eeeeee",
    "fillType2": "#dddddd",
    "fillType3": "#cccccc",
    "fillType4": "#bbbbbb",
    "fillType5": "#aaaaaa",
    "fillType6": "#999999",
    "fillType7": "#888888"
  },
  "flowchart": {
    "htmlLabels": false,
    "curve": "linear",
    "nodeSpacing": 80,
    "rankSpacing": 100,
    "useMaxWidth": false,
    "diagramPadding": 20,
    "defaultRenderer": "dagre"
  }
}
MERMAID_CONFIG

            if timeout 15 mmdc -i "$mermaid_file" -o "$image_file" \
                    --configFile "$config_file" \
                    --backgroundColor white \
                    --width 3000 \
                    --height 1200 >/dev/null 2>&1; then

                # Convert to grayscale to ensure black and white output
                if command -v convert &> /dev/null; then
                    convert "$image_file" -colorspace Gray "$image_file" 2>/dev/null || true
                fi

                log_success "Rendered diagram $i"
                # Replace placeholder with image reference
                sed -i "s|<!-- MERMAID_PLACEHOLDER_${i} -->|![Diagram ${i}](${image_file})|" "$output_file"
            else
                log_warning "Failed to render diagram $i, restoring code block"
                # Replace placeholder with original mermaid block
                local mermaid_content=$(cat "$mermaid_file")
                # Use a temporary file for complex replacement
                awk -v num="$i" -v content="$mermaid_content" '
                    /<!-- MERMAID_PLACEHOLDER_/ && index($0, num " -->") {
                        print "```mermaid"
                        print content
                        print "```"
                        next
                    }
                    { print }
                ' "$output_file" > "$output_file.tmp" && mv "$output_file.tmp" "$output_file"
            fi
        fi
    done

    log_success "Processed $diagram_count Mermaid diagram(s)"
}

# Convert markdown to PDF using pandoc
convert_to_pdf() {
    local input_file="$1"
    local output_file="$2"
    local template_file="$3"

    log_info "Converting to PDF with pandoc..."

    # Build pandoc command
    local pandoc_args=(
        "$input_file"
        -o "$output_file"
        --pdf-engine="$PDF_ENGINE"
        --from markdown+yaml_metadata_block+tex_math_dollars+pipe_tables+backtick_code_blocks+fenced_code_attributes+footnotes+definition_lists+raw_html+raw_tex+startnum+fancy_lists+compact_definition_lists+blank_before_header+blank_before_blockquote
        --standalone
        --highlight-style=tango
        --pdf-engine-opt=-shell-escape
    )

    # Add template if not default
    if [[ "$template_file" != "default" ]] && [[ -f "$template_file" ]]; then
        pandoc_args+=(--template="$template_file")
    fi

    # Add metadata variables
    pandoc_args+=(
        -V documentclass=article
        -V papersize="$PAPER_SIZE"
        -V fontsize="$FONT_SIZE"
        -V geometry:margin="$MARGIN"
        -V colorlinks=true
        -V linkcolor=blue
        -V urlcolor=blue
        -V toccolor=black
    )

    # Add proper spacing between main content and footnotes and configure title formatting
    pandoc_args+=(
        -V header-includes="\usepackage{titling} \usepackage{authblk} \setlength{\footnotesep}{12pt} \setlength{\skip\footins}{20pt plus 4pt minus 2pt} \setlength{\droptitle}{-2em} \pretitle{\begin{center}\LARGE\bfseries} \posttitle{\par\end{center}\vskip 1.5em} \preauthor{\begin{center}\large} \postauthor{\end{center}} \predate{\begin{center}\normalsize} \postdate{\par\end{center}\vskip 2em}"
    )

    # Font settings for xelatex/lualatex
    if [[ "$PDF_ENGINE" == "xelatex" ]] || [[ "$PDF_ENGINE" == "lualatex" ]]; then
        # Use academic/professional fonts with full Unicode support
        case $FONT_FAMILY in
            libertine)
                # Use EB Garamond for classic academic look with full Unicode support
                if fc-list | grep -q "EBGaramond"; then
                    pandoc_args+=(
                        -V mainfont="EB Garamond 12"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                        -V mainfontoptions="Numbers=OldStyle"
                    )
                else
                    # Fallback to DejaVu for Unicode support
                    pandoc_args+=(
                        -V mainfont="DejaVu Serif"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                    )
                fi
                ;;
            times)
                # Use EB Garamond as a Times alternative with better Unicode support
                if fc-list | grep -q "EBGaramond"; then
                    pandoc_args+=(
                        -V mainfont="EB Garamond 12"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                    )
                else
                    pandoc_args+=(
                        -V mainfont="DejaVu Serif"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                    )
                fi
                ;;
            computer-modern)
                # Use EB Garamond for professional academic appearance
                if fc-list | grep -q "EBGaramond"; then
                    pandoc_args+=(
                        -V mainfont="EB Garamond 12"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                        -V mainfontoptions="Numbers=OldStyle"
                    )
                else
                    pandoc_args+=(
                        -V mainfont="DejaVu Serif"
                        -V sansfont="DejaVu Sans"
                        -V monofont="DejaVu Sans Mono"
                        -V mathfont="DejaVu Math TeX Gyre"
                    )
                fi
                ;;
            *)
                # For custom fonts, still try to set them
                pandoc_args+=(-V mainfont="$FONT_FAMILY")
                ;;
        esac

        # Add line spacing for academic papers
        pandoc_args+=(
            -V linestretch=1.5
            -V indent=true
        )
    fi

    # Table of contents
    if [[ "$TOC" = true ]]; then
        pandoc_args+=(--toc --toc-depth=3)
    fi

    # Section numbering - only if document doesn't already have numbers
    # Check if the document already has numbered sections (e.g., "1.1 ", "2.3.4 ")
    if [[ "$NUMBER_SECTIONS" = true ]]; then
        # Check for existing section numbers at the start of headers
        if ! grep -qE '^#{1,6}\s+[0-9]+(\.[0-9]+)*\s+' "$input_file"; then
            pandoc_args+=(--number-sections)
        else
            if [[ "$VERBOSE" = true ]]; then
                log_info "Document already has numbered sections, skipping --number-sections"
            fi
        fi
    fi

    # Add citation processing if bibliography exists
    if grep -q "\[^" "$input_file" 2>/dev/null; then
        pandoc_args+=(--citeproc)
    fi

    # Verbose output
    if [[ "$VERBOSE" = true ]]; then
        pandoc_args+=(--verbose)
        echo "Pandoc command: pandoc ${pandoc_args[*]}"
    fi

    # Run pandoc
    if pandoc "${pandoc_args[@]}"; then
        log_success "PDF generated successfully: $output_file"
        return 0
    else
        log_error "PDF generation failed"
        return 1
    fi
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                usage
                exit 0
                ;;
            -o|--output)
                OUTPUT_FILE="$2"
                shift 2
                ;;
            -t|--template)
                TEMPLATE="$2"
                shift 2
                ;;
            -e|--engine)
                PDF_ENGINE="$2"
                shift 2
                ;;
            -s|--font-size)
                FONT_SIZE="$2"
                shift 2
                ;;
            -m|--margin)
                MARGIN="$2"
                shift 2
                ;;
            -f|--font)
                FONT_FAMILY="$2"
                shift 2
                ;;
            --paper)
                PAPER_SIZE="$2"
                shift 2
                ;;
            --no-toc)
                TOC=false
                shift
                ;;
            --no-numbers)
                NUMBER_SECTIONS=false
                shift
                ;;
            --no-clean)
                CLEAN_TEMP=false
                shift
                ;;
            --skip-mermaid)
                SKIP_MERMAID=true
                shift
                ;;
            -v|--verbose)
                VERBOSE=true
                shift
                ;;
            -*)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
            *)
                INPUT_FILE="$1"
                shift
                ;;
        esac
    done
}

# Main function
main() {
    # Parse arguments
    parse_args "$@"

    # Validate input
    if [[ -z "${INPUT_FILE:-}" ]]; then
        log_error "No input file specified"
        usage
        exit 1
    fi

    if [[ ! -f "$INPUT_FILE" ]]; then
        log_error "Input file not found: $INPUT_FILE"
        exit 1
    fi

    # Set output file if not specified
    if [[ -z "${OUTPUT_FILE:-}" ]]; then
        OUTPUT_FILE="${INPUT_FILE%.md}.pdf"
    fi

    log_info "Starting PDF generation..."
    log_info "Input: $INPUT_FILE"
    log_info "Output: $OUTPUT_FILE"
    log_info "Template: $TEMPLATE"

    # Check dependencies
    check_dependencies

    # Create temp directory
    TEMP_DIR=$(mktemp -d -t md-to-pdf.XXXXXX)
    trap 'cleanup' EXIT

    # Download template
    local template_file
    if [[ "$TEMPLATE" == "plain" ]]; then
        template_file="default"
    else
        template_file=$(download_template "$TEMPLATE")
    fi

    # Extract YAML from HTML comments if present
    local yaml_extracted_file="$TEMP_DIR/with_yaml.md"
    extract_yaml_from_comments "$INPUT_FILE" "$yaml_extracted_file"

    # Pre-render Mermaid diagrams
    local processed_file="$TEMP_DIR/processed.md"
    render_mermaid_diagrams "$yaml_extracted_file" "$processed_file" "$TEMP_DIR"

    # Convert to PDF
    convert_to_pdf "$processed_file" "$OUTPUT_FILE" "$template_file"

    # Show file size
    if [[ -f "$OUTPUT_FILE" ]]; then
        local size=$(du -h "$OUTPUT_FILE" | cut -f1)
        log_success "Output size: $size"
    fi
}

# Cleanup function
cleanup() {
    if [[ "$CLEAN_TEMP" = true ]] && [[ -n "$TEMP_DIR" ]] && [[ -d "$TEMP_DIR" ]]; then
        rm -rf "$TEMP_DIR"
    elif [[ "$CLEAN_TEMP" = false ]]; then
        log_info "Temporary files kept in: $TEMP_DIR"
    fi
}

# Run main function
main "$@"