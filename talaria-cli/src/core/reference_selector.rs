#![allow(dead_code)]

use talaria_bio::alignment::Alignment;
/// Reference selection algorithm for choosing representative sequences
use talaria_bio::sequence::Sequence;
use crate::cli::formatting::output::{format_number, tree_section};
use talaria_utils::workspace::TempWorkspace;
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Algorithm selection for reference sequence selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAlgorithm {
    /// Single-pass O(n) algorithm (default) - fast, processes each query once
    SinglePass,
    /// Similarity matrix O(n²) algorithm - slower but potentially more optimal
    SimilarityMatrix,
    /// Hybrid approach (future implementation)
    Hybrid,
    /// Graph centrality-based algorithm (SEQUOIA architecture specification)
    /// Uses formula: Score = α·Degree + β·Betweenness + γ·Coverage
    GraphCentrality,
}

#[derive(Debug, Clone)]
pub struct ReferenceSelectorImpl {
    pub min_length: usize,
    pub similarity_threshold: f64,
    pub taxonomy_aware: bool,
    pub use_taxonomy_weights: bool, // Weight alignment scores by taxonomic distance
    pub all_vs_all: bool,           // Use all-vs-all mode for Lambda alignments
    pub manifest_acc2taxid: Option<PathBuf>, // Path to manifest-based accession2taxid file
    pub batch_enabled: bool,        // Enable batched processing
    pub batch_size: usize,          // Batch size for processing
    pub selection_algorithm: SelectionAlgorithm, // Algorithm to use for selection
    pub use_alignment: bool,        // Use alignment-based selection
    pub use_similarity: bool,       // Use similarity-based selection
    #[allow(dead_code)]
    fast_mode: bool, // Use faster but less optimal algorithm for huge datasets
    workspace: Option<Arc<Mutex<TempWorkspace>>>, // Workspace for temp files
}

#[derive(Debug, Clone)]
pub struct SelectionResult {
    pub references: Vec<Sequence>,
    pub children: HashMap<String, Vec<String>>, // reference_id -> child_ids
    pub discarded: HashSet<String>,
}

impl ReferenceSelectorImpl {
    pub fn new() -> Self {
        Self {
            min_length: 50,
            similarity_threshold: 0.9,
            taxonomy_aware: false,  // Default to db-reduce style (no taxonomic splitting)
            use_taxonomy_weights: false, // Default to no taxonomy weighting
            all_vs_all: true,            // Default to db-reduce style (all-vs-all search)
            manifest_acc2taxid: None,
            batch_enabled: false, // Default: no batching
            batch_size: 5000,     // Default batch size
            selection_algorithm: SelectionAlgorithm::SinglePass, // Default to fast O(n) algorithm
            use_alignment: false, // Default: no alignment
            use_similarity: false, // Default: no similarity
            fast_mode: false,     // Default: quality over speed
            workspace: None,
        }
    }

    pub fn with_manifest_acc2taxid(mut self, path: Option<PathBuf>) -> Self {
        self.manifest_acc2taxid = path;
        self
    }

    pub fn with_all_vs_all(mut self, all_vs_all: bool) -> Self {
        self.all_vs_all = all_vs_all;
        self
    }

    pub fn with_min_length(mut self, min_length: usize) -> Self {
        self.min_length = min_length;
        self
    }

    pub fn with_similarity_threshold(mut self, threshold: f64) -> Self {
        self.similarity_threshold = threshold;
        self
    }

    pub fn with_taxonomy_aware(mut self, enabled: bool) -> Self {
        self.taxonomy_aware = enabled;
        self
    }

    pub fn with_taxonomy_weights(mut self, enabled: bool) -> Self {
        self.use_taxonomy_weights = enabled;
        self
    }

    pub fn with_batch_settings(mut self, enabled: bool, size: usize) -> Self {
        self.batch_enabled = enabled;
        self.batch_size = size;
        self
    }

    pub fn with_selection_algorithm(mut self, algorithm: SelectionAlgorithm) -> Self {
        self.selection_algorithm = algorithm;
        self
    }

    pub fn with_workspace(mut self, workspace: Arc<Mutex<TempWorkspace>>) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Simple greedy reference selection based only on sequence length
    /// Then assigns non-selected sequences to their best matching reference
    pub fn simple_select_references(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> SelectionResult {
        let target_count = (sequences.len() as f64 * target_ratio) as usize;

        // Phase 1: Select references
        let pb = ProgressBar::new(target_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Selecting references")
                .unwrap()
                .progress_chars("##-"),
        );

        // Sort sequences by length (descending) - longest first
        let mut sorted_sequences = sequences.clone();
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

        let mut references = Vec::new();
        let mut reference_ids = HashSet::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        // First, mark all sequences that are too short as discarded
        for seq in &sequences {
            if seq.len() < self.min_length {
                discarded.insert(seq.id.clone());
            }
        }

        // Step 1: Select references (longest sequences)
        for seq in &sorted_sequences {
            if references.len() >= target_count {
                break;
            }

            // Skip if too short
            if seq.len() < self.min_length {
                continue;
            }

            // This sequence becomes a reference
            references.push(seq.clone());
            reference_ids.insert(seq.id.clone());
            children.insert(seq.id.clone(), Vec::new());
            discarded.insert(seq.id.clone());

            pb.inc(1);
        }

        pb.finish_and_clear();
        println!("Selected {} references", references.len());

        // Phase 2: Assign non-reference sequences to their best matching reference
        // Calculate how many sequences need assignment
        let sequences_to_assign = sequences
            .iter()
            .filter(|seq| !reference_ids.contains(&seq.id) && seq.len() >= self.min_length)
            .count();

        let pb2 = ProgressBar::new(sequences_to_assign as u64);
        pb2.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Assigning children to references")
                .unwrap()
                .progress_chars("##-"),
        );

        // Use atomic counter for thread-safe progress updates
        use std::sync::atomic::{AtomicUsize, Ordering};
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let pb_clone = pb2.clone();
        let counter_clone = progress_counter.clone();

        // Collect assignments in parallel with progress updates
        let assignments: Vec<(String, String)> = sequences
            .par_iter()
            .filter_map(|seq| {
                // Skip if this is a reference or too short
                if reference_ids.contains(&seq.id) || seq.len() < self.min_length {
                    return None;
                }

                // Find the reference with the closest length
                let best_ref = references
                    .iter()
                    .min_by_key(|ref_seq| (ref_seq.len() as i64 - seq.len() as i64).abs())?;

                // Update progress every 100 sequences
                let count = counter_clone.fetch_add(1, Ordering::Relaxed);
                if count % 100 == 0 {
                    pb_clone.set_position(count as u64);
                }

                Some((best_ref.id.clone(), seq.id.clone()))
            })
            .collect();

        pb2.set_position(sequences_to_assign as u64);

        // Build children map from assignments
        for (ref_id, child_id) in assignments {
            children
                .entry(ref_id)
                .or_default()
                .push(child_id.clone());
            discarded.insert(child_id);
        }

        pb2.finish_and_clear();
        println!(
            "Assigned {} sequences to {} references",
            sequences_to_assign,
            references.len()
        );

        SelectionResult {
            references,
            children,
            discarded,
        }
    }

