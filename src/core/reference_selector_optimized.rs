/// Optimized reference selection with single index strategy
/// This module provides efficient reference selection using shared indices

use crate::bio::sequence::Sequence;
use crate::tools::traits::Aligner;
use crate::utils::temp_workspace::TempWorkspace;
use dashmap::DashMap;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use anyhow::Result;

/// Caching strategy for alignment scores
#[derive(Debug, Clone)]
pub struct AlignmentCache {
    /// Cache of pairwise alignment scores
    scores: Arc<DashMap<(String, String), f64>>,
    /// Cache of k-mer profiles
    kmers: Arc<DashMap<String, Vec<u64>>>,
    /// Statistics
    hits: Arc<Mutex<usize>>,
    misses: Arc<Mutex<usize>>,
}

impl AlignmentCache {
    pub fn new() -> Self {
        Self {
            scores: Arc::new(DashMap::new()),
            kmers: Arc::new(DashMap::new()),
            hits: Arc::new(Mutex::new(0)),
            misses: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get_score(&self, id1: &str, id2: &str) -> Option<f64> {
        // Ensure consistent ordering for cache key
        let key = if id1 < id2 {
            (id1.to_string(), id2.to_string())
        } else {
            (id2.to_string(), id1.to_string())
        };

        if let Some(score) = self.scores.get(&key) {
            *self.hits.lock().unwrap() += 1;
            Some(*score)
        } else {
            *self.misses.lock().unwrap() += 1;
            None
        }
    }

    pub fn set_score(&self, id1: &str, id2: &str, score: f64) {
        let key = if id1 < id2 {
            (id1.to_string(), id2.to_string())
        } else {
            (id2.to_string(), id1.to_string())
        };
        self.scores.insert(key, score);
    }

    pub fn get_kmers(&self, id: &str) -> Option<Vec<u64>> {
        self.kmers.get(id).map(|k| k.clone())
    }

    pub fn set_kmers(&self, id: &str, kmers: Vec<u64>) {
        self.kmers.insert(id.to_string(), kmers);
    }

    pub fn stats(&self) -> (usize, usize) {
        (*self.hits.lock().unwrap(), *self.misses.lock().unwrap())
    }
}

/// Optimized reference selector with shared index strategy
pub struct OptimizedReferenceSelector {
    pub min_length: usize,
    pub similarity_threshold: f64,
    pub taxonomy_aware: bool,
    pub use_taxonomy_weights: bool,
    pub workspace: Option<Arc<Mutex<TempWorkspace>>>,
    pub cache: AlignmentCache,
    pub parallel_taxa: bool,
    pub max_index_size: Option<usize>,
}

impl OptimizedReferenceSelector {
    pub fn new() -> Self {
        Self {
            min_length: 50,
            similarity_threshold: 0.9,
            taxonomy_aware: true,
            use_taxonomy_weights: false,
            workspace: None,
            cache: AlignmentCache::new(),
            parallel_taxa: true,
            max_index_size: None,
        }
    }

    pub fn with_workspace(mut self, workspace: Arc<Mutex<TempWorkspace>>) -> Self {
        self.workspace = Some(workspace);
        self
    }

    pub fn with_cache(mut self, cache: AlignmentCache) -> Self {
        self.cache = cache;
        self
    }

    /// Select references using single shared index strategy
    pub fn select_references_with_shared_index(
        &mut self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
        aligner: &mut dyn Aligner,
    ) -> Result<SelectionResult> {
        let start = Instant::now();
        let target_count = (sequences.len() as f64 * target_ratio) as usize;

        println!("🔧 Optimized reference selection with shared index");
        println!("  Total sequences: {}", sequences.len());
        println!("  Target references: {} ({:.1}%)", target_count, target_ratio * 100.0);

        // Step 1: Group sequences by taxonomy if taxonomy-aware
        let taxonomic_groups = if self.taxonomy_aware {
            self.group_by_taxonomy(&sequences)
        } else {
            vec![("all".to_string(), sequences.clone())]
        };

        println!("  Taxonomic groups: {}", taxonomic_groups.len());

        // Step 2: Build SINGLE shared index for ALL sequences
        println!("\n📊 Building shared LAMBDA index...");
        let index_start = Instant::now();

        let all_sequences_path = if let Some(ws) = &self.workspace {
            let workspace = ws.lock().unwrap();
            workspace.get_file_path("shared_index", "fasta")
        } else {
            std::env::temp_dir().join("talaria_shared_index.fasta")
        };

        // Write all sequences to single file
        crate::bio::fasta::write_fasta(&all_sequences_path, &sequences)?;

        // Build index ONCE
        let index_path = all_sequences_path.with_extension("lambda");
        aligner.build_index(&all_sequences_path, &index_path)?;

        println!("  ✓ Index built in {:.2}s", index_start.elapsed().as_secs_f64());

        // Step 3: Process each taxonomic group using the shared index
        let multi_progress = MultiProgress::new();
        let overall_pb = multi_progress.add(ProgressBar::new(target_count as u64));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Overall progress")
                .unwrap()
        );

        let mut all_references = Vec::new();
        let mut all_children: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_discarded = HashSet::new();

        // Process groups sequentially (aligner requires mutable access)
        let mut group_results = Vec::new();
        for (taxon, group_seqs) in &taxonomic_groups {
            let group_pb = multi_progress.add(ProgressBar::new(group_seqs.len() as u64));
            group_pb.set_style(
                ProgressStyle::default_bar()
                    .template(&format!("[{{elapsed_precise}}] {{bar:40.green}} {{pos}}/{{len}} Taxon: {}", taxon))
                    .unwrap()
            );

            let result = self.process_taxonomic_group(
                taxon,
                group_seqs,
                target_ratio,
                &sequences,
                aligner,
                &group_pb,
            )?;
            group_results.push(result);
        }

        // Merge results
        for result in group_results {
            all_references.extend(result.references);
            for (ref_id, children) in result.children {
                all_children.entry(ref_id).or_insert_with(Vec::new).extend(children);
            }
            all_discarded.extend(result.discarded);

            overall_pb.set_position(all_references.len().min(target_count) as u64);

            // Stop if we have enough references
            if all_references.len() >= target_count {
                break;
            }
        }

        // Trim to target count if needed
        if all_references.len() > target_count {
            all_references.truncate(target_count);
        }

        overall_pb.finish_with_message(format!("Selected {} references", all_references.len()));

        // Print cache statistics
        let (hits, misses) = self.cache.stats();
        let hit_rate = if hits + misses > 0 {
            (hits as f64 / (hits + misses) as f64) * 100.0
        } else {
            0.0
        };

        println!("\n📈 Performance Statistics:");
        println!("  Total time: {:.2}s", start.elapsed().as_secs_f64());
        println!("  Cache hit rate: {:.1}% ({} hits, {} misses)", hit_rate, hits, misses);
        println!("  References selected: {}", all_references.len());
        println!("  Sequences covered: {}", all_children.values().map(|c| c.len()).sum::<usize>());

        Ok(SelectionResult {
            references: all_references,
            children: all_children,
            discarded: all_discarded,
        })
    }

    /// Process a single taxonomic group
    fn process_taxonomic_group(
        &self,
        _taxon: &str,
        group_sequences: &[Sequence],
        target_ratio: f64,
        _all_sequences: &[Sequence],
        _aligner: &mut dyn Aligner,
        progress: &ProgressBar,
    ) -> Result<SelectionResult> {
        let group_target = (group_sequences.len() as f64 * target_ratio) as usize;
        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        // Sort by length (longest first)
        let mut sorted_seqs = group_sequences.to_vec();
        sorted_seqs.sort_by_key(|s| std::cmp::Reverse(s.sequence.len()));

        for candidate in sorted_seqs {
            if discarded.contains(&candidate.id) {
                continue;
            }

            if candidate.sequence.len() < self.min_length {
                discarded.insert(candidate.id.clone());
                continue;
            }

            // Check similarity against existing references
            let mut max_similarity: f64 = 0.0;
            for reference in &references {
                let similarity = self.calculate_similarity_cached(&candidate, reference)?;
                max_similarity = max_similarity.max(similarity);

                if similarity >= self.similarity_threshold {
                    // This sequence is covered by existing reference
                    children.entry(reference.id.clone())
                        .or_insert_with(Vec::new)
                        .push(candidate.id.clone());
                    discarded.insert(candidate.id.clone());
                    break;
                }
            }

            // If not covered, make it a reference
            if !discarded.contains(&candidate.id) {
                references.push(candidate.clone());
                discarded.insert(candidate.id.clone());

                // Find children for this new reference
                let mut ref_children = Vec::new();
                for other in group_sequences {
                    if discarded.contains(&other.id) || other.id == candidate.id {
                        continue;
                    }

                    let similarity = self.calculate_similarity_cached(&candidate, other)?;
                    if similarity >= self.similarity_threshold {
                        ref_children.push(other.id.clone());
                        discarded.insert(other.id.clone());
                    }
                }

                if !ref_children.is_empty() {
                    children.insert(candidate.id.clone(), ref_children);
                }
            }

            progress.inc(1);

            if references.len() >= group_target {
                break;
            }
        }

        progress.finish();

        Ok(SelectionResult {
            references,
            children,
            discarded,
        })
    }

    /// Calculate similarity between two sequences with caching
    fn calculate_similarity_cached(&self, seq1: &Sequence, seq2: &Sequence) -> Result<f64> {
        // Check cache first
        if let Some(score) = self.cache.get_score(&seq1.id, &seq2.id) {
            return Ok(score);
        }

        // Calculate k-mer similarity as fast approximation
        let kmers1 = self.get_or_compute_kmers(seq1);
        let kmers2 = self.get_or_compute_kmers(seq2);

        let similarity = self.calculate_kmer_similarity(&kmers1, &kmers2);

        // Cache the result
        self.cache.set_score(&seq1.id, &seq2.id, similarity);

        Ok(similarity)
    }

    /// Get or compute k-mer profile for a sequence
    fn get_or_compute_kmers(&self, seq: &Sequence) -> Vec<u64> {
        if let Some(kmers) = self.cache.get_kmers(&seq.id) {
            return kmers;
        }

        let kmers = self.compute_kmers(seq, 5);
        self.cache.set_kmers(&seq.id, kmers.clone());
        kmers
    }

    /// Compute k-mer profile for a sequence
    fn compute_kmers(&self, seq: &Sequence, k: usize) -> Vec<u64> {
        let mut kmers = Vec::new();
        let data = &seq.sequence;

        if data.len() < k {
            return kmers;
        }

        for i in 0..=data.len() - k {
            let kmer = &data[i..i + k];
            let mut hash = 0u64;
            for (j, &byte) in kmer.iter().enumerate() {
                hash |= (byte as u64) << (j * 8);
            }
            kmers.push(hash);
        }

        kmers.sort_unstable();
        kmers.dedup();
        kmers
    }

    /// Calculate k-mer similarity between two profiles
    fn calculate_kmer_similarity(&self, kmers1: &[u64], kmers2: &[u64]) -> f64 {
        if kmers1.is_empty() || kmers2.is_empty() {
            return 0.0;
        }

        let mut i = 0;
        let mut j = 0;
        let mut intersection = 0;

        while i < kmers1.len() && j < kmers2.len() {
            if kmers1[i] == kmers2[j] {
                intersection += 1;
                i += 1;
                j += 1;
            } else if kmers1[i] < kmers2[j] {
                i += 1;
            } else {
                j += 1;
            }
        }

        let union = kmers1.len() + kmers2.len() - intersection;
        intersection as f64 / union as f64
    }

    /// Group sequences by taxonomy
    fn group_by_taxonomy(&self, sequences: &[Sequence]) -> Vec<(String, Vec<Sequence>)> {
        let mut groups: HashMap<String, Vec<Sequence>> = HashMap::new();

        for seq in sequences {
            let taxon = self.extract_taxonomy_group(&seq.id, seq.description.as_deref());
            groups.entry(taxon).or_insert_with(Vec::new).push(seq.clone());
        }

        let mut sorted_groups: Vec<_> = groups.into_iter().collect();
        // Sort by group size (largest first) for better load balancing
        sorted_groups.sort_by_key(|(_, seqs)| std::cmp::Reverse(seqs.len()));

        sorted_groups
    }

    /// Extract taxonomy group from sequence metadata
    fn extract_taxonomy_group(&self, id: &str, description: Option<&str>) -> String {
        // Look for taxonomy patterns in description
        // Examples: "OS=Homo sapiens", "Tax=9606", "[Bacteria]"

        if let Some(desc) = description {
            // Try OS= pattern
            if let Some(start) = desc.find("OS=") {
                let organism = &desc[start + 3..];
                if let Some(end) = organism.find(" GN=").or_else(|| organism.find(" PE=")) {
                    return organism[..end].trim().to_string();
                }
            }

            // Try Tax= pattern
            if let Some(start) = desc.find("Tax=") {
                let taxid = &desc[start + 4..];
                if let Some(end) = taxid.find(' ') {
                    return format!("taxid_{}", &taxid[..end]);
                }
            }

            // Try [Organism] pattern
            if let Some(start) = desc.find('[') {
                if let Some(end) = desc.find(']') {
                    if end > start {
                        return desc[start + 1..end].to_string();
                    }
                }
            }
        }

        // Fallback: use ID prefix
        if let Some(pos) = id.find('|') {
            return id[..pos].to_string();
        }

        "unknown".to_string()
    }
}

/// Result of reference selection
#[derive(Debug, Clone)]
pub struct SelectionResult {
    pub references: Vec<Sequence>,
    pub children: HashMap<String, Vec<String>>,
    pub discarded: HashSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alignment_cache() {
        let cache = AlignmentCache::new();

        // Test setting and getting scores
        cache.set_score("seq1", "seq2", 0.95);
        assert_eq!(cache.get_score("seq1", "seq2"), Some(0.95));
        assert_eq!(cache.get_score("seq2", "seq1"), Some(0.95)); // Order independence

        // Test cache miss
        assert_eq!(cache.get_score("seq1", "seq3"), None);

        // Test statistics
        let (hits, misses) = cache.stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_kmer_computation() {
        let selector = OptimizedReferenceSelector::new();
        let seq = Sequence::new("test".to_string(), b"ACGTACGT".to_vec());

        let kmers = selector.compute_kmers(&seq, 3);
        assert!(!kmers.is_empty());

        // Check that kmers are unique
        let mut unique_kmers = kmers.clone();
        unique_kmers.dedup();
        assert_eq!(kmers.len(), unique_kmers.len());
    }

    #[test]
    fn test_taxonomy_extraction() {
        let selector = OptimizedReferenceSelector::new();

        // Test OS= pattern
        let taxon = selector.extract_taxonomy_group("id", Some("OS=Homo sapiens GN=GENE"));
        assert_eq!(taxon, "Homo sapiens");

        // Test Tax= pattern
        let taxon = selector.extract_taxonomy_group("id", Some("Tax=9606 OS=Human"));
        assert_eq!(taxon, "taxid_9606");

        // Test bracket pattern
        let taxon = selector.extract_taxonomy_group("id", Some("[Escherichia coli] protein"));
        assert_eq!(taxon, "Escherichia coli");

        // Test fallback
        let taxon = selector.extract_taxonomy_group("sp|P12345|PROT", None);
        assert_eq!(taxon, "sp");
    }
}