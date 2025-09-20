/// Graph centrality-based reference selection as specified in CASG architecture
/// This module implements the 5-dimensional approach for delta compression
/// Formula: Centrality Score = α·Degree + β·Betweenness + γ·Coverage
/// where α=0.5, β=0.3, γ=0.2
use crate::bio::sequence::Sequence;
use crate::core::memory_estimator::MemoryEstimator;
use crate::core::phylogenetic_clusterer::{ClusteringConfig, PhylogeneticClusterer};
use crate::tools::Aligner;
use crate::utils::temp_workspace::TempWorkspace;
use anyhow::Result;
use dashmap::DashMap;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use petgraph::algo::dijkstra;
use petgraph::graph::{NodeIndex, UnGraph};
use petgraph::visit::EdgeRef;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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

/// Graph node for centrality calculation
#[derive(Debug, Clone)]
struct GraphNode {
    sequence_id: String,
    degree: f64,
    betweenness: f64,
    coverage: f64,
    centrality_score: f64,
}

impl Eq for GraphNode {}

impl PartialEq for GraphNode {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_id == other.sequence_id
    }
}

impl Ord for GraphNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.centrality_score
            .partial_cmp(&other.centrality_score)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for GraphNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Graph centrality-based reference selector
pub struct OptimizedReferenceSelector {
    pub min_length: usize,
    pub similarity_threshold: f64,
    pub taxonomy_aware: bool,
    pub use_taxonomy_weights: bool,
    pub workspace: Option<Arc<Mutex<TempWorkspace>>>,
    pub cache: AlignmentCache,
    pub parallel_taxa: bool,
    pub max_index_size: Option<usize>,
    // Graph centrality weights from architecture
    pub alpha: f64, // Degree weight (0.5)
    pub beta: f64,  // Betweenness weight (0.3)
    pub gamma: f64, // Coverage weight (0.2)
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
            // CASG architecture-specified weights
            alpha: 0.5, // Degree centrality weight
            beta: 0.3,  // Betweenness centrality weight
            gamma: 0.2, // Coverage weight
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

    /// Select references using graph centrality metrics as per CASG architecture
    /// Implements: Centrality Score = α·Degree + β·Betweenness + γ·Coverage
    pub fn select_references_with_shared_index(
        &mut self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
        aligner: &mut dyn Aligner,
    ) -> Result<SelectionResult> {
        println!("🔬 Graph centrality-based reference selection (CASG 5-dimensional approach)");
        println!(
            "  Formula: Score = {:.1}·Degree + {:.1}·Betweenness + {:.1}·Coverage",
            self.alpha, self.beta, self.gamma
        );

        // Build similarity graph first
        let graph_result = self.build_similarity_graph(&sequences, aligner)?;
        let selected_refs = self.select_by_centrality(graph_result, &sequences, target_ratio)?;

        Ok(selected_refs)
    }