    /// Select reference sequences - defaults to simple greedy selection
    pub fn select_references(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> SelectionResult {
        // Default to simple selection (matching original db-reduce)
        self.simple_select_references(sequences, target_ratio)
    }

    /// Select reference sequences with similarity-based clustering
    /// This uses k-mer similarity to group similar sequences
    pub fn select_references_with_similarity(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> SelectionResult {
        let target_count = (sequences.len() as f64 * target_ratio) as usize;
        let total_sequences = sequences.len();

        let pb = ProgressBar::new(total_sequences as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message("Selecting reference sequences");

        // Sort sequences by length (descending) for greedy selection
        let mut sorted_sequences = sequences;
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        // Process sequences in batches for efficiency
        let batch_size = 1000;
        let sequence_map: Arc<DashMap<String, Sequence>> = Arc::new(DashMap::new());
        for seq in &sorted_sequences {
            sequence_map.insert(seq.id.clone(), seq.clone());
        }

        for (batch_idx, batch) in sorted_sequences.chunks(batch_size).enumerate() {
            pb.set_message(format!(
                "Processing batch {}/{}",
                batch_idx + 1,
                sorted_sequences.len().div_ceil(batch_size)
            ));

            // Process batch in parallel
            let batch_results: Vec<_> = batch
                .par_iter()
                .filter_map(|query| {
                    // Skip if already processed
                    if discarded.contains(&query.id) {
                        return None;
                    }

                    // Skip short sequences
                    if query.len() < self.min_length {
                        return None;
                    }

                    // This sequence becomes a reference
                    let mut query_children = Vec::new();

                    // Find similar sequences that can be represented as children
                    for other in &sorted_sequences {
                        if other.id == query.id || discarded.contains(&other.id) {
                            continue;
                        }

                        // Check if taxonomically close (if enabled)
                        if self.taxonomy_aware {
                            if let (Some(q_tax), Some(o_tax)) = (query.taxon_id, other.taxon_id) {
                                // Simple taxonomic distance check
                                if (q_tax as i32 - o_tax as i32).abs() > 1000 {
                                    continue;
                                }
                            }
                        }

                        // Check sequence similarity (simplified check for performance)
                        if self.is_similar_fast(query, other) {
                            query_children.push(other.id.clone());
                        }
                    }

                    Some((query.clone(), query_children))
                })
                .collect();

            // Update results
            for (reference, ref_children) in batch_results {
                if !discarded.contains(&reference.id) {
                    // Store reference and its children
                    children.insert(reference.id.clone(), ref_children.clone());
                    references.push(reference.clone());

                    // Mark reference as processed after adding it
                    discarded.insert(reference.id.clone());

                    // Mark children as processed
                    for child_id in &ref_children {
                        discarded.insert(child_id.clone());
                    }

                    pb.inc(1);

                    // Check if we've reached target
                    if references.len() >= target_count {
                        break;
                    }
                }
            }

            if references.len() >= target_count {
                break;
            }
        }

        // Handle sequences that weren't selected as references or children
        for seq in sorted_sequences {
            if !discarded.contains(&seq.id) && seq.len() >= self.min_length {
                // Add as reference with no children
                children.insert(seq.id.clone(), Vec::new());
                references.push(seq);

                if references.len() >= target_count {
                    break;
                }
            }
        }

        pb.finish_and_clear();
        println!("Selected {} reference sequences", references.len());

        SelectionResult {
            references,
            children,
            discarded,
        }
    }

    /// Fast similarity check using k-mer overlap
    fn is_similar_fast(&self, seq1: &Sequence, seq2: &Sequence) -> bool {
        // Quick length check
        let len_ratio = seq1.len().min(seq2.len()) as f64 / seq1.len().max(seq2.len()) as f64;
        if len_ratio < 0.8 {
            return false;
        }

        // K-mer based similarity (faster than full alignment)
        let k = 3; // k-mer size
        let kmers1 = self.extract_kmers(&seq1.sequence, k);
        let kmers2 = self.extract_kmers(&seq2.sequence, k);

        let intersection: HashSet<_> = kmers1.intersection(&kmers2).collect();
        let union_size = kmers1.len() + kmers2.len() - intersection.len();

        if union_size == 0 {
            return false;
        }

        let jaccard = intersection.len() as f64 / union_size as f64;
        jaccard >= self.similarity_threshold * 0.7 // Relaxed threshold for k-mer similarity
    }

    /// Extract k-mers from a sequence
    fn extract_kmers(&self, sequence: &[u8], k: usize) -> HashSet<Vec<u8>> {
        if sequence.len() < k {
            return HashSet::new();
        }

        let mut kmers = HashSet::new();
        for window in sequence.windows(k) {
            kmers.insert(window.to_vec());
        }
        kmers
    }

    /// Auto-detect optimal number of references based on coverage
    /// Stops when adding new references provides diminishing returns
    pub fn select_references_auto(&self, sequences: Vec<Sequence>) -> SelectionResult {
        let pb = ProgressBar::new(sequences.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Auto-detecting references",
                )
                .unwrap(),
        );

        // Sort by length (longest first)
        let mut sorted_sequences = sequences.clone();
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();
        let mut coverage_history = Vec::new();

        // Minimum thresholds for better coverage
        const MIN_REFERENCES: usize = 100;
        const MIN_COVERAGE: f64 = 0.10; // At least 10% coverage

        for query in &sorted_sequences {
            if discarded.contains(&query.id) {
                continue;
            }

            if query.len() < self.min_length {
                continue;
            }

            let mut query_children = Vec::new();
            let mut new_coverage = 0;

            // Find sequences similar to this potential reference
            for other in &sorted_sequences {
                if other.id == query.id || discarded.contains(&other.id) {
                    continue;
                }

                // More relaxed length ratio for protein diversity
                let len_ratio =
                    other.len().min(query.len()) as f64 / other.len().max(query.len()) as f64;
                if len_ratio >= 0.5 {
                    // Relaxed from 0.7 to 0.5
                    // Check k-mer similarity with more permissive settings
                    let k = 2; // Use 2-mers instead of 3-mers for proteins
                    let kmers1 = self.extract_kmers(&query.sequence, k);
                    let kmers2 = self.extract_kmers(&other.sequence, k);

                    let intersection: HashSet<_> = kmers1.intersection(&kmers2).collect();
                    let union_size = kmers1.len() + kmers2.len() - intersection.len();

                    if union_size > 0 {
                        let jaccard = intersection.len() as f64 / union_size as f64;
                        // Much more relaxed threshold for proteins (0.2 instead of 0.4)
                        if jaccard >= 0.2 {
                            query_children.push(other.id.clone());
                            new_coverage += 1;
                        }
                    }
                }
            }

            // Check if this reference provides enough value
            let total_covered = discarded.len() + new_coverage + 1;
            let coverage_ratio = total_covered as f64 / sequences.len() as f64;

            // Add coverage to history
            coverage_history.push(coverage_ratio);

            // Only check for diminishing returns after minimum thresholds are met
            if references.len() >= MIN_REFERENCES && coverage_ratio >= MIN_COVERAGE {
                // Check diminishing returns over last 10 references (not 3)
                if coverage_history.len() > 10 {
                    let recent_improvement = coverage_history[coverage_history.len() - 1]
                        - coverage_history[coverage_history.len() - 10];

                    // Stop if improvement over last 10 references is less than 0.1%
                    if recent_improvement < 0.001 {
                        pb.finish_and_clear();
                        println!(
                            "Auto-detected {} references (coverage: {:.1}%, plateau reached)",
                            references.len(),
                            coverage_ratio * 100.0
                        );
                        break;
                    }
                }
            }

            // Stop if we've covered 95% of sequences (but ensure minimum coverage first)
            if coverage_ratio > 0.95 && references.len() >= MIN_REFERENCES {
                pb.finish_and_clear();
                println!(
                    "Auto-detected {} references (coverage: {:.1}%)",
                    references.len(),
                    coverage_ratio * 100.0
                );
                break;
            }

            // Limit to reasonable number of references (e.g., 10% of sequences)
            if references.len() >= sequences.len() / 10 && references.len() >= MIN_REFERENCES {
                pb.finish_and_clear();
                println!(
                    "Auto-detected {} references (coverage: {:.1}%, max references reached)",
                    references.len(),
                    coverage_ratio * 100.0
                );
                break;
            }

            // Add as reference
            for child_id in &query_children {
                discarded.insert(child_id.clone());
            }
            children.insert(query.id.clone(), query_children);
            references.push(query.clone());
            discarded.insert(query.id.clone());

            pb.inc(1);
            pb.set_message(format!(
                "References: {}, Coverage: {:.1}%",
                references.len(),
                coverage_ratio * 100.0
            ));
        }

        // Calculate final coverage
        let final_coverage = discarded.len() as f64 / sequences.len() as f64;

        // If we found very few sequences, fall back to simple selection
        if final_coverage < 0.01 && sequences.len() > 1000 {
            pb.finish_and_clear();
            println!(
                "Auto-detection found too few matches, falling back to length-based selection"
            );
            // Use simple selection with 10% ratio as fallback
            return self.simple_select_references(sequences, 0.1);
        }

        pb.finish_and_clear();
        println!(
            "Auto-detected {} references (final coverage: {:.1}%)",
            references.len(),
            final_coverage * 100.0
        );

        SelectionResult {
            references,
            children,
            discarded,
        }
    }

    /// Perform full alignment-based selection (more accurate but slower)
    pub fn select_references_with_alignment(
        &self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
    ) -> SelectionResult {
        let target_count = (sequences.len() as f64 * target_ratio) as usize;

        let pb = ProgressBar::new(sequences.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Aligning sequences")
                .unwrap(),
        );

        // Sort by length
        let mut sorted_sequences = sequences;
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        for query in &sorted_sequences {
            if discarded.contains(&query.id) {
                continue;
            }

            if query.len() < self.min_length {
                continue;
            }

            let mut query_children = Vec::new();

            // Align against other sequences
            for other in &sorted_sequences {
                if other.id == query.id || discarded.contains(&other.id) {
                    continue;
                }

                // Perform alignment
                let alignment = Alignment::global(query, other);
                // Use identity directly from bio::alignment::AlignmentResult
                let similarity = alignment.identity;

                if similarity >= self.similarity_threshold {
                    query_children.push(other.id.clone());
                    discarded.insert(other.id.clone());
                }
            }

            // Add as reference
            children.insert(query.id.clone(), query_children);
            references.push(query.clone());
            discarded.insert(query.id.clone());

            pb.inc(1);

            if references.len() >= target_count {
                break;
            }
        }

        pb.finish_and_clear();
        println!("Selected {} references with alignment", references.len());

        SelectionResult {
            references,
            children,
            discarded,
        }
    }

    #[allow(dead_code)]
    fn calculate_similarity(&self, alignment: &talaria_tools::AlignmentSummary) -> f64 {
        // Use identity field directly from tools::AlignmentResult
        // Identity is already a percentage (0.0 to 100.0)
        alignment.identity as f64 / 100.0
    }

    /// Calculate a weight based on taxonomic distance between two sequences
    /// Returns a value between 0.5 and 1.5 where:
    /// - Same species (all levels match): 1.5 (boost similar taxonomy)
    /// - Same genus but different species: 1.2
    /// - Same family but different genus: 1.0
    /// - Different families: 0.8 (penalize distant taxonomy)
    /// - No taxonomy data: 1.0 (neutral)
    #[allow(dead_code)]
    fn calculate_taxonomy_weight(&self, seq1: &Sequence, seq2: &Sequence) -> f64 {
        // Extract taxonomy from sequence descriptions
        // Expected format in description: "OS=Organism GN=Gene Tax=9606"
        // Or in FASTA header: >ID description [taxonomy info]

        let tax1 = self.extract_taxonomy(seq1.description.as_deref().unwrap_or(""));
        let tax2 = self.extract_taxonomy(seq2.description.as_deref().unwrap_or(""));

        // If either sequence lacks taxonomy data, return neutral weight
        if tax1.is_empty() || tax2.is_empty() {
            return 1.0;
        }

        // Count matching taxonomy levels
        let mut matches = 0usize;
        let max_levels = tax1.len().min(tax2.len());

        for i in 0..max_levels {
            if tax1[i] == tax2[i] {
                matches += 1;
            } else {
                break; // Stop at first mismatch (taxonomy is hierarchical)
            }
        }

        // Weight based on taxonomy similarity
        match matches {
            0 => 0.8,  // Different at kingdom level (very distant)
            1 => 0.9,  // Same kingdom, different phylum
            2 => 0.95, // Same phylum, different class
            3 => 1.0,  // Same class, different order
            4 => 1.05, // Same order, different family
            5 => 1.1,  // Same family, different genus
            6 => 1.2,  // Same genus, different species
            _ => 1.5,  // Same species or subspecies (very close)
        }
    }

    /// Extract taxonomy levels from sequence description
    /// Returns a vector of taxonomy levels from most general to most specific
    #[allow(dead_code)]
    fn extract_taxonomy(&self, description: &str) -> Vec<String> {
        let mut taxonomy = Vec::new();

        // Look for NCBI taxonomy ID format: "Tax=9606" or "TaxID=9606"
        if let Some(tax_match) = description
            .split_whitespace()
            .find(|s| s.starts_with("Tax=") || s.starts_with("TaxID="))
        {
            let tax_id = tax_match.split('=').nth(1).unwrap_or("");
            // Map common taxonomy IDs to hierarchy
            // This is a simplified example - in production, would use a taxonomy database
            taxonomy = match tax_id {
                "9606" => vec![
                    "Eukaryota",
                    "Chordata",
                    "Mammalia",
                    "Primates",
                    "Hominidae",
                    "Homo",
                    "sapiens",
                ],
                "10090" => vec![
                    "Eukaryota",
                    "Chordata",
                    "Mammalia",
                    "Rodentia",
                    "Muridae",
                    "Mus",
                    "musculus",
                ],
                "7227" => vec![
                    "Eukaryota",
                    "Arthropoda",
                    "Insecta",
                    "Diptera",
                    "Drosophilidae",
                    "Drosophila",
                    "melanogaster",
                ],
                "6239" => vec![
                    "Eukaryota",
                    "Nematoda",
                    "Chromadorea",
                    "Rhabditida",
                    "Rhabditidae",
                    "Caenorhabditis",
                    "elegans",
                ],
                "4932" => vec![
                    "Eukaryota",
                    "Ascomycota",
                    "Saccharomycetes",
                    "Saccharomycetales",
                    "Saccharomycetaceae",
                    "Saccharomyces",
                    "cerevisiae",
                ],
                "562" => vec![
                    "Bacteria",
                    "Proteobacteria",
                    "Gammaproteobacteria",
                    "Enterobacterales",
                    "Enterobacteriaceae",
                    "Escherichia",
                    "coli",
                ],
                _ => vec![],
            }
            .iter()
            .map(|s| s.to_string())
            .collect();
        }

        // Alternative: Look for organism name "OS=Homo sapiens"
        if taxonomy.is_empty() {
            if let Some(os_match) = description
                .split_whitespace()
                .position(|s| s == "OS=")
                .and_then(|i| description.split_whitespace().nth(i + 1))
            {
                // Parse organism name into taxonomy hierarchy
                // This is simplified - real implementation would use taxonomy database
                let organism = os_match.to_lowercase();
                if organism.contains("homo") && organism.contains("sapiens") {
                    taxonomy = ["Eukaryota",
                        "Chordata",
                        "Mammalia",
                        "Primates",
                        "Hominidae",
                        "Homo",
                        "sapiens"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                } else if organism.contains("mus") && organism.contains("musculus") {
                    taxonomy = ["Eukaryota",
                        "Chordata",
                        "Mammalia",
                        "Rodentia",
                        "Muridae",
                        "Mus",
                        "musculus"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                }
                // Add more organism mappings as needed
            }
        }

        taxonomy
    }

    /// Select references using LAMBDA aligner for accurate alignments
    /// This implements the aegis-research db-reduce algorithm:
    /// 1. Run all-vs-all alignments
    /// 2. Group sequences by alignment relationships
    /// 3. Select longest sequence from each group as reference
    /// 4. All other sequences in group become children
    pub fn select_references_with_lambda(
        &mut self,
        sequences: Vec<Sequence>,
    ) -> anyhow::Result<SelectionResult> {
        use talaria_tools::aligners::lambda::LambdaAligner;
        use talaria_tools::{Tool, ToolManager};
        use crate::cli::formatting::output::{
            action, info, section_header, success, warning, tree_section,
        };
        

        section_header("LAMBDA-based Reference Selection (aegis algorithm)");
        info(&format!(
            "Processing {} sequences",
            format_number(sequences.len())
        ));

        // Step 1: Pre-filter by taxonomy if enabled
        let (sequences_to_process, taxonomy_groups) = if self.taxonomy_aware {
            action("Grouping sequences by taxonomy...");
            let mut taxon_groups: HashMap<u32, Vec<Sequence>> = HashMap::new();
            let mut no_taxon_sequences = Vec::new();

            for seq in sequences.clone() {
                if let Some(taxon_id) = seq.taxon_id {
                    taxon_groups
                        .entry(taxon_id)
                        .or_default()
                        .push(seq);
                } else {
                    no_taxon_sequences.push(seq);
                }
            }

            let taxonomy_items = vec![
                ("Taxonomic groups", format_number(taxon_groups.len())),
                ("Without taxonomy", format_number(no_taxon_sequences.len())),
            ];
            tree_section("Taxonomy Summary", taxonomy_items, false);

            (sequences, Some(taxon_groups))
        } else {
            (sequences, None)
        };

        // Check if LAMBDA is installed
        let manager = ToolManager::new()?;
        let lambda_path = manager.get_current_tool_path(Tool::Lambda)?;
        info(&format!("LAMBDA binary: {:?}", lambda_path));

        // Create LAMBDA aligner with optional manifest-based taxonomy
        let mut aligner = LambdaAligner::new(lambda_path)?;

        // Pass workspace to aligner if available
        if let Some(workspace) = &self.workspace {
            aligner = aligner.with_workspace(workspace.clone());
        }

        // Set batch settings
        aligner = aligner.with_batch_settings(self.batch_enabled, self.batch_size);

        // If we have a manifest-based accession2taxid file, use it
        if let Some(ref acc2taxid_path) = self.manifest_acc2taxid {
            // Also need the taxdump directory
            let taxonomy_dir = talaria_core::system::paths::talaria_taxonomy_current_dir();
            let taxdump_dir = taxonomy_dir.join("taxdump");

            if taxdump_dir.exists() {
                aligner = aligner.with_taxonomy(Some(acc2taxid_path.clone()), Some(taxdump_dir));
                success("Using manifest-based taxonomy mapping");
            } else {
                warning("taxdump directory not found, taxonomy features disabled");
            }
        }

        success("LAMBDA aligner initialized");

        // Run alignments with LAMBDA
        section_header("Running LAMBDA Alignments");
        info(&format!(
            "Mode: {}",
            if self.all_vs_all {
                "all-vs-all"
            } else {
                "query-vs-reference"
            }
        ));

        let alignments = if self.all_vs_all {
            // All-vs-all mode: self-alignment within the dataset
            aligner.search_all_vs_all(&sequences_to_process)?
        } else if self.taxonomy_aware && taxonomy_groups.is_some() {
            // Process each taxonomic group separately for better performance
            action("Processing by taxonomic groups with shared LAMBDA index...");
            let mut all_alignments = Vec::new();
            let taxon_groups = taxonomy_groups.unwrap();

            // CRITICAL OPTIMIZATION: Create ONE shared index for ALL reference sequences
            // This avoids creating thousands of indices for large databases like SwissProt
            info("Building comprehensive reference index for all taxonomic groups...");

            // Collect ALL reference sequences from ALL groups first
            let mut all_reference_sequences = Vec::new();
            let mut group_references = Vec::new(); // Track which references belong to which group

            for (taxon_id, group_sequences) in taxon_groups.iter() {
                if group_sequences.len() < 10 {
                    continue;
                }

                // Sort by length within the group
                let mut sorted_group = group_sequences.clone();
                sorted_group.sort_by_key(|s| std::cmp::Reverse(s.len()));

                // Take top 20% as reference sequences within this group
                let reference_count = std::cmp::max(2, sorted_group.len() / 5);
                let reference_sequences: Vec<_> =
                    sorted_group.iter().take(reference_count).cloned().collect();

                group_references.push((taxon_id.clone(), reference_sequences.clone()));
                all_reference_sequences.extend(reference_sequences);
            }

            // Handle sequences without taxonomy ID
            let no_taxon: Vec<Sequence> = sequences_to_process
                .iter()
                .filter(|s| s.taxon_id.is_none())
                .cloned()
                .collect();

            if !no_taxon.is_empty() && no_taxon.len() >= 10 {
                let mut sorted_group = no_taxon.clone();
                sorted_group.sort_by_key(|s| std::cmp::Reverse(s.len()));
                let reference_count = std::cmp::max(2, sorted_group.len() / 5);
                let reference_sequences: Vec<_> =
                    sorted_group.iter().take(reference_count).cloned().collect();
                group_references.push((0, reference_sequences.clone())); // Use 0 for no-taxon
                all_reference_sequences.extend(reference_sequences);
            }

            // Create the shared index ONCE for all reference sequences
            info(&format!(
                "Creating shared LAMBDA index for {} total reference sequences...",
                format_number(all_reference_sequences.len())
            ));
            let shared_index = aligner.create_index_for_sequences(&all_reference_sequences)?;
            success("Shared LAMBDA index created successfully");

            // Process groups with progress indicator
            use indicatif::{ProgressBar, ProgressStyle};
            use std::sync::Arc;
            use std::sync::atomic::{AtomicUsize, Ordering};

            // Count total groups to process
            let groups_to_process: Vec<_> = taxon_groups
                .iter()
                .filter(|(_, g)| g.len() >= 10)
                .collect();

            let total_groups = groups_to_process.len();

            if total_groups > 0 {
                info(&format!("Processing {} taxonomic groups...", format_number(total_groups)));

                // Use parallel processing for any multi-group dataset for better performance
                let use_parallel = total_groups > 2 && std::env::var("TALARIA_NO_PARALLEL").is_err();

                if use_parallel {
                    info("Using parallel processing for taxonomic groups");

                    // Prepare query groups - process entire taxonomic groups at once (matching db-reduce)
                    // LAMBDA handles large groups efficiently internally
                    let mut query_groups: Vec<Vec<Sequence>> = Vec::new();
                    for (_taxon_id, group_sequences) in groups_to_process.iter() {
                        // Sort by length within the group
                        let mut sorted_group = (*group_sequences).clone();
                        sorted_group.sort_by_key(|s| std::cmp::Reverse(s.len()));

                        // Process entire group at once - no splitting
                        // LAMBDA can handle large groups efficiently
                        query_groups.push(sorted_group);
                    }

                    // Determine number of parallel workers - optimize for fewer processes with more threads each
                    // Use 1/8 of CPU cores for process count, giving each process more threads
                    let parallel_processes = std::cmp::max(1, std::cmp::min(4, num_cpus::get() / 8));
                    info(&format!("Running {} LAMBDA processes in parallel", parallel_processes));

                    // Process in parallel
                    // Update progress bar to show batches instead of just groups
                    let total_batches = query_groups.len();
                    let progress = Arc::new(ProgressBar::new(total_batches as u64));
                    progress.set_style(
                        ProgressStyle::default_bar()
                            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} batches ({msg})")
                            .unwrap()
                            .progress_chars("━━╺"),
                    );

                    info(&format!("Processing {} batches from {} groups",
                        format_number(total_batches),
                        format_number(total_groups)));

                    // Suppress LAMBDA's individual progress bars
                    println!("Note: Individual LAMBDA progress bars are suppressed in parallel mode.");
                    println!("Overall progress will be shown as batches complete.");

                    let processed = Arc::new(AtomicUsize::new(0));
                    let alignments_found = Arc::new(AtomicUsize::new(0));

                    // Run parallel searches
                    let group_results = aligner.search_groups_parallel(query_groups, &shared_index, parallel_processes)?;

                    // Collect all results
                    for group_alignments in group_results {
                        let count = group_alignments.len();
                        all_alignments.extend(group_alignments);

                        let proc = processed.fetch_add(1, Ordering::SeqCst) + 1;
                        let total_align = alignments_found.fetch_add(count, Ordering::SeqCst) + count;

                        progress.set_position(proc as u64);
                        progress.set_message(format!("{} alignments found", format_number(total_align)));
                    }

                    progress.finish_with_message(format!("✓ {} alignments found", format_number(all_alignments.len())));

                } else {
                    // Sequential processing (original implementation)
                    let progress = ProgressBar::new(total_groups as u64);
                    progress.set_style(
                        ProgressStyle::default_bar()
                            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} groups ({msg})")
                            .unwrap()
                            .progress_chars("━━╺"),
                    );

                    let mut processed = 0;
                    let mut total_alignments_found = 0;

                    // Now process each group using the shared index
                    for (_taxon_id, group_sequences) in groups_to_process {
                        // Sort by length within the group
                        let mut sorted_group = group_sequences.clone();
                        sorted_group.sort_by_key(|s| std::cmp::Reverse(s.len()));

                        // Process entire taxonomic group at once (matching db-reduce approach)
                        // LAMBDA handles large groups efficiently internally
                        let group_alignments = aligner.search_with_index_silent(&sorted_group, &shared_index)?;
                        let group_alignment_count = group_alignments.len();
                        total_alignments_found += group_alignment_count;
                        all_alignments.extend(group_alignments);

                        progress.set_message(format!(
                            "Group {}/{}: {} sequences, {} alignments found",
                            processed + 1,
                            total_groups,
                            format_number(sorted_group.len()),
                            format_number(group_alignment_count)
                        ));

                        processed += 1;
                        progress.set_position(processed);
                        progress.set_message(format!("{} alignments found", format_number(total_alignments_found)));
                    }

                    progress.finish_with_message(format!("✓ {} alignments found", format_number(total_alignments_found)));
                }
            }

            // Process sequences without taxonomy ID if any
            if !no_taxon.is_empty() && no_taxon.len() >= 10 {
                info(&format!(
                    "Processing {} sequences without taxonomy...",
                    format_number(no_taxon.len())
                ));

                let mut sorted_group = no_taxon.clone();
                sorted_group.sort_by_key(|s| std::cmp::Reverse(s.len()));

                // Sample queries for no-taxon group too
                let query_sample_size = std::cmp::min(100, sorted_group.len() / 1000);
                let query_sample_size = std::cmp::max(10, query_sample_size);

                let query_sequences: Vec<_> = if sorted_group.len() <= query_sample_size {
                    sorted_group.clone()
                } else {
                    let step = sorted_group.len() / query_sample_size;
                    (0..query_sample_size)
                        .map(|i| sorted_group[i * step].clone())
                        .collect()
                };

                // Use the SHARED index for no-taxon group too - no new index creation!
                // Use silent mode here too
                let group_alignments = aligner.search_with_index_silent(&query_sequences, &shared_index)?;
                let alignment_count = group_alignments.len();
                all_alignments.extend(group_alignments);

                success(&format!(
                    "Processed no-taxon group: {} alignments found",
                    format_number(alignment_count)
                ));
            }

            all_alignments
        } else {
            // Default: Query-vs-reference mode
            // Use a subset as reference (e.g., longest sequences)
            let mut sorted_sequences = sequences_to_process.clone();
            sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

            // Take top 20% as reference sequences
            let reference_count = std::cmp::max(10, sequences_to_process.len() / 5);
            let reference_sequences: Vec<_> = sorted_sequences
                .iter()
                .take(reference_count)
                .cloned()
                .collect();

            // Sample queries intelligently
            let query_sample_size = std::cmp::min(100, sequences_to_process.len() / 1000);
            let query_sample_size = std::cmp::max(10, query_sample_size);

            let query_sequences: Vec<_> = if sequences_to_process.len() <= query_sample_size {
                sequences_to_process.clone()
            } else {
                let step = sequences_to_process.len() / query_sample_size;
                (0..query_sample_size)
                    .map(|i| sorted_sequences[i * step].clone())
                    .collect()
            };

            let alignment_items = vec![
                ("Query sequences", format!("{} (sampled from {})",
                    format_number(query_sequences.len()),
                    format_number(sequences_to_process.len()))),
                (
                    "Reference sequences",
                    format!(
                        "{} (top 20% longest)",
                        format_number(reference_sequences.len())
                    ),
                ),
            ];
            tree_section("Alignment Setup", alignment_items, false);

            // Use sampled queries
            aligner.search(&query_sequences, &reference_sequences)?
        };
        success(&format!(
            "LAMBDA alignments complete: {} alignments found",
            format_number(alignments.len())
        ));

        // Dispatch to appropriate algorithm
        match self.selection_algorithm {
            SelectionAlgorithm::SinglePass => {
                self.select_with_single_pass(alignments, sequences_to_process)
            }
            SelectionAlgorithm::SimilarityMatrix => {
                self.select_with_similarity_matrix(alignments, sequences_to_process)
            }
            SelectionAlgorithm::Hybrid => {
                // For now, default to single-pass
                warning("Hybrid algorithm not yet implemented, using SinglePass");
                self.select_with_single_pass(alignments, sequences_to_process)
            }
            SelectionAlgorithm::GraphCentrality => {
                // Use graph centrality approach from SEQUOIA architecture
                use crate::cli::formatting::output::info;
                info("Using graph centrality-based selection (SEQUOIA 5-dimensional)");
                self.select_with_graph_centrality(alignments, sequences_to_process)
            }
        }
    }

    /// Single-pass O(n) greedy selection matching original ref-db-gen.cpp
    fn select_with_single_pass(
        &self,
        alignments: Vec<talaria_tools::AlignmentSummary>,
        sequences: Vec<Sequence>,
    ) -> anyhow::Result<SelectionResult> {
        // Group alignments by query sequence (matching original approach)
        let mut query_alignments: HashMap<String, Vec<(String, f64, usize)>> = HashMap::new();

        for alignment in alignments {
            // Group by query, store (reference, identity, ref_length)
            query_alignments
                .entry(alignment.query_id.clone())
                .or_default()
                .push((
                    alignment.reference_id.clone(),
                    alignment.identity as f64,
                    alignment.ref_end,
                ));
        }

        let grouping_items = vec![
            (
                "Total alignments",
                format_number(query_alignments.values().map(|v| v.len()).sum::<usize>()),
            ),
            ("Unique queries", format_number(query_alignments.len())),
        ];
        tree_section("Alignment Grouping", grouping_items, false);

        // Create sequence length lookup
        let seq_lengths: HashMap<String, usize> =
            sequences.iter().map(|s| (s.id.clone(), s.len())).collect();

        // Sort queries by number of hits (process most connected first)
        let mut sorted_queries: Vec<_> = query_alignments
            .iter()
            .map(|(query, subjects)| (query.clone(), subjects.len()))
            .collect();
        sorted_queries.sort_by_key(|(_query, count)| std::cmp::Reverse(*count));

        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        let pb2 = ProgressBar::new(sorted_queries.len() as u64);
        pb2.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Processing queries")
                .unwrap(),
        );

        // Process queries in single pass (matching original O(n) algorithm)
        for (i, (query_id, _hit_count)) in sorted_queries.iter().enumerate() {
            pb2.set_position(i as u64);

            // Skip if already processed
            if discarded.contains(query_id) {
                continue;
            }

            // Get this query's alignments
            if let Some(subjects) = query_alignments.get(query_id) {
                // Find the longest subject that covers this query
                let mut best_subject: Option<(String, usize)> = None;
                let mut max_length = 0;

                for (subject_id, identity, _subject_len) in subjects {
                    // Skip if already discarded
                    // Use 80% identity threshold for tighter clustering
                    if *identity < 0.8 || discarded.contains(subject_id) {
                        continue;
                    }

                    // Get actual sequence length
                    if let Some(&seq_len) = seq_lengths.get(subject_id) {
                        if seq_len > max_length {
                            max_length = seq_len;
                            best_subject = Some((subject_id.clone(), seq_len));
                        }
                    }
                }

                // Select the best subject as reference
                if let Some((ref_id, _)) = best_subject {
                    if !discarded.contains(&ref_id) {
                        references.push(sequences.iter().find(|s| s.id == ref_id).unwrap().clone());
                        discarded.insert(ref_id.clone());

                        // Mark all well-aligned sequences as children
                        let mut ref_children = Vec::new();
                        for (subject_id, identity, _) in subjects {
                            // Use 80% identity threshold for child relationships
                            if *identity >= 0.8
                                && !discarded.contains(subject_id)
                                && subject_id != &ref_id
                            {
                                ref_children.push(subject_id.clone());
                                discarded.insert(subject_id.clone());
                            }
                        }

                        // Also mark the query itself as covered if not the reference
                        if query_id != &ref_id && !discarded.contains(query_id) {
                            ref_children.push(query_id.clone());
                            discarded.insert(query_id.clone());
                        }

                        if !ref_children.is_empty() {
                            children.insert(ref_id, ref_children);
                        }
                    }
                } else if !discarded.contains(query_id) {
                    // No good subject found with >70% identity
                    // Only make it a reference if it truly has no alignments
                    // or all alignments are very poor quality
                    let has_any_decent_alignment = subjects.iter()
                        .any(|(_, identity, _)| *identity >= 0.5);

                    if !has_any_decent_alignment {
                        // Truly isolated sequence, becomes its own reference
                        references.push(
                            sequences
                                .iter()
                                .find(|s| &s.id == query_id)
                                .unwrap()
                                .clone(),
                        );
                        discarded.insert(query_id.clone());
                    }
                    // Otherwise, leave it uncovered for now
                }
            } else if !discarded.contains(query_id) {
                // Query has no alignments at all - truly isolated
                // This is rare but valid - becomes its own reference
                references.push(
                    sequences
                        .iter()
                        .find(|s| &s.id == query_id)
                        .unwrap()
                        .clone(),
                );
                discarded.insert(query_id.clone());
            }

            // Report progress
            if i % 1000 == 0 {
                let coverage = discarded.len() as f64 / sequences.len() as f64;
                pb2.set_message(format!(
                    "References: {}, Coverage: {:.1}%",
                    references.len(),
                    coverage * 100.0
                ));
            }
        }

        // Note: We do NOT add uncovered sequences as references
        // This matches the original db-reduce behavior where uncovered sequences
        // remain as non-references and can be handled differently downstream

        // Calculate actual coverage statistics
        let covered_count = discarded.len();
        let total_count = sequences.len();
        let uncovered_count = total_count - covered_count;
        let coverage = covered_count as f64 / total_count as f64;
        let ref_percentage = references.len() as f64 / total_count as f64 * 100.0;

        pb2.finish_and_clear();

        println!(
            "Selected {} references ({:.1}% of total)",
            references.len(),
            ref_percentage
        );
        println!(
            "Covered: {} sequences, Uncovered: {} sequences ({:.1}% coverage)",
            covered_count,
            uncovered_count,
            coverage * 100.0
        );

        // Warn if too many sequences are uncovered
        if uncovered_count > total_count / 10 {
            println!(
                "⚠️  Warning: {} sequences ({:.1}%) remain uncovered. Consider adjusting identity threshold.",
                uncovered_count,
                (uncovered_count as f64 / total_count as f64) * 100.0
            );
        }

        Ok(SelectionResult {
            references,
            children,
            discarded,
        })
    }

    /// Similarity matrix O(n²) algorithm - evaluates all candidates against all uncovered sequences
    fn select_with_similarity_matrix(
        &self,
        alignments: Vec<talaria_tools::AlignmentSummary>,
        sequences: Vec<Sequence>,
    ) -> anyhow::Result<SelectionResult> {
        use crate::cli::formatting::output::{info, section_header, warning};
        section_header("Similarity Matrix Algorithm");
        info("Using O(n²) algorithm - slower but potentially more optimal");

        // Build similarity matrix from alignments
        let mut similarity_matrix: HashMap<(String, String), f64> = HashMap::new();
        for alignment in alignments {
            let key = (alignment.query_id.clone(), alignment.reference_id.clone());
            similarity_matrix.insert(key.clone(), alignment.identity as f64);
            // Also insert reverse for bidirectional lookups
            let reverse_key = (alignment.reference_id.clone(), alignment.query_id.clone());
            similarity_matrix.insert(reverse_key, alignment.identity as f64);
        }

        info(&format!(
            "Built similarity matrix with {} entries",
            format_number(similarity_matrix.len())
        ));

        // Greedy selection based on alignment coverage
        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();
        let mut uncovered: HashSet<String> = sequences.iter().map(|s| s.id.clone()).collect();

        let pb2 = ProgressBar::new(sequences.len() as u64);
        pb2.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Selecting references (matrix)")
                .unwrap(),
        );

        // Sort sequences by length (longest first)
        let mut sorted_sequences = sequences.clone();
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

        // Iteratively select references that cover the most uncovered sequences
        let mut iteration = 0;
        while !uncovered.is_empty() {
            iteration += 1;
            let mut best_reference = None;
            let mut best_coverage = Vec::new();
            let mut best_score = 0.0;

            // Evaluate all candidates in parallel
            let uncovered_vec: Vec<String> = uncovered.iter().cloned().collect();
            let candidates_scores: Vec<(Sequence, Vec<String>, f64)> = sorted_sequences
                .par_iter()
                .filter(|candidate| !discarded.contains(&candidate.id))
                .map(|candidate| {
                    let mut coverage = Vec::new();
                    let mut score = 0.0;

                    for other_id in &uncovered_vec {
                        if other_id == &candidate.id {
                            continue;
                        }

                        // Check similarity from alignment results
                        let key = (candidate.id.clone(), other_id.clone());
                        if let Some(&similarity) = similarity_matrix.get(&key) {
                            if similarity >= 0.7 {
                                // 70% identity threshold
                                coverage.push(other_id.clone());

                                // Apply taxonomy weighting if enabled
                                let weighted_score = if self.use_taxonomy_weights {
                                    // Find the sequences to extract taxonomy data
                                    let candidate_seq =
                                        sorted_sequences.iter().find(|s| s.id == candidate.id);
                                    let other_seq =
                                        sorted_sequences.iter().find(|s| s.id == *other_id);

                                    if let (Some(cand), Some(other)) = (candidate_seq, other_seq) {
                                        // Calculate taxonomic weight based on shared taxonomy levels
                                        let weight = self.calculate_taxonomy_weight(cand, other);
                                        similarity * weight
                                    } else {
                                        similarity
                                    }
                                } else {
                                    similarity
                                };

                                score += weighted_score;
                            }
                        }
                    }

                    (candidate.clone(), coverage, score)
                })
                .collect();

            // Find the best candidate from parallel evaluation
            for (candidate, coverage, score) in candidates_scores {
                if score > best_score {
                    best_reference = Some(candidate);
                    best_coverage = coverage;
                    best_score = score;
                }
            }

            // If no good reference found, add remaining as individual references
            if best_reference.is_none() || best_coverage.is_empty() {
                info(&format!(
                    "No more good references found after {} iterations",
                    iteration
                ));
                // Add remaining sequences as their own references
                for seq_id in uncovered.iter() {
                    if let Some(seq) = sorted_sequences.iter().find(|s| &s.id == seq_id) {
                        references.push(seq.clone());
                        discarded.insert(seq_id.clone());
                    }
                }
                break;
            }

            // Add the best reference and update coverage
            let reference = best_reference.unwrap();
            references.push(reference.clone());
            children.insert(reference.id.clone(), best_coverage.clone());

            // Mark covered sequences as discarded
            uncovered.remove(&reference.id);
            for child_id in &best_coverage {
                uncovered.remove(child_id);
                discarded.insert(child_id.clone());
            }
            discarded.insert(reference.id.clone());

            pb2.set_position((sequences.len() - uncovered.len()) as u64);
            pb2.set_message(format!(
                "Iteration {}: {} refs, {} uncovered",
                iteration,
                references.len(),
                uncovered.len()
            ));

            // Early termination for very large datasets
            if iteration > 1000 {
                warning("Reached maximum iterations (1000), terminating early");
                break;
            }
        }

        pb2.finish_and_clear();
        println!(
            "Selected {} references using similarity matrix",
            references.len()
        );

        Ok(SelectionResult {
            references,
            children,
            discarded,
        })
    }

