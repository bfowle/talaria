/// Reference selection algorithm for choosing representative sequences

use crate::bio::sequence::Sequence;
use crate::bio::alignment::Alignment;
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ReferenceSelector {
    min_length: usize,
    similarity_threshold: f64,
    taxonomy_aware: bool,
    use_taxonomy_weights: bool,  // Weight alignment scores by taxonomic distance
    all_vs_all: bool,  // Use all-vs-all mode for Lambda alignments
    manifest_acc2taxid: Option<PathBuf>,  // Path to manifest-based accession2taxid file
}

#[derive(Debug, Clone)]
pub struct SelectionResult {
    pub references: Vec<Sequence>,
    pub children: HashMap<String, Vec<String>>, // reference_id -> child_ids
    pub discarded: HashSet<String>,
}

impl ReferenceSelector {
    pub fn new() -> Self {
        Self {
            min_length: 50,
            similarity_threshold: 0.9,
            taxonomy_aware: true,
            use_taxonomy_weights: false,  // Default to no taxonomy weighting
            all_vs_all: false,  // Default to query-vs-reference
            manifest_acc2taxid: None,
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
    
    /// Simple greedy reference selection based only on sequence length
    /// Then assigns non-selected sequences to their best matching reference
    pub fn simple_select_references(&self, sequences: Vec<Sequence>, target_ratio: f64) -> SelectionResult {
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
        
        pb.finish_with_message(format!("Selected {} references", references.len()));
        
        // Phase 2: Assign non-reference sequences to their best matching reference
        // Calculate how many sequences need assignment
        let sequences_to_assign = sequences.iter()
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
                    .min_by_key(|ref_seq| {
                        (ref_seq.len() as i64 - seq.len() as i64).abs()
                    })?;
                
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
            children.entry(ref_id).or_insert_with(Vec::new).push(child_id.clone());
            discarded.insert(child_id);
        }
        
        pb2.finish_with_message(format!(
            "Assigned {} sequences to {} references",
            sequences_to_assign,
            references.len()
        ));
        
        SelectionResult {
            references,
            children,
            discarded,
        }
    }
    
    /// Select reference sequences - defaults to simple greedy selection
    pub fn select_references(&self, sequences: Vec<Sequence>, target_ratio: f64) -> SelectionResult {
        // Default to simple selection (matching original db-reduce)
        self.simple_select_references(sequences, target_ratio)
    }
    
    /// Select reference sequences with similarity-based clustering
    /// This uses k-mer similarity to group similar sequences
    pub fn select_references_with_similarity(&self, sequences: Vec<Sequence>, target_ratio: f64) -> SelectionResult {
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
            pb.set_message(format!("Processing batch {}/{}", 
                batch_idx + 1, 
                (sorted_sequences.len() + batch_size - 1) / batch_size));
            
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
        
        pb.finish_with_message(format!("Selected {} reference sequences", references.len()));
        
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
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Auto-detecting references")
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
        const MIN_COVERAGE: f64 = 0.10;  // At least 10% coverage

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
                let len_ratio = other.len().min(query.len()) as f64 / other.len().max(query.len()) as f64;
                if len_ratio >= 0.5 {  // Relaxed from 0.7 to 0.5
                    // Check k-mer similarity with more permissive settings
                    let k = 2;  // Use 2-mers instead of 3-mers for proteins
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
                        pb.finish_with_message(format!(
                            "Auto-detected {} references (coverage: {:.1}%, plateau reached)",
                            references.len(),
                            coverage_ratio * 100.0
                        ));
                        break;
                    }
                }
            }
            
            // Stop if we've covered 95% of sequences (but ensure minimum coverage first)
            if coverage_ratio > 0.95 && references.len() >= MIN_REFERENCES {
                pb.finish_with_message(format!(
                    "Auto-detected {} references (coverage: {:.1}%)",
                    references.len(),
                    coverage_ratio * 100.0
                ));
                break;
            }

            // Limit to reasonable number of references (e.g., 10% of sequences)
            if references.len() >= sequences.len() / 10 && references.len() >= MIN_REFERENCES {
                pb.finish_with_message(format!(
                    "Auto-detected {} references (coverage: {:.1}%, max references reached)",
                    references.len(),
                    coverage_ratio * 100.0
                ));
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
            pb.set_message(format!("References: {}, Coverage: {:.1}%", 
                                  references.len(), coverage_ratio * 100.0));
        }
        
        // Calculate final coverage
        let final_coverage = discarded.len() as f64 / sequences.len() as f64;

        // If we found very few sequences, fall back to simple selection
        if final_coverage < 0.01 && sequences.len() > 1000 {
            pb.finish_with_message("Auto-detection found too few matches, falling back to length-based selection");
            // Use simple selection with 10% ratio as fallback
            return self.simple_select_references(sequences, 0.1);
        }

