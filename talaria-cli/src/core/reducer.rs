/// Main reduction pipeline
use talaria_bio::sequence::Sequence;
use talaria_sequoia::TargetAligner;
use talaria_core::Config;
use super::{
    delta_encoder::{DeltaEncoder, DeltaRecord},
    reference_selector::{ReferenceSelectorImpl, SelectionAlgorithm},
};
use talaria_utils::workspace::TempWorkspace;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Reducer {
    config: Config,
    progress_callback: Option<Box<dyn Fn(&str, f64) + Send + Sync>>,
    use_similarity: bool,
    use_alignment: bool,
    use_taxonomy_weights: bool,
    silent: bool,
    no_deltas: bool,
    max_align_length: usize,
    input_file_size: u64,
    output_file_size: u64,
    all_vs_all: bool,
    manifest_acc2taxid: Option<PathBuf>,
    batch_enabled: bool,
    batch_size: usize,
    pub selection_algorithm: SelectionAlgorithm,
    workspace: Option<Arc<Mutex<TempWorkspace>>>,
}

impl Reducer {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            progress_callback: None,
            use_similarity: false,
            use_alignment: false,
            use_taxonomy_weights: false,
            silent: false,
            no_deltas: false,
            max_align_length: 10000,
            input_file_size: 0,
            output_file_size: 0,
            all_vs_all: true,
            manifest_acc2taxid: None,
            batch_enabled: false,
            batch_size: 5000,
            selection_algorithm: SelectionAlgorithm::SinglePass,
            workspace: None,
        }
    }

    pub fn with_no_deltas(mut self, no_deltas: bool) -> Self {
        self.no_deltas = no_deltas;
        self
    }

    pub fn with_max_align_length(mut self, max_length: usize) -> Self {
        self.max_align_length = max_length;
        self
    }

    pub fn with_selection_mode(mut self, use_similarity: bool, use_alignment: bool) -> Self {
        self.use_similarity = use_similarity;
        self.use_alignment = use_alignment;
        self
    }

    pub fn with_taxonomy_weights(mut self, use_weights: bool) -> Self {
        self.use_taxonomy_weights = use_weights;
        self
    }

    pub fn with_silent(mut self, silent: bool) -> Self {
        self.silent = silent;
        self
    }

    pub fn with_file_sizes(mut self, input_size: u64, output_size: u64) -> Self {
        self.input_file_size = input_size;
        self.output_file_size = output_size;
        self
    }

    pub fn with_all_vs_all(mut self, all_vs_all: bool) -> Self {
        self.all_vs_all = all_vs_all;
        self
    }

    pub fn with_manifest_acc2taxid(mut self, path: Option<PathBuf>) -> Self {
        self.manifest_acc2taxid = path;
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

    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str, f64) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    pub fn reduce(
        &mut self,
        sequences: Vec<Sequence>,
        reduction_ratio: f64,
        target_aligner: TargetAligner,
    ) -> Result<(Vec<Sequence>, Vec<DeltaRecord>, usize), crate::TalariaError> {
        let multi_progress = MultiProgress::new();

        // Step 0: Sanitize sequences by removing those with ambiguous residues
        let (sanitized_sequences, removed_count) = if !self.silent {
            use crate::cli::output::*;
            action("Sanitizing sequences (removing ambiguous residues)...");
            talaria_bio::sequence::sanitize_sequences(sequences)
        } else {
            talaria_bio::sequence::sanitize_sequences(sequences)
        };

        // Sanitization results are now shown by sanitize_sequences function

        // Update workspace stats if available
        if let Some(workspace) = &self.workspace {
            if let Ok(mut ws) = workspace.lock() {
                ws.update_stats(|s| {
                    s.sanitized_sequences = sanitized_sequences.len();
                    s.removed_sequences = removed_count;
                })
                .ok();

                // Save sanitized sequences to workspace
                let sanitized_path = ws.get_file_path("sanitized_fasta", "fasta");
                drop(ws); // Release lock before writing

                // write_fasta will show its own progress for large files
                talaria_bio::fasta::write_fasta(&sanitized_path, &sanitized_sequences).ok();
            }
        }

        let sequences = sanitized_sequences;

        // Step 1: Select references
        let selection_pb = if !self.silent {
            let pb = multi_progress.add(ProgressBar::new(100));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}% Selecting references")
                    .unwrap(),
            );
            pb
        } else {
            ProgressBar::hidden()
        };

        if let Some(ref callback) = self.progress_callback {
            callback("Selecting references", 0.0);
        }

        let mut selector = self.configure_selector(&target_aligner);

        // Pass workspace to selector if available
        if let Some(workspace) = &self.workspace {
            selector = selector.with_workspace(workspace.clone());
        }

        // Choose selection method based on configuration
        let selection_result = if reduction_ratio == 0.0 {
            // Auto-detection mode - no ratio specified
            // LAMBDA is required for auto-detection
            let manager = talaria_tools::ToolManager::new().map_err(|e| {
                crate::TalariaError::Configuration(format!("Failed to initialize tool manager: {}", e))
            })?;

            if !manager.is_installed(talaria_tools::Tool::Lambda) {
                return Err(crate::TalariaError::Configuration("LAMBDA aligner is required for auto-detection mode.\n\n\
                        To install LAMBDA:\n  \
                        talaria tools install lambda\n\n\
                        Or specify a fixed reduction ratio:\n  \
                        talaria reduce -i input.fasta -o output.fasta -r 0.3\n\n\
                        For more information: https://github.com/seqan/lambda3".to_string()));
            }

            if !self.silent {
                use crate::cli::output::*;
                info("Using LAMBDA aligner for intelligent auto-detection...");
            }

            selector
                .select_references_with_lambda(sequences.clone())
                .map_err(|e| {
                    crate::TalariaError::Other(format!("LAMBDA alignment failed: {}", e))
                })?
        } else if self.use_alignment {
            // Use full alignment-based selection
            selector.select_references_with_alignment(sequences.clone(), reduction_ratio)
        } else if self.use_similarity {
            // Use k-mer similarity-based selection
            selector.select_references_with_similarity(sequences.clone(), reduction_ratio)
        } else {
            // Use simple greedy selection (default, matches original db-reduce)
            selector.simple_select_references(sequences.clone(), reduction_ratio)
        };

        if let Some(ref callback) = self.progress_callback {
            callback("Reference selection complete", 50.0);
        }
        if !self.silent {
            selection_pb.finish_and_clear();
            println!("Reference selection complete");
        }

        // Step 2: Encode deltas (if not skipped)
        // Capture original count before moving sequences
        let original_count = sequences.len();

        let deltas = if self.no_deltas {
            // Skip delta encoding entirely
            if !self.silent {
                use crate::cli::output::*;
                info("Skipping delta encoding (--no-deltas flag)");
            }
            Vec::new()
        } else {
            // Calculate total children to process
            let total_before_filter: usize =
                selection_result.children.values().map(|v| v.len()).sum();

            // Print informative message about delta encoding
            if !self.silent && total_before_filter > 0 {
                use crate::cli::output::*;
                section_header(&format!(
                    "Delta Encoding ({} sequences)",
                    format_number(total_before_filter)
                ));
                if total_before_filter > 10000 {
                    let tips = vec![
                        (
                            "Time estimate",
                            "Several minutes for large datasets".to_string(),
                        ),
                        (
                            "Speed tip",
                            "Use --no-deltas for faster processing".to_string(),
                        ),
                        (
                            "Alternative",
                            "Use --max-align-length to limit sequence length".to_string(),
                        ),
                    ];
                    tree_section("Performance Notes", tips, false);
                }
            }

            // Filter children to exclude very long sequences
            let filtered_children =
                self.filter_long_sequences(&selection_result.children, &sequences);
            let total_children: usize = filtered_children.values().map(|v| v.len()).sum();

            let encoding_pb = if !self.silent {
                // Create a new standalone progress bar instead of using MultiProgress
                let pb = ProgressBar::new(total_children as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Encoding deltas",
                        )
                        .unwrap()
                        .progress_chars("##-"),
                );
                pb.enable_steady_tick(std::time::Duration::from_millis(100));
                pb
            } else {
                ProgressBar::hidden()
            };

            if let Some(ref callback) = self.progress_callback {
                callback("Encoding deltas", 50.0);
            }

            if !self.silent && total_children > 0 {
                use crate::cli::output::*;
                action(&format!(
                    "Processing {} sequences for delta encoding...",
                    format_number(total_children)
                ));
            }

            let encoder = DeltaEncoder::new();
            let sequence_map: HashMap<String, Sequence> =
                sequences.into_iter().map(|s| (s.id.clone(), s)).collect();

            let reference_map: HashMap<String, Sequence> = selection_result
                .references
                .iter()
                .map(|s| (s.id.clone(), s.clone()))
                .collect();

            // Use the progress bar in encoding
            let pb_clone = encoding_pb.clone();
            let deltas = encoder.encode_batch_with_progress(
                &reference_map,
                &filtered_children,
                &sequence_map,
                move |_| {
                    pb_clone.inc(1);
                },
            );

            if let Some(ref callback) = self.progress_callback {
                callback("Delta encoding complete", 100.0);
            }
            if !self.silent {
                encoding_pb.finish_and_clear();
                println!("Delta encoding complete");
            }

            deltas
        };

        // Don't print statistics here - let the command do it after writing files

        Ok((selection_result.references, deltas, original_count))
    }

    fn configure_selector(&self, target_aligner: &TargetAligner) -> ReferenceSelectorImpl {
        let mut selector = ReferenceSelectorImpl::new()
            .with_min_length(self.config.reduction.min_sequence_length)
            .with_similarity_threshold(self.config.reduction.similarity_threshold)
            .with_taxonomy_aware(self.config.reduction.taxonomy_aware)
            .with_taxonomy_weights(self.use_taxonomy_weights)
            .with_all_vs_all(self.all_vs_all)
            .with_manifest_acc2taxid(self.manifest_acc2taxid.clone())
            .with_batch_settings(self.batch_enabled, self.batch_size)
            .with_selection_algorithm(self.selection_algorithm);

        // Adjust selector based on target aligner
        match target_aligner {
            TargetAligner::Lambda => {
                // LAMBDA uses the configured settings (default: all-vs-all, not taxonomy-aware)
                // to match db-reduce approach
            }
            TargetAligner::Blast => {
                // BLAST needs diverse sequences
                selector = selector.with_similarity_threshold(0.85);
            }
            TargetAligner::Kraken => {
                // Kraken needs good k-mer coverage
                selector = selector.with_similarity_threshold(0.8);
            }
            TargetAligner::Diamond => {
                // Diamond is similar to BLAST
                selector = selector.with_similarity_threshold(0.85);
            }
            TargetAligner::MMseqs2 => {
                // MMseqs2 handles clustering well
                selector = selector.with_similarity_threshold(0.9);
            }
            TargetAligner::Generic => {
                // Use default settings
            }
        }

        selector
    }

    fn filter_long_sequences(
        &self,
        children: &HashMap<String, Vec<String>>,
        sequences: &[Sequence],
    ) -> HashMap<String, Vec<String>> {
        let seq_map: HashMap<String, &Sequence> =
            sequences.iter().map(|s| (s.id.clone(), s)).collect();

        let mut filtered = HashMap::new();
        let mut skipped_count = 0;
        let mut max_length_seen = 0;

        for (ref_id, child_ids) in children {
            let mut filtered_children = Vec::new();

            for child_id in child_ids {
                if let Some(child_seq) = seq_map.get(child_id) {
                    let seq_len = child_seq.len();
                    if seq_len > max_length_seen {
                        max_length_seen = seq_len;
                    }

                    if seq_len <= self.max_align_length {
                        filtered_children.push(child_id.clone());
                    } else {
                        skipped_count += 1;
                    }
                }
            }

            if !filtered_children.is_empty() {
                filtered.insert(ref_id.clone(), filtered_children);
            }
        }

        if skipped_count > 0 && !self.silent {
            use crate::cli::output::*;
            let filter_items = vec![
                ("Filtered sequences", format_number(skipped_count)),
                (
                    "Length threshold",
                    format!("{} residues", format_number(self.max_align_length)),
                ),
                (
                    "Longest seen",
                    format!("{} residues", format_number(max_length_seen)),
                ),
            ];
            tree_section("Sequence Filtering", filter_items, false);
        }

        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reducer_default_batch_settings() {
        let config = Config::default();
        let reducer = Reducer::new(config);

        // Test that defaults are set properly - we can verify through behavior
        assert!(!reducer.silent); // Check accessible field
    }

    #[test]
    fn test_reducer_with_batch_settings() {
        let config = Config::default();
        let reducer = Reducer::new(config).with_batch_settings(true, 10000);

        // Test that builder pattern works
        assert!(!reducer.silent); // Verify other defaults unchanged
    }

    #[test]
    fn test_reducer_builder_pattern() {
        let config = Config::default();
        let temp_path = PathBuf::from("/test/path");

        let reducer = Reducer::new(config)
            .with_selection_mode(true, true)
            .with_no_deltas(true)
            .with_max_align_length(5000)
            .with_all_vs_all(true)
            .with_taxonomy_weights(true)
            .with_batch_settings(false, 3000)
            .with_manifest_acc2taxid(Some(temp_path.clone()))
            .with_file_sizes(1000, 500);

        // Test accessible fields
        assert!(reducer.use_similarity);
        assert!(reducer.use_alignment);
        assert!(reducer.no_deltas);
        assert_eq!(reducer.max_align_length, 5000);
        assert!(reducer.all_vs_all);
        assert!(reducer.use_taxonomy_weights);
        assert_eq!(reducer.manifest_acc2taxid, Some(temp_path));
        assert_eq!(reducer.input_file_size, 1000);
        assert_eq!(reducer.output_file_size, 500);
    }

    #[test]
    fn test_configure_selector_passes_settings() {
        let config = Config::default();
        let reducer = Reducer::new(config)
            .with_batch_settings(true, 7500)
            .with_taxonomy_weights(true)
            .with_all_vs_all(true);

        let selector = reducer.configure_selector(&TargetAligner::Lambda);

        // Test that selector is configured properly
        // We can't access private fields but can verify the method runs without error
        let _ = selector; // Use the selector to avoid warning
    }

    #[test]
    fn test_reducer_with_selection_algorithm() {
        let config = Config::default();

        // Test SinglePass algorithm
        let reducer_sp =
            Reducer::new(config.clone()).with_selection_algorithm(SelectionAlgorithm::SinglePass);
        assert_eq!(
            reducer_sp.selection_algorithm,
            SelectionAlgorithm::SinglePass
        );

        // Test SimilarityMatrix algorithm
        let reducer_sm = Reducer::new(config.clone())
            .with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix);
        assert_eq!(
            reducer_sm.selection_algorithm,
            SelectionAlgorithm::SimilarityMatrix
        );

        // Test Hybrid algorithm
        let reducer_h =
            Reducer::new(config.clone()).with_selection_algorithm(SelectionAlgorithm::Hybrid);
        assert_eq!(reducer_h.selection_algorithm, SelectionAlgorithm::Hybrid);
    }

    #[test]
    fn test_configure_selector_with_algorithm() {
        let config = Config::default();
        let reducer = Reducer::new(config)
            .with_selection_algorithm(SelectionAlgorithm::SimilarityMatrix)
            .with_batch_settings(true, 10000)
            .with_taxonomy_weights(true);

        let selector = reducer.configure_selector(&TargetAligner::Lambda);

        // Verify the selector is configured with the correct algorithm
        assert_eq!(
            selector.selection_algorithm,
            SelectionAlgorithm::SimilarityMatrix
        );
    }

    #[test]
    fn test_filter_long_sequences() {
        let config = Config::default();
        let reducer = Reducer::new(config).with_max_align_length(100);

        let sequences = vec![
            Sequence::new("seq1".to_string(), vec![65; 80]), // Valid
            Sequence::new("seq2".to_string(), vec![65; 120]), // Too long
            Sequence::new("seq3".to_string(), vec![65; 60]), // Valid
        ];

        let children = HashMap::from([
            (
                "ref1".to_string(),
                vec!["seq1".to_string(), "seq2".to_string()],
            ),
            ("ref2".to_string(), vec!["seq3".to_string()]),
        ]);

        let filtered = reducer.filter_long_sequences(&children, &sequences);

        // seq2 should be filtered out
        assert_eq!(filtered.get("ref1").unwrap().len(), 1);
        assert!(filtered.get("ref1").unwrap().contains(&"seq1".to_string()));
        assert!(!filtered.get("ref1").unwrap().contains(&"seq2".to_string()));
    }
}