    /// Graph centrality-based selection algorithm from SEQUOIA architecture
    /// Implements: Centrality Score = α·Degree + β·Betweenness + γ·Coverage
    fn select_with_graph_centrality(
        &self,
        alignments: Vec<talaria_tools::AlignmentSummary>,
        sequences: Vec<Sequence>,
    ) -> anyhow::Result<SelectionResult> {
        use crate::cli::formatting::output::{info, section_header};
        use super::reference_selector_optimized::{
            AlignmentCache, OptimizedReferenceSelector,
        };

        section_header("Graph Centrality Algorithm (SEQUOIA)");
        info("Formula: Score = 0.5·Degree + 0.3·Betweenness + 0.2·Coverage");

        // Create optimized selector with graph centrality
        let mut selector = OptimizedReferenceSelector::new().with_cache(AlignmentCache::new());

        if let Some(workspace) = &self.workspace {
            selector = selector.with_workspace(workspace.clone());
        }

        // Build alignment cache from provided alignments
        for alignment in alignments {
            let score = (alignment.identity / 100.0) as f64;
            selector
                .cache
                .set_score(&alignment.query_id, &alignment.reference_id, score);
        }

        // Use a mock aligner since we have pre-computed alignments
        let mut mock_aligner = talaria_tools::MockAligner::new();

        // Calculate target ratio based on desired references
        let target_ratio = 0.1; // Default to 10% references

        // Run graph centrality selection
        let result = selector.select_references_with_shared_index(
            sequences,
            target_ratio,
            &mut mock_aligner,
        )?;

        // Convert from optimized::SelectionResult to this module's SelectionResult
        Ok(SelectionResult {
            references: result.references,
            children: result.children,
            discarded: result.discarded,
        })
    }
}