        pb.finish_with_message(format!("Auto-detected {} references (final coverage: {:.1}%)",
                                      references.len(), final_coverage * 100.0));

        SelectionResult {
            references,
            children,
            discarded,
        }
    }
    
    /// Perform full alignment-based selection (more accurate but slower)
    pub fn select_references_with_alignment(&self, sequences: Vec<Sequence>, target_ratio: f64) -> SelectionResult {
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
                let similarity = self.calculate_similarity(&alignment);
                
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
        
        pb.finish_with_message(format!("Selected {} references with alignment", references.len()));
        
        SelectionResult {
            references,
            children,
            discarded,
        }
    }
    
    fn calculate_similarity(&self, alignment: &crate::bio::alignment::AlignmentResult) -> f64 {
        let matches = alignment.alignment_string.iter()
            .filter(|&&c| c == b'|')
            .count();
        let total = alignment.alignment_string.len();

        if total == 0 {
            0.0
        } else {
            matches as f64 / total as f64
        }
    }

    /// Calculate a weight based on taxonomic distance between two sequences
    /// Returns a value between 0.5 and 1.5 where:
    /// - Same species (all levels match): 1.5 (boost similar taxonomy)
    /// - Same genus but different species: 1.2
    /// - Same family but different genus: 1.0
    /// - Different families: 0.8 (penalize distant taxonomy)
    /// - No taxonomy data: 1.0 (neutral)
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
            0 => 0.8,   // Different at kingdom level (very distant)
            1 => 0.9,   // Same kingdom, different phylum
            2 => 0.95,  // Same phylum, different class
            3 => 1.0,   // Same class, different order
            4 => 1.05,  // Same order, different family
            5 => 1.1,   // Same family, different genus
            6 => 1.2,   // Same genus, different species
            _ => 1.5,   // Same species or subspecies (very close)
        }
    }

    /// Extract taxonomy levels from sequence description
    /// Returns a vector of taxonomy levels from most general to most specific
    fn extract_taxonomy(&self, description: &str) -> Vec<String> {
        let mut taxonomy = Vec::new();

        // Look for NCBI taxonomy ID format: "Tax=9606" or "TaxID=9606"
        if let Some(tax_match) = description.split_whitespace()
            .find(|s| s.starts_with("Tax=") || s.starts_with("TaxID="))
        {
            let tax_id = tax_match.split('=').nth(1).unwrap_or("");
            // Map common taxonomy IDs to hierarchy
            // This is a simplified example - in production, would use a taxonomy database
            taxonomy = match tax_id {
                "9606" => vec!["Eukaryota", "Chordata", "Mammalia", "Primates", "Hominidae", "Homo", "sapiens"],
                "10090" => vec!["Eukaryota", "Chordata", "Mammalia", "Rodentia", "Muridae", "Mus", "musculus"],
                "7227" => vec!["Eukaryota", "Arthropoda", "Insecta", "Diptera", "Drosophilidae", "Drosophila", "melanogaster"],
                "6239" => vec!["Eukaryota", "Nematoda", "Chromadorea", "Rhabditida", "Rhabditidae", "Caenorhabditis", "elegans"],
                "4932" => vec!["Eukaryota", "Ascomycota", "Saccharomycetes", "Saccharomycetales", "Saccharomycetaceae", "Saccharomyces", "cerevisiae"],
                "562" => vec!["Bacteria", "Proteobacteria", "Gammaproteobacteria", "Enterobacterales", "Enterobacteriaceae", "Escherichia", "coli"],
                _ => vec![],
            }.iter().map(|s| s.to_string()).collect();
        }

        // Alternative: Look for organism name "OS=Homo sapiens"
        if taxonomy.is_empty() {
            if let Some(os_match) = description.split_whitespace()
                .position(|s| s == "OS=")
                .and_then(|i| description.split_whitespace().nth(i + 1))
            {
                // Parse organism name into taxonomy hierarchy
                // This is simplified - real implementation would use taxonomy database
                let organism = os_match.to_lowercase();
                if organism.contains("homo") && organism.contains("sapiens") {
                    taxonomy = vec!["Eukaryota", "Chordata", "Mammalia", "Primates", "Hominidae", "Homo", "sapiens"]
                        .iter().map(|s| s.to_string()).collect();
                } else if organism.contains("mus") && organism.contains("musculus") {
                    taxonomy = vec!["Eukaryota", "Chordata", "Mammalia", "Rodentia", "Muridae", "Mus", "musculus"]
                        .iter().map(|s| s.to_string()).collect();
                }
                // Add more organism mappings as needed
            }
        }

        taxonomy
    }
    
    /// Select references using LAMBDA aligner for accurate alignments
    /// This implements the original db-reduce algorithm
    pub fn select_references_with_lambda(&self, sequences: Vec<Sequence>) -> anyhow::Result<SelectionResult> {
        use crate::tools::{ToolManager, Tool};
        use crate::tools::lambda::LambdaAligner;

        println!("Starting LAMBDA-based reference selection...");
        println!("  Processing {} sequences", sequences.len());

        // Check if LAMBDA is installed
        let manager = ToolManager::new()?;
        let lambda_path = manager.get_current_tool_path(Tool::Lambda)?;
        println!("  LAMBDA binary: {:?}", lambda_path);

        // Create LAMBDA aligner with optional manifest-based taxonomy
        let mut aligner = LambdaAligner::new(lambda_path)?;

        // If we have a manifest-based accession2taxid file, use it
        if let Some(ref acc2taxid_path) = self.manifest_acc2taxid {
            // Also need the taxdump directory
            let taxonomy_dir = crate::core::paths::talaria_taxonomy_dir();
            let taxdump_dir = taxonomy_dir.join("taxdump");

            if taxdump_dir.exists() {
                aligner = aligner.with_taxonomy(Some(acc2taxid_path.clone()), Some(taxdump_dir));
                println!("  Using manifest-based taxonomy mapping");
            } else {
                println!("  Warning: taxdump directory not found, taxonomy features disabled");
            }
        }

        println!("  LAMBDA aligner initialized");

        // Run alignments with LAMBDA
        let pb = ProgressBar::new(sequences.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Running LAMBDA alignments")
                .unwrap(),
        );

        let alignments = if self.all_vs_all {
            // All-vs-all mode: self-alignment within the dataset
            aligner.search_all_vs_all(&sequences)?
        } else {
            // Default: Query-vs-reference mode
            // Use a subset as reference (e.g., longest sequences)
            let mut sorted_sequences = sequences.clone();
            sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));

            // Take top 20% as reference sequences
            let reference_count = std::cmp::max(10, sequences.len() / 5);
            let reference_sequences: Vec<_> = sorted_sequences.iter()
                .take(reference_count)
                .cloned()
                .collect();

            // All sequences are queries
            aligner.search(&sequences, &reference_sequences)?
        };
        pb.finish_with_message("LAMBDA alignments complete");
        
        // Build similarity matrix from alignments
        let mut similarity_matrix: HashMap<(String, String), f64> = HashMap::new();
        for alignment in alignments {
            let key = (alignment.query_id.clone(), alignment.subject_id.clone());
            similarity_matrix.insert(key, alignment.identity);
        }
        
        // Greedy selection based on alignment coverage
        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();
        let mut uncovered: HashSet<String> = sequences.iter().map(|s| s.id.clone()).collect();
        
        let pb2 = ProgressBar::new(sequences.len() as u64);
        pb2.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Selecting references")
                .unwrap(),
        );
        
        // Sort sequences by length (longest first)
        let mut sorted_sequences = sequences.clone();
        sorted_sequences.sort_by_key(|s| std::cmp::Reverse(s.len()));
        
        // Iteratively select references that cover the most uncovered sequences
        while !uncovered.is_empty() {
            let mut best_reference = None;
            let mut best_coverage = Vec::new();
            let mut best_score = 0.0;
            
            // Find the sequence that covers the most uncovered sequences
            for candidate in &sorted_sequences {
                if discarded.contains(&candidate.id) {
                    continue;
                }
                
                let mut coverage = Vec::new();
                let mut score = 0.0;
                
                for other_id in &uncovered {
                    if other_id == &candidate.id {
                        continue;
                    }

                    // Check similarity from alignment results
                    let key = (candidate.id.clone(), other_id.clone());
                    if let Some(&similarity) = similarity_matrix.get(&key) {
                        if similarity >= 0.7 { // 70% identity threshold
                            coverage.push(other_id.clone());

                            // Apply taxonomy weighting if enabled
                            let weighted_score = if self.use_taxonomy_weights {
                                // Find the sequences to extract taxonomy data
                                let candidate_seq = sorted_sequences.iter()
                                    .find(|s| s.id == candidate.id);
                                let other_seq = sorted_sequences.iter()
                                    .find(|s| s.id == *other_id);

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
                
                if score > best_score {
                    best_reference = Some(candidate.clone());
                    best_coverage = coverage;
                    best_score = score;
                }
            }
            
            // If no good reference found, break
            if best_reference.is_none() || best_coverage.is_empty() {
                // Add remaining sequences as their own references
                for seq_id in uncovered.iter() {
                    if let Some(seq) = sorted_sequences.iter().find(|s| &s.id == seq_id) {
                        references.push(seq.clone());
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
            pb2.set_message(format!("References: {}, Uncovered: {}", 
                                   references.len(), uncovered.len()));
        }
        
        pb2.finish_with_message(format!("Selected {} references using LAMBDA", references.len()));
        
        Ok(SelectionResult {
            references,
            children,
            discarded,
        })
    }
}

impl Default for ReferenceSelector {
    fn default() -> Self {
        Self::new()
    }
}