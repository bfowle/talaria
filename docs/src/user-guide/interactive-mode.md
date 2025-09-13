# Interactive Mode

Talaria provides a powerful Terminal User Interface (TUI) for interactive operations, making complex tasks more accessible through guided wizards and visual interfaces.

## Starting Interactive Mode

```bash
# Launch interactive mode
talaria interactive

# Or use shorthand
talaria -i
```

## Main Menu

The interactive mode presents a main menu with the following options:

1. **Download databases** - Download biological databases with progress tracking
2. **Reduce a FASTA file** - Intelligently reduce FASTA files with guided configuration
3. **View statistics** - Analyze FASTA files and view detailed statistics
4. **Setup wizard** - Configure Talaria for first-time use
5. **Configure settings** - Edit configuration with a visual editor
6. **View documentation** - Browse built-in documentation
7. **Exit** - Exit interactive mode

### Navigation

- **↑/↓** or **j/k**: Navigate menu items
- **Enter**: Select item
- **q** or **Esc**: Exit/go back

## Features

### 1. Database Download Wizard

Interactive database downloading with real-time progress:

```
┌─ Database Download Wizard ─────────────┐
│                                        │
│  Select Source:                        │
│  > UniProt - Protein sequences         │
│    NCBI - Comprehensive databases      │
│    Custom - Local file                 │
│                                        │
└────────────────────────────────────────┘
```

Features:
- Database source selection (UniProt, NCBI)
- Dataset selection with size information
- Real-time download progress
- Automatic decompression
- Checksum verification

### 2. FASTA Reduction Wizard

Step-by-step FASTA reduction with visual feedback:

```
┌─ FASTA Reduction Wizard ───────────────┐
│                                        │
│  Select Target Aligner:                │
│  > LAMBDA - Fast protein aligner       │
│    BLAST - Traditional aligner         │
│    Diamond - Ultra-fast aligner        │
│    MMseqs2 - Sensitive search          │
│    Kraken - Taxonomic classifier       │
│                                        │
└────────────────────────────────────────┘
```

Steps:
1. Input file selection
2. Target aligner selection
3. Configuration options (threshold, identity, taxonomy)
4. Review settings
5. Processing with progress bar
6. Results summary

Configuration options:
- **Clustering threshold**: 0.0-1.0 (similarity threshold)
- **Min identity**: 0.0-1.0 (minimum sequence identity)
- **Preserve taxonomy**: Yes/No (maintain taxonomic diversity)
- **Remove redundant**: Yes/No (remove duplicate sequences)
- **Optimize for memory**: Yes/No (memory-efficient processing)

### 3. Statistics Viewer

Interactive FASTA file analysis with multiple views:

```
┌─ Database Statistics ──────────────────┐
│ Overview | Distributions | Analysis   │
├────────────────────────────────────────┤
│ Total Sequences:      12,543          │
│ Total Bases:          4,567,890        │
│ Avg Length:           364.2 bp         │
│ GC Content:           52.3%            │
│ Redundancy:           15.7%            │
│ Taxonomy Div:         78.4%            │
│ Compression:          1.2x             │
└────────────────────────────────────────┘
```

Tabs:
- **Overview**: Key metrics, GC content gauge, sequence count trends
- **Distributions**: Length distribution chart, composition analysis
- **Analysis**: Recommendations, memory requirements, optimization suggestions

Navigation:
- **Tab/Shift-Tab**: Switch between tabs
- **↑/↓**: Scroll content
- **q**: Exit viewer

### 4. Setup Wizard

First-time configuration wizard:

1. **Aligner selection**: Choose your primary aligner
2. **Input/output paths**: Set default directories
3. **Reduction parameters**: Configure thresholds
4. **Save configuration**: Optionally save for future use

The wizard creates a configuration file at `~/.config/talaria/config.toml`.

### 5. Configuration Editor

Visual configuration editor with field validation:

```
┌─ Talaria Configuration Editor ─────────┐
│ File: ~/.config/talaria/config.toml   │
├────────────────────────────────────────┤
│ ▶ Target Ratio              0.30      │
│   Min Sequence Length       50        │
│   Max Delta Distance        100       │
│   Similarity Threshold      0.90      │
│   Taxonomy Aware            [✓]       │
│   Gap Penalty               -11       │
│   Gap Extension             -1        │
│   Algorithm                 nw        │
│   Output Format             fasta     │
│   Include Metadata          [✓]       │
│   Compress Output           [ ]       │
│   Chunk Size                10000     │
└────────────────────────────────────────┘
[s]ave [l]oad [r]eset [q]uit
```

Features:
- Edit all configuration parameters
- Boolean toggles with Space/Enter
- Numeric validation
- Save/load configurations
- Reset to defaults

Keyboard shortcuts:
- **↑/↓** or **j/k**: Navigate fields
- **Enter**: Edit field (or toggle boolean)
- **Space**: Toggle boolean fields
- **s**: Save configuration
- **l**: Load configuration
- **r**: Reset to defaults
- **q** or **Esc**: Exit editor

### 6. Documentation Viewer

Built-in documentation browser:

```
┌─ Documentation ─────────────────────────┐
│ Quick Start | Algorithms | Examples    │
├────────────────────────────────────────┤
│ # Quick Start Guide                    │
│                                        │
│ Welcome to Talaria! This tool         │
│ intelligently reduces FASTA databases │
│ for optimal indexing.                 │
│                                        │
│ ## Basic Usage                         │
│                                        │
│ 1. Reduce a FASTA file:               │
│    talaria reduce -i input.fasta ...  │
│                                        │
└────────────────────────────────────────┘
Tab: Switch section | ↑/↓: Scroll | q: Quit
```

Sections:
- **Quick Start**: Getting started guide
- **Reduction Algorithm**: Technical details
- **Aligner Optimizations**: Aligner-specific strategies
- **Configuration**: Configuration guide
- **Examples**: Common use cases
- **FAQ**: Frequently asked questions

Navigation:
- **Tab/Shift-Tab** or **←/→**: Switch sections
- **↑/↓** or **j/k**: Scroll content
- **PgUp/PgDn**: Fast scroll
- **q**: Exit viewer

## Color Themes

The interface uses color coding for clarity:
- **Cyan**: Headers and titles
- **Yellow**: Selected items and highlights
- **Green**: Success messages and positive values
- **Red**: Errors and warnings
- **White**: Normal text
- **Gray**: Help text and descriptions

## Terminal Requirements

- Minimum terminal size: 80x24
- Unicode support for box drawing characters
- 256-color terminal recommended
- Works in: iTerm2, Terminal.app, GNOME Terminal, Windows Terminal, etc.

## Tips and Tricks

1. **Quick navigation**: Use vim-style keys (j/k) for faster navigation
2. **Escape anywhere**: Press Esc to go back or cancel operations
3. **Tab completion**: In file dialogs, use Tab for path completion
4. **Progress monitoring**: All long operations show real-time progress
5. **Configuration persistence**: Settings are saved automatically

## Troubleshooting

### Terminal Issues

If the interface appears corrupted:
```bash
# Reset terminal
reset

# Or clear and restart
clear && talaria interactive
```

### Color Problems

If colors don't display correctly:
```bash
# Check terminal color support
echo $TERM

# Set to 256-color mode
export TERM=xterm-256color
```

### Unicode Issues

If box characters appear as question marks:
```bash
# Check locale
locale

# Set UTF-8 locale
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
```

## Examples

### Complete Reduction Workflow

1. Start interactive mode: `talaria interactive`
2. Select "Download databases"
3. Choose UniProt → SwissProt
4. Wait for download to complete
5. Select "Reduce a FASTA file"
6. Enter the downloaded file path
7. Choose target aligner (e.g., LAMBDA)
8. Configure options
9. Review and start reduction
10. View statistics on the reduced file

### Quick Configuration

1. Start interactive mode: `talaria interactive`
2. Select "Configure settings"
3. Navigate to desired field with arrow keys
4. Press Enter to edit
5. Type new value and press Enter
6. Press 's' to save
7. Press 'q' to exit

## See Also

- [Configuration](configuration.md) - Detailed configuration options
- [Basic Usage](basic-usage.md) - Command-line usage
- [Downloading Databases](../databases/downloading.md) - Database download guide