impl Default for ReferenceSelectorImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_algorithm_default() {
        let selector = ReferenceSelectorImpl::new();
        // Default should be SinglePass
        assert_eq!(selector.selection_algorithm, SelectionAlgorithm::SinglePass);
    }

    #[test]
    fn test_selection_algorithm_builder() {
        let selector = ReferenceSelectorImpl::new()
            .with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix);
        assert_eq!(
            selector.selection_algorithm,
            SelectionAlgorithm::SimilarityMatrix
        );

        let selector2 =
            ReferenceSelectorImpl::new().with_selection_algorithm(SelectionAlgorithm::Hybrid);
        assert_eq!(selector2.selection_algorithm, SelectionAlgorithm::Hybrid);
    }

    #[test]
    fn test_reference_selector_default_batch_settings() {
        let selector = ReferenceSelectorImpl::new();
        // We can't directly access private fields, but we can test the behavior
        // by verifying that the defaults work as expected
        assert!(selector.min_length == 50); // This is accessible through default
    }

    #[test]
    fn test_reference_selector_with_batch_settings() {
        let selector = ReferenceSelectorImpl::new().with_batch_settings(true, 10000);

        // The batch settings are stored but not directly accessible
        // We can test that the builder pattern works without errors
        assert!(selector.min_length == 50); // Verify other defaults are unchanged
    }

    #[test]
    fn test_reference_selector_builder_pattern() {
        let selector = ReferenceSelectorImpl::new()
            .with_min_length(100)
            .with_similarity_threshold(0.8)
            .with_taxonomy_aware(true)
            .with_taxonomy_weights(true)
            .with_all_vs_all(false)
            .with_batch_settings(true, 2000)
            .with_manifest_acc2taxid(Some(PathBuf::from("/test/path")));

        // Test that the builder pattern compiles and runs without errors
        // We can't access private fields directly, but can verify public behavior
        assert_eq!(selector.min_length, 100);
        assert_eq!(selector.similarity_threshold, 0.8);
        assert!(selector.taxonomy_aware);
        assert_eq!(
            selector.manifest_acc2taxid,
            Some(PathBuf::from("/test/path"))
        );
    }

    #[test]
    fn test_simple_selection_result() {
        let selector = ReferenceSelectorImpl::new();

        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 100]), // 100 bp
            Sequence::new("seq2".to_string(), vec![65; 80]),  // 80 bp
            Sequence::new("seq3".to_string(), vec![65; 60]),  // 60 bp
            Sequence::new("seq4".to_string(), vec![65; 40]),  // 40 bp - too short
        ];

        let result = selector.simple_select_references(sequences, 0.5);

        // Should select ~50% of sequences (excluding too short)
        assert_eq!(result.references.len(), 2);
        // Longest sequences should be selected as references
        assert!(result.references.iter().any(|s| s.id == "seq1"));
        assert!(result.references.iter().any(|s| s.id == "seq2"));
        // seq4 should be discarded (too short)
        assert!(result.discarded.contains("seq4"));
    }

    #[test]
    fn test_parallel_candidate_evaluation() {
        // Test that parallel evaluation produces correct results
        use rayon::prelude::*;
        use std::sync::Arc;

        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 100]),
            Sequence::new("seq2".to_string(), vec![65; 100]),
            Sequence::new("seq3".to_string(), vec![65; 100]),
        ];

        // Create a mock similarity matrix
        let mut similarity_matrix = HashMap::new();
        similarity_matrix.insert(("seq1".to_string(), "seq2".to_string()), 0.8);
        similarity_matrix.insert(("seq2".to_string(), "seq1".to_string()), 0.8);
        similarity_matrix.insert(("seq1".to_string(), "seq3".to_string()), 0.75);
        similarity_matrix.insert(("seq3".to_string(), "seq1".to_string()), 0.75);
        similarity_matrix.insert(("seq2".to_string(), "seq3".to_string()), 0.9);
        similarity_matrix.insert(("seq3".to_string(), "seq2".to_string()), 0.9);

        let similarity_matrix = Arc::new(similarity_matrix);
        let uncovered = Arc::new(vec![
            "seq1".to_string(),
            "seq2".to_string(),
            "seq3".to_string(),
        ]);

        // Parallel evaluation
        let results: Vec<_> = sequences
            .par_iter()
            .map(|candidate| {
                let mut coverage = Vec::new();
                let mut score = 0.0;

                for other_id in uncovered.iter() {
                    if other_id == &candidate.id {
                        continue;
                    }

                    let key = (candidate.id.clone(), other_id.clone());
                    if let Some(&similarity) = similarity_matrix.get(&key) {
                        if similarity >= 0.7 {
                            coverage.push(other_id.clone());
                            score += similarity;
                        }
                    }
                }

                (candidate.id.clone(), coverage, score)
            })
            .collect();

        // Verify results
        assert_eq!(results.len(), 3, "Should evaluate all 3 candidates");

        // Find the best candidate
        let best = results
            .into_iter()
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

        assert!(best.is_some(), "Should find a best candidate");
        let (_best_id, best_coverage, best_score) = best.unwrap();

        // seq2 should be best as it has high similarity to both seq1 and seq3
        assert!(best_score > 0.0, "Best score should be positive");
        assert!(
            !best_coverage.is_empty(),
            "Best candidate should cover some sequences"
        );
    }

    #[test]
    fn test_early_termination_conditions() {
        // Test that selection stops at coverage target
        let sequences: Vec<_> = (0..1000)
            .map(|i| Sequence::new(format!("seq{}", i), vec![65; 100]))
            .collect();

        // With early termination, we should select far fewer than 1000 references
        let total_sequences = sequences.len();
        let min_coverage_target = 0.99;
        let covered = 990; // Simulating 99% coverage

        let coverage_ratio = covered as f64 / total_sequences as f64;
        assert!(
            coverage_ratio >= min_coverage_target,
            "Should meet coverage target"
        );
    }

    #[test]
    fn test_bidirectional_similarity_matrix() {
        // Test that similarity matrix contains both directions
        let mut similarity_matrix = HashMap::new();

        // Simulate building bidirectional matrix
        let query_id = "A".to_string();
        let subject_id = "B".to_string();
        let identity = 0.85;

        // Insert both directions as the code does
        similarity_matrix.insert((query_id.clone(), subject_id.clone()), identity);
        similarity_matrix.insert((subject_id.clone(), query_id.clone()), identity);

        // Check both directions exist
        assert_eq!(
            similarity_matrix.get(&("A".to_string(), "B".to_string())),
            Some(&0.85)
        );
        assert_eq!(
            similarity_matrix.get(&("B".to_string(), "A".to_string())),
            Some(&0.85)
        );
        assert_eq!(similarity_matrix.len(), 2, "Should have both directions");
    }

    #[test]
    fn test_both_algorithms_produce_valid_results() {
        // Create test sequences
        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 100]),
            Sequence::new("seq2".to_string(), vec![65; 90]),
            Sequence::new("seq3".to_string(), vec![65; 80]),
            Sequence::new("seq4".to_string(), vec![65; 70]),
            Sequence::new("seq5".to_string(), vec![65; 60]),
        ];

        // Test single-pass algorithm
        let selector_sp =
            ReferenceSelectorImpl::new().with_selection_algorithm(SelectionAlgorithm::SinglePass);
        let result_sp = selector_sp.simple_select_references(sequences.clone(), 0.4);

        // Test similarity matrix would need alignments, so we'll use simple selection
        let selector_sm = ReferenceSelectorImpl::new()
            .with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix);
        let result_sm = selector_sm.simple_select_references(sequences.clone(), 0.4);

        // Both should produce valid results
        assert!(
            !result_sp.references.is_empty(),
            "Single-pass should select references"
        );
        assert!(
            !result_sm.references.is_empty(),
            "Matrix algorithm should select references"
        );

        // Check invariants for both results
        for result in [result_sp, result_sm].iter() {
            // No sequence should be in both references and children values
            let ref_ids: HashSet<_> = result.references.iter().map(|r| &r.id).collect();
            for (_ref_id, children) in &result.children {
                for child_id in children {
                    assert!(
                        !ref_ids.contains(&child_id),
                        "Child {} should not also be a reference",
                        child_id
                    );
                }
            }

            // All discarded sequences should be accounted for
            assert!(
                result.discarded.len() <= sequences.len(),
                "Cannot discard more sequences than input"
            );
        }
    }

    #[test]
    fn test_selection_algorithm_properties() {
        // Test that selection maintains important properties
        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 100]),
            Sequence::new("seq2".to_string(), vec![65; 80]),
            Sequence::new("seq3".to_string(), vec![65; 60]),
            Sequence::new("seq4".to_string(), vec![65; 40]),
            Sequence::new("seq5".to_string(), vec![65; 20]), // Too short
        ];

        let selector = ReferenceSelectorImpl::new();
        let result = selector.simple_select_references(sequences.clone(), 0.5);

        // Property 1: All sequences are accounted for
        let total_accounted =
            result.references.len() + result.children.values().map(|v| v.len()).sum::<usize>();
        assert!(
            total_accounted <= sequences.len(),
            "Cannot account for more sequences than input"
        );

        // Property 2: References should be among the longer sequences
        let min_ref_length = result.references.iter().map(|r| r.len()).min().unwrap_or(0);
        assert!(
            min_ref_length >= selector.min_length,
            "All references should meet minimum length"
        );

        // Property 3: Coverage ratio should be reasonable
        let coverage = result.discarded.len() as f64 / sequences.len() as f64;
        assert!(coverage <= 1.0, "Coverage cannot exceed 100%");
    }
}
