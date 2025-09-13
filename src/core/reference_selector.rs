/// Reference selection algorithm for choosing representative sequences

use crate::bio::sequence::Sequence;
use crate::bio::alignment::Alignment;
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ReferenceSelector {
    min_length: usize,
    similarity_threshold: f64,
    taxonomy_aware: bool,
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
        }
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
                
                // Quick similarity check with relaxed threshold for auto-detection
                // Use a more lenient check since we're trying to find natural clusters
                let len_ratio = other.len().min(query.len()) as f64 / other.len().max(query.len()) as f64;
                if len_ratio >= 0.7 {
                    // Check k-mer similarity with relaxed threshold
                    let k = 3;
                    let kmers1 = self.extract_kmers(&query.sequence, k);
                    let kmers2 = self.extract_kmers(&other.sequence, k);
                    
                    let intersection: HashSet<_> = kmers1.intersection(&kmers2).collect();
                    let union_size = kmers1.len() + kmers2.len() - intersection.len();
                    
                    if union_size > 0 {
                        let jaccard = intersection.len() as f64 / union_size as f64;
                        // Use relaxed threshold for auto-detection (0.4 instead of 0.63)
                        if jaccard >= 0.4 {
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
            
            // Stop if we're getting diminishing returns
            if references.len() > 10 && coverage_history.len() > 3 {
                let recent_improvement = coverage_history[coverage_history.len() - 1] 
                    - coverage_history[coverage_history.len() - 3];
                
                // Stop if improvement over last 3 references is less than 1%
                if recent_improvement < 0.01 {
                    pb.finish_with_message(format!(
                        "Auto-detected {} references (coverage: {:.1}%, plateau reached)",
                        references.len(),
                        coverage_ratio * 100.0
                    ));
                    break;
                }
            }
            
            // Stop if we've covered 95% of sequences
            if coverage_ratio > 0.95 {
                pb.finish_with_message(format!(
                    "Auto-detected {} references (coverage: {:.1}%)",
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
        
        pb.finish_with_message(format!("Auto-detected {} references", references.len()));
        
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
}

impl Default for ReferenceSelector {
    fn default() -> Self {
        Self::new()
    }
}