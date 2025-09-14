/// Main reduction pipeline

use crate::bio::sequence::Sequence;
use crate::cli::TargetAligner;
use crate::core::{
    config::Config,
    delta_encoder::{DeltaEncoder, DeltaRecord},
    reference_selector::ReferenceSelector,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;

pub struct Reducer {
    config: Config,
    progress_callback: Option<Box<dyn Fn(&str, f64) + Send + Sync>>,
    use_similarity: bool,
    use_alignment: bool,
    silent: bool,
    no_deltas: bool,
    max_align_length: usize,
    input_file_size: u64,
    output_file_size: u64,
    all_vs_all: bool,
}

impl Reducer {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            progress_callback: None,
            use_similarity: false,
            use_alignment: false,
            silent: false,
            no_deltas: false,
            max_align_length: 10000,
            input_file_size: 0,
            output_file_size: 0,
            all_vs_all: false,
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
    
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self 
    where
        F: Fn(&str, f64) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }
    
    pub fn reduce(
        &self,
        sequences: Vec<Sequence>,
        reduction_ratio: f64,
        target_aligner: TargetAligner,
    ) -> Result<(Vec<Sequence>, Vec<DeltaRecord>, usize), crate::TalariaError> {
        let multi_progress = MultiProgress::new();
        
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
        
        let selector = self.configure_selector(&target_aligner);
        
        // Choose selection method based on configuration
        let selection_result = if reduction_ratio == 0.0 {
            // Auto-detection mode - no ratio specified
            // Check if LAMBDA is available for more accurate alignment-based selection
            if let Ok(manager) = crate::tools::ToolManager::new() {
                if manager.is_installed(crate::tools::Tool::Lambda) {
                    if !self.silent {
                        println!("Using LAMBDA aligner for accurate auto-detection...");
                    }
                    match selector.select_references_with_lambda(sequences.clone()) {
                        Ok(result) => result,
                        Err(e) => {
                            if !self.silent {
                                eprintln!("LAMBDA alignment failed: {}, falling back to k-mer based auto-detection", e);
                            }
                            selector.select_references_auto(sequences.clone())
                        }
                    }
                } else {
                    if !self.silent {
                        println!("Using k-mer based auto-detection (install LAMBDA for more accurate results)...");
                    }
                    selector.select_references_auto(sequences.clone())
                }
            } else {
                if !self.silent {
                    println!("Using k-mer based auto-detection...");
                }
                selector.select_references_auto(sequences.clone())
            }
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
            selection_pb.finish_with_message("Reference selection complete");
        }
        
        // Step 2: Encode deltas (if not skipped)
        // Capture original count before moving sequences
        let original_count = sequences.len();
        
        let deltas = if self.no_deltas {
            // Skip delta encoding entirely
            if !self.silent {
                println!("Skipping delta encoding (--no-deltas flag)");
            }
            Vec::new()
        } else {
            // Calculate total children to process
            let total_before_filter: usize = selection_result.children.values().map(|v| v.len()).sum();
            
            // Print informative message about delta encoding
            if !self.silent && total_before_filter > 0 {
                println!("\nStarting delta encoding for {} child sequences...", total_before_filter);
                if total_before_filter > 10000 {
                    println!("  Note: This may take several minutes for large datasets.");
                    println!("  Consider using --no-deltas for faster processing without reconstruction capability.");
                    println!("  Or use --max-align-length to limit alignment to shorter sequences.");
                }
            }
            
            // Filter children to exclude very long sequences
            let filtered_children = self.filter_long_sequences(&selection_result.children, &sequences);
            let total_children: usize = filtered_children.values().map(|v| v.len()).sum();
            
            let encoding_pb = if !self.silent {
                // Create a new standalone progress bar instead of using MultiProgress
                let pb = ProgressBar::new(total_children as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Encoding deltas")
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
                println!("  Processing {} sequences for delta encoding...", total_children);
            }
            
            let encoder = DeltaEncoder::new();
            let sequence_map: HashMap<String, Sequence> = sequences
                .into_iter()
                .map(|s| (s.id.clone(), s))
                .collect();
            
            let reference_map: HashMap<String, Sequence> = selection_result.references
                .iter()
                .map(|s| (s.id.clone(), s.clone()))
                .collect();
            
            // Use the progress bar in encoding
            let pb_clone = encoding_pb.clone();
            let deltas = encoder.encode_batch_with_progress(
                &reference_map, 
                &filtered_children, 
                &sequence_map,
                move |_| { pb_clone.inc(1); }
            );
            
            if let Some(ref callback) = self.progress_callback {
                callback("Delta encoding complete", 100.0);
            }
            if !self.silent {
                encoding_pb.finish_with_message("Delta encoding complete");
            }
            
            deltas
        };
        
        // Don't print statistics here - let the command do it after writing files
        
        Ok((selection_result.references, deltas, original_count))
    }
    
    fn configure_selector(&self, target_aligner: &TargetAligner) -> ReferenceSelector {
        let mut selector = ReferenceSelector::new()
            .with_min_length(self.config.reduction.min_sequence_length)
            .with_similarity_threshold(self.config.reduction.similarity_threshold)
            .with_taxonomy_aware(self.config.reduction.taxonomy_aware)
            .with_all_vs_all(self.all_vs_all);
        
        // Adjust selector based on target aligner
        match target_aligner {
            TargetAligner::Lambda => {
                // LAMBDA benefits from taxonomy-aware selection
                selector = selector.with_taxonomy_aware(true);
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
        sequences: &[Sequence]
    ) -> HashMap<String, Vec<String>> {
        let seq_map: HashMap<String, &Sequence> = sequences
            .iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        
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
            println!("  Filtered out {} sequences longer than {} residues", 
                     skipped_count, self.max_align_length);
            println!("  (longest sequence seen: {} residues)", max_length_seen);
        }
        
        filtered
    }
}