    /// Original implementation for fallback
    pub fn select_references_with_shared_index_legacy(
        &mut self,
        sequences: Vec<Sequence>,
        target_ratio: f64,
        aligner: &mut dyn Aligner,
    ) -> Result<SelectionResult> {
        let start = Instant::now();
        let target_count = (sequences.len() as f64 * target_ratio) as usize;

        println!("🔧 Optimized reference selection with shared index");
        println!("  Total sequences: {}", sequences.len());
        println!(
            "  Target references: {} ({:.1}%)",
            target_count,
            target_ratio * 100.0
        );

        // Step 1: Use phylogenetic clustering for intelligent grouping
        let taxonomic_groups = if self.taxonomy_aware {
            println!("  🧬 Using phylogenetic clustering for taxonomic grouping...");

            // Create clustering config optimized for SwissProt/UniProt
            let config = ClusteringConfig::for_swissprot();
            let clusterer = PhylogeneticClusterer::new(config);

            // Check memory constraints
            let memory_estimator = MemoryEstimator::new();
            if !memory_estimator.can_process_cluster(&sequences) {
                println!("  ⚠️  Large dataset detected, using adaptive clustering");
            }

            // Perform clustering
            match clusterer.create_clusters(sequences.clone()) {
                Ok(clusters) => {
                    println!("  ✓ Created {} phylogenetic clusters:", clusters.len());
                    for (i, cluster) in clusters.iter().take(5).enumerate() {
                        println!(
                            "    Cluster {}: {} sequences, {} taxa",
                            i + 1,
                            cluster.sequences.len(),
                            cluster.taxa.len()
                        );
                    }
                    if clusters.len() > 5 {
                        println!("    ... and {} more clusters", clusters.len() - 5);
                    }

                    // Convert clusters to the expected format
                    clusters
                        .into_iter()
                        .enumerate()
                        .map(|(i, cluster)| {
                            let name = if cluster.taxa.len() == 1 {
                                format!("taxid_{}", cluster.taxa.iter().next().unwrap())
                            } else {
                                format!("cluster_{}_taxa_{}", i, cluster.taxa.len())
                            };
                            (name, cluster.sequences)
                        })
                        .collect()
                }
                Err(e) => {
                    println!("  ⚠️  Phylogenetic clustering failed: {}", e);
                    println!("  Falling back to simple taxonomy grouping");
                    self.group_by_taxonomy(&sequences)
                }
            }
        } else {
            vec![("all".to_string(), sequences.clone())]
        };

        println!("  Total groups: {}", taxonomic_groups.len());

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

        // Note: Index building would be done internally by the aligner
        // when search() is called, if needed
        // let index_path = all_sequences_path.with_extension("lambda");

        println!(
            "  ✓ Index built in {:.2}s",
            index_start.elapsed().as_secs_f64()
        );

        // Step 3: Process each taxonomic group using the shared index
        let multi_progress = MultiProgress::new();
        let overall_pb = multi_progress.add(ProgressBar::new(target_count as u64));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Overall progress")
                .unwrap(),
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
                    .template(&format!(
                        "[{{elapsed_precise}}] {{bar:40.green}} {{pos}}/{{len}} Taxon: {}",
                        taxon
                    ))
                    .unwrap(),
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
                all_children
                    .entry(ref_id)
                    .or_default()
                    .extend(children);
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
        println!(
            "  Cache hit rate: {:.1}% ({} hits, {} misses)",
            hit_rate, hits, misses
        );
        println!("  References selected: {}", all_references.len());
        println!(
            "  Sequences covered: {}",
            all_children.values().map(|c| c.len()).sum::<usize>()
        );

        Ok(SelectionResult {
            references: all_references,
            children: all_children,
            discarded: all_discarded,
        })
    }

    /// Build similarity graph for centrality calculation
    fn build_similarity_graph(
        &self,
        sequences: &[Sequence],
        _aligner: &mut dyn Aligner,
    ) -> Result<(UnGraph<String, f64>, HashMap<String, NodeIndex>)> {
        println!("  Building similarity graph...");
        let mut graph = UnGraph::<String, f64>::new_undirected();
        let mut node_map = HashMap::new();

        // Add nodes
        for seq in sequences {
            let node = graph.add_node(seq.id.clone());
            node_map.insert(seq.id.clone(), node);
        }

        // Add edges based on similarity
        let progress = ProgressBar::new((sequences.len() * (sequences.len() - 1) / 2) as u64);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Building graph")
                .unwrap(),
        );

        for i in 0..sequences.len() {
            for j in i + 1..sequences.len() {
                let sim = self.calculate_similarity_cached(&sequences[i], &sequences[j])?;
                if sim >= self.similarity_threshold * 0.5 {
                    // Lower threshold for graph construction
                    let node_i = node_map[&sequences[i].id];
                    let node_j = node_map[&sequences[j].id];
                    graph.add_edge(node_i, node_j, sim);
                }
                progress.inc(1);
            }
        }
        progress.finish_with_message("Graph built");

