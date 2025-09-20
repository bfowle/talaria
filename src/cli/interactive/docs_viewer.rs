use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;

pub struct DocsViewer {
    sections: Vec<(&'static str, &'static str)>,
    current_section: usize,
    list_state: ListState,
    scroll_offset: u16,
}

impl DocsViewer {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let sections = vec![
            ("Quick Start", QUICK_START_DOC),
            ("Reduction Algorithm", REDUCTION_ALGORITHM_DOC),
            ("Aligner Optimizations", ALIGNER_OPTIMIZATIONS_DOC),
            ("Configuration", CONFIGURATION_DOC),
            ("Examples", EXAMPLES_DOC),
            ("FAQ", FAQ_DOC),
        ];

        Self {
            sections,
            current_section: 0,
            list_state,
            scroll_offset: 0,
        }
    }

    fn next_section(&mut self) {
        self.current_section = (self.current_section + 1) % self.sections.len();
        self.list_state.select(Some(self.current_section));
        self.scroll_offset = 0;
    }

    fn previous_section(&mut self) {
        if self.current_section == 0 {
            self.current_section = self.sections.len() - 1;
        } else {
            self.current_section -= 1;
        }
        self.list_state.select(Some(self.current_section));
        self.scroll_offset = 0;
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    fn scroll_down(&mut self) {
        self.scroll_offset += 1;
    }
}