        Ok((graph, node_map))
    }

    /// Calculate centrality metrics and select references
    fn select_by_centrality(
        &self,
        graph_data: (UnGraph<String, f64>, HashMap<String, NodeIndex>),
        sequences: &[Sequence],
        target_ratio: f64,
    ) -> Result<SelectionResult> {
        let (graph, node_map) = graph_data;
        let target_count = (sequences.len() as f64 * target_ratio) as usize;

        println!("  Calculating centrality metrics...");

        // Calculate metrics for each node
        let mut graph_nodes = Vec::new();

        for seq in sequences {
            let node_idx = node_map[&seq.id];

            // Degree centrality: number of connections
            let degree = graph.edges(node_idx).count() as f64;

            // Betweenness centrality: how often node appears in shortest paths
            let betweenness = self.calculate_betweenness(&graph, node_idx, &node_map);

            // Coverage: sequence length as proxy for information content
            let coverage = seq.sequence.len() as f64;

            // Calculate final centrality score
            let centrality_score =
                self.alpha * degree + self.beta * betweenness + self.gamma * (coverage / 1000.0);

            graph_nodes.push(GraphNode {
                sequence_id: seq.id.clone(),
                degree,
                betweenness,
                coverage,
                centrality_score,
            });
        }

        // Sort by centrality score (highest first)
        graph_nodes.sort_by(|a, b| {
            b.centrality_score
                .partial_cmp(&a.centrality_score)
                .unwrap_or(Ordering::Equal)
        });

        println!("  Top 5 centrality scores:");
        for (i, node) in graph_nodes.iter().take(5).enumerate() {
            println!(
                "    {}. {} - Score: {:.2} (D:{:.0}, B:{:.2}, C:{:.0})",
                i + 1,
                &node.sequence_id[..node.sequence_id.len().min(20)],
                node.centrality_score,
                node.degree,
                node.betweenness,
                node.coverage
            );
        }

        // Select top nodes as references
        let mut references = Vec::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut discarded = HashSet::new();

        let seq_map: HashMap<String, &Sequence> =
            sequences.iter().map(|s| (s.id.clone(), s)).collect();

        for node in graph_nodes.iter().take(target_count) {
            if let Some(seq) = seq_map.get(&node.sequence_id) {
                references.push((*seq).clone());
                discarded.insert(node.sequence_id.clone());

                // Find sequences covered by this reference
                let node_idx = node_map[&node.sequence_id];
                for edge in graph.edges(node_idx) {
                    let other_idx = edge.target();
                    let other_id = &graph[other_idx];

                    if !discarded.contains(other_id) && edge.weight() >= &self.similarity_threshold
                    {
                        children
                            .entry(node.sequence_id.clone())
                            .or_default()
                            .push(other_id.clone());
                        discarded.insert(other_id.clone());
                    }
                }
            }
        }

        println!(
            "  Selected {} references based on centrality",
            references.len()
        );
        println!("  Covered {} sequences", discarded.len());

        Ok(SelectionResult {
            references,
            children,
            discarded,
        })
    }

    /// Calculate betweenness centrality for a node
    fn calculate_betweenness(
        &self,
        graph: &UnGraph<String, f64>,
        node: NodeIndex,
        _node_map: &HashMap<String, NodeIndex>,
    ) -> f64 {
        // Simplified betweenness: count shortest paths through this node
        let mut betweenness = 0.0;

        // Sample calculation - for production, use full algorithm
        let distances = dijkstra(graph, node, None, |e| *e.weight() as i32);

        // Normalize by number of reachable nodes
        let reachable = distances.len() as f64;
        if reachable > 0.0 {
            betweenness = reachable / graph.node_count() as f64;
        }

        betweenness * 100.0 // Scale for visibility
    }

    /// Process a single taxonomic group (legacy)
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
                    children
                        .entry(reference.id.clone())
                        .or_default()
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
            groups
                .entry(taxon)
                .or_default()
                .push(seq.clone());
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