pub fn run_docs_viewer<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut viewer = DocsViewer::new();

    loop {
        terminal.draw(|f| draw_viewer(f, &mut viewer))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => break,
                KeyCode::Tab | KeyCode::Right => viewer.next_section(),
                KeyCode::BackTab | KeyCode::Left => viewer.previous_section(),
                KeyCode::Down | KeyCode::Char('j') => viewer.scroll_down(),
                KeyCode::Up | KeyCode::Char('k') => viewer.scroll_up(),
                KeyCode::PageDown => {
                    for _ in 0..10 {
                        viewer.scroll_down();
                    }
                }
                KeyCode::PageUp => {
                    for _ in 0..10 {
                        viewer.scroll_up();
                    }
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn draw_viewer(f: &mut Frame, viewer: &mut DocsViewer) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Section tabs
    let titles: Vec<Span> = viewer
        .sections
        .iter()
        .enumerate()
        .map(|(i, (title, _))| {
            if i == viewer.current_section {
                Span::styled(
                    format!(" {} ", title),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(format!(" {} ", title), Style::default().fg(Color::White))
            }
        })
        .collect();

    let tabs = Paragraph::new(Line::from(titles))
        .block(
            Block::default()
                .title(" Documentation ")
                .borders(Borders::ALL),
        )
        .alignment(Alignment::Center);
    f.render_widget(tabs, chunks[0]);

    // Content
    let (_title, content) = viewer.sections[viewer.current_section];
    let lines: Vec<Line> = content
        .lines()
        .skip(viewer.scroll_offset as usize)
        .map(|line| {
            if line.starts_with('#') {
                Line::from(Span::styled(
                    line,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if line.starts_with('-') || line.starts_with('*') {
                Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))
            } else if line.starts_with("```") {
                Line::from(Span::styled(line, Style::default().fg(Color::Green)))
            } else {
                Line::from(Span::raw(line))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, chunks[1]);

    // Help bar
    let help =
        Paragraph::new("Tab/←→: Switch section | ↑/↓: Scroll | PgUp/PgDn: Fast scroll | q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
    f.render_widget(help, chunks[2]);
}

// Documentation content
const QUICK_START_DOC: &str = r#"
# Quick Start Guide

Welcome to Talaria! This tool intelligently reduces FASTA databases for optimal indexing.

## Basic Usage

1. **Reduce a FASTA file:**
   ```
   talaria reduce -i input.fasta -o output.fasta -a lambda
   ```

2. **Download a database:**
   ```
   talaria download --database uniprot --dataset swissprot
   ```

3. **View statistics:**
   ```
   talaria stats -i reduced.fasta
   ```

## Interactive Mode

Run `talaria interactive` for a user-friendly TUI with:
- Database download wizard
- FASTA reduction wizard
- Statistics viewer
- Configuration editor

## Key Concepts

- **Target Ratio**: How much to reduce the database (0.3 = 30% of original size)
- **Similarity Threshold**: Sequences more similar than this are grouped
- **Taxonomy Aware**: Preserves taxonomic diversity during reduction
"#;

const REDUCTION_ALGORITHM_DOC: &str = r#"
# Reduction Algorithm

Talaria uses a multi-stage approach to intelligently reduce FASTA databases:

## 1. Reference Selection

The algorithm selects representative sequences using:
- Greedy set cover approach
- Sequence similarity clustering
- Taxonomic diversity preservation

## 2. Delta Encoding

Non-reference sequences are encoded as differences from their closest reference:
- Needleman-Wunsch alignment for optimal alignment
- Compact delta format for storage
- Lossless reconstruction capability

## 3. Optimization Strategies

Different aligners benefit from different optimization strategies:
- **LAMBDA**: Optimizes for k-mer diversity
- **BLAST**: Preserves sequence complexity distribution
- **Kraken**: Maintains taxonomic representation
- **Diamond**: Ensures seed diversity for double-indexing
- **MMseqs2**: Balances sensitivity and speed

## Configuration

Key parameters:
- `target_ratio`: Size reduction goal (0.0-1.0)
- `similarity_threshold`: Clustering threshold (0.0-1.0)
- `min_sequence_length`: Filter short sequences
- `taxonomy_aware`: Enable taxonomic balancing
"#;

const ALIGNER_OPTIMIZATIONS_DOC: &str = r#"
# Aligner-Specific Optimizations

Each aligner has unique indexing requirements that Talaria optimizes for:

## LAMBDA (Seqan3)
- **K-mer diversity**: Maximizes unique k-mers in reference set
- **Seed coverage**: Ensures good seed distribution
- **Format**: Outputs LAMBDA-compatible FASTA

## BLAST
- **Complexity weighting**: Prioritizes high-complexity sequences
- **Database composition**: Maintains statistical properties for E-values
- **Format**: Standard NCBI FASTA format

## Kraken
- **Taxonomic coverage**: One reference per taxonomic group minimum
- **K-mer uniqueness**: Minimizes ambiguous k-mers
- **Format**: Kraken-compatible with taxonomy IDs

## Diamond
- **Double-indexing optimization**: Seed diversity for both stages
- **Taxonomy grouping**: Interleaves taxonomic groups
- **Complexity sorting**: Complex sequences first

## MMseqs2
- **Profile diversity**: Optimizes for profile search
- **Clustering**: Uses MMseqs2-like clustering
- **Sensitivity levels**: Adjustable for different sensitivity modes

## Generic
- **Balanced approach**: No specific optimizations
- **Standard clustering**: Uses similarity-based clustering
- **Universal compatibility**: Works with any aligner
"#;

const CONFIGURATION_DOC: &str = r#"
# Configuration Guide

Talaria uses a TOML configuration file for persistent settings.

## Location
Default: `~/.config/talaria/config.toml`

## Structure

```toml
[reduction]
target_ratio = 0.3
min_sequence_length = 50
max_delta_distance = 100
similarity_threshold = 0.9
taxonomy_aware = true

[alignment]
gap_penalty = -11
gap_extension = -1
algorithm = "needleman-wunsch"

[output]
format = "fasta"
include_metadata = true
compress_output = false

[performance]
chunk_size = 10000
batch_size = 1000
cache_alignments = true
```

## Loading Configuration

1. **Command line**: `--config path/to/config.toml`
2. **Environment**: `TALARIA_CONFIG=/path/to/config.toml`
3. **Default**: Automatically loads from `~/.config/talaria/config.toml`

## Interactive Editing

Use the configuration editor in interactive mode:
1. Run `talaria interactive`
2. Select "Configure settings"
3. Edit values with arrow keys and Enter
4. Press 's' to save
"#;

const EXAMPLES_DOC: &str = r#"
# Examples

## Common Use Cases

### 1. Reduce UniProt for LAMBDA
```bash
# Download SwissProt
talaria download --database uniprot --dataset swissprot -o swissprot.fasta

# Reduce to 30% for LAMBDA indexing
talaria reduce -i swissprot.fasta -o swissprot.reduced.fasta \
    -a lambda -r 0.3 --preserve-taxonomy

# Build LAMBDA index
lambda_indexer -d swissprot.reduced.fasta -i swissprot.lambda
```

### 2. Prepare NCBI NR for Diamond
```bash
# Reduce NR database for Diamond
talaria reduce -i nr.fasta -o nr.diamond.fasta \
    -a diamond -r 0.4 --min-length 30

# Build Diamond database
diamond makedb --in nr.diamond.fasta -d nr.diamond
```

### 3. Create Kraken Database
```bash
# Reduce with taxonomic awareness
talaria reduce -i refseq.fasta -o kraken_db.fasta \
    -a kraken --preserve-taxonomy --target-ratio 0.25

# Add to Kraken database
kraken2-build --add-to-library kraken_db.fasta --db kraken2_db
```

### 4. Validate Reduction Quality
```bash
# Check reduction quality
talaria validate -o original.fasta -r reduced.fasta -d deltas.json

# Compare alignment results
talaria validate -o original.fasta -r reduced.fasta \
    --original-results original.m8 --reduced-results reduced.m8
```

### 5. Reconstruct Original Sequences
```bash
# Reconstruct all sequences
talaria reconstruct -r references.fasta -d deltas.json -o reconstructed.fasta

# Reconstruct specific sequences
talaria reconstruct -r references.fasta -d deltas.json \
    --sequences seq1,seq2,seq3 -o subset.fasta
```
"#;

const FAQ_DOC: &str = r#"
# Frequently Asked Questions

## Q: How much reduction is safe?

A: It depends on your use case:
- **High sensitivity needed**: Use 0.5-0.7 (50-70% of original)
- **Balanced**: Use 0.3-0.4 (30-40% of original)
- **Maximum reduction**: Use 0.1-0.2 (10-20% of original)

## Q: Which aligner optimization should I use?

A: Choose based on your downstream aligner:
- LAMBDA → `--aligner lambda`
- BLAST/BLAST+ → `--aligner blast`
- Diamond → `--aligner diamond`
- Kraken/Kraken2 → `--aligner kraken`
- MMseqs2 → `--aligner mmseqs2`
- Unsure → `--aligner generic`

## Q: Why preserve taxonomy?

A: Taxonomic preservation ensures:
- Representative coverage of all organisms
- Accurate taxonomic classification
- Prevention of bias toward overrepresented species

## Q: Can I recover original sequences?

A: Yes! Talaria stores deltas that allow perfect reconstruction:
```bash
talaria reconstruct -r refs.fasta -d deltas.json -o original.fasta
```

## Q: How does this compare to CD-HIT?

A: Key differences:
- **Talaria**: Aligner-aware optimization, delta encoding, taxonomy preservation
- **CD-HIT**: Generic clustering, no reconstruction, simpler algorithm

## Q: Memory requirements?

A: Roughly:
- Input file size × 3 for processing
- Reduced file size × 2 for indexing
- Example: 10GB input → 30GB RAM for reduction

## Q: Can I use multiple threads?

A: Yes! Use `-j` or `--threads`:
```bash
talaria reduce -i input.fasta -o output.fasta -j 16
```

## Q: What about distributed processing?

A: Currently single-node only. Distributed processing planned for v2.0.
For large files (>200GB), consider splitting by taxonomy first.
"#;
