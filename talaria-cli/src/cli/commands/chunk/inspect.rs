/// Inspect SEQUOIA chunk distribution and taxonomic organization
use anyhow::Result;
use clap::Args;
use colored::*;
use std::collections::BTreeMap;
use std::path::Path;

use talaria_bio::taxonomy::{ncbi, TaxonomyDB};
use talaria_sequoia::manifest::Manifest;
use talaria_sequoia::{ManifestMetadata, TaxonId, TemporalManifest};
use crate::cli::formatting::format_number;
use crate::cli::visualize::{ascii_histogram_categorized, sparkline, CategoryCounts};
use talaria_sequoia::database::DatabaseManager;
use crate::cli::progress::create_spinner;

/// Trait for inspecting manifest contents
pub trait ManifestInspector {
    /// Inspect taxonomic organization of chunks
    fn inspect_taxonomy(&self, max_depth: usize) -> Result<String>;

    /// Inspect chunk size distribution
    fn inspect_chunk_distribution(&self) -> Result<String>;

    /// Inspect graph centrality metrics for reference selection
    fn inspect_centrality_metrics(&self) -> Result<String>;

    /// Inspect temporal timeline (requires base path context)
    fn inspect_temporal_timeline(&self, base_path: &Path, database: &str) -> Result<String>;
}

impl ManifestInspector for TemporalManifest {
    fn inspect_taxonomy(&self, _max_depth: usize) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Taxonomic Organization:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Group chunks by taxonomy level (keeping both name and ID)
        let mut taxonomy_groups: BTreeMap<(String, TaxonId), Vec<&ManifestMetadata>> = BTreeMap::new();

        // Try to load taxonomy database for better display
        let taxonomy_db = load_taxonomy_db().ok();

        // Group chunks by their primary taxon ID
        for chunk in &self.chunk_index {
            if let Some(first_taxon) = chunk.taxon_ids.first() {
                let name = get_taxonomy_name(*first_taxon, &taxonomy_db);
                taxonomy_groups
                    .entry((name, *first_taxon))
                    .or_default()
                    .push(chunk);
            }
        }

        // Convert to vector and categorize
        let mut all_groups: Vec<_> = taxonomy_groups
            .iter()
            .map(|((name, taxon_id), chunks)| {
                let total_sequences: usize = chunks.iter().map(|c| c.sequence_count).sum();
                let total_size: usize = chunks.iter().map(|c| c.size).sum();
                (
                    (name.clone(), *taxon_id),
                    chunks.clone(),
                    total_sequences,
                    total_size,
                )
            })
            .collect();

        // Sort by sequence count (descending)
        all_groups.sort_by(|a, b| b.2.cmp(&a.2));

        // Categorize taxa
        let mut model_organisms = Vec::new();
        let mut pathogens = Vec::new();
        let mut environmental = Vec::new();
        let mut others = Vec::new();

        for group in all_groups.iter() {
            let ((name, taxon_id), _, _, _) = group;

            // Categorize based on taxon ID or name patterns
            if is_model_organism(*taxon_id, name) {
                model_organisms.push(group.clone());
            } else if is_pathogen(*taxon_id, name) {
                pathogens.push(group.clone());
            } else if is_environmental(*taxon_id, name) {
                environmental.push(group.clone());
            } else {
                others.push(group.clone());
            }
        }

        // Display by category

        // Helper to format each category entry
        let format_category_entry = |name: &str,
                                     taxon_id: &TaxonId,
                                     chunks: &[&ManifestMetadata],
                                     total_sequences: usize,
                                     total_size: usize|
         -> String {
            let display_name = if name.starts_with("TaxID") {
                name.to_string()
            } else {
                format!("{} (taxid:{})", name, taxon_id.0)
            };
            format!(
                "    ├─ {} ({} chunks, {} sequences, {:.2} MB)\n",
                display_name.bold(),
                chunks.len(),
                format_number(total_sequences),
                total_size as f64 / (1024.0 * 1024.0)
            )
        };

        // Model Organisms
        if !model_organisms.is_empty() {
            output.push_str(&format!("\n  {} Model Organisms:\n", "●".green()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in
                model_organisms.iter().take(3)
            {
                output.push_str(&format_category_entry(
                    name,
                    taxon_id,
                    chunks,
                    *total_sequences,
                    *total_size,
                ));
            }
            if model_organisms.len() > 3 {
                output.push_str(&format!(
                    "    └─ ... {} more model organisms\n",
                    model_organisms.len() - 3
                ));
            }
        }

        // Pathogens
        if !pathogens.is_empty() {
            output.push_str(&format!("\n  {} Pathogens:\n", "●".yellow()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in pathogens.iter().take(3)
            {
                output.push_str(&format_category_entry(
                    name,
                    taxon_id,
                    chunks,
                    *total_sequences,
                    *total_size,
                ));
            }
            if pathogens.len() > 3 {
                output.push_str(&format!(
                    "    └─ ... {} more pathogens\n",
                    pathogens.len() - 3
                ));
            }
        }

        // Environmental
        if !environmental.is_empty() {
            output.push_str(&format!("\n  {} Environmental:\n", "●".blue()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in
                environmental.iter().take(3)
            {
                output.push_str(&format_category_entry(
                    name,
                    taxon_id,
                    chunks,
                    *total_sequences,
                    *total_size,
                ));
            }
            if environmental.len() > 3 {
                output.push_str(&format!(
                    "    └─ ... {} more environmental organisms\n",
                    environmental.len() - 3
                ));
            }
        }

        // Others - show top 3
        if !others.is_empty() {
            // Check if all organisms have unknown taxonomy
            let all_unknown = others.iter().all(|((_, taxon_id), _, _, _)| taxon_id.0 == 0);

            if all_unknown && model_organisms.is_empty() && pathogens.is_empty() && environmental.is_empty() {
                output.push_str(&format!("\n  {} Other Organisms:\n", "●".white()));
                output.push_str(&format!("    {} No taxonomy data available for this database\n", "ℹ".blue()));

                // Show summary for unknown organisms
                for ((name, taxon_id), chunks, total_sequences, total_size) in
                    others.iter().take(1)
                {
                    output.push_str(&format_category_entry(
                        name,
                        taxon_id,
                        chunks,
                        *total_sequences,
                        *total_size,
                    ));
                }
            } else {
                output.push_str(&format!("\n  {} Other Organisms:\n", "●".white()));
                let others_to_show = 3.min(others.len());
                for ((name, taxon_id), chunks, total_sequences, total_size) in
                    others.iter().take(others_to_show)
                {
                    output.push_str(&format_category_entry(
                        name,
                        taxon_id,
                        chunks,
                        *total_sequences,
                        *total_size,
                    ));
                }
                if others.len() > others_to_show {
                    output.push_str(&format!(
                        "    └─ ... {} more organisms\n",
                        others.len() - others_to_show
                    ));
                }
            }
        }

        output.push_str(&format!(
            "\nTotal chunks: {}\n",
            format_number(self.chunk_index.len()).cyan()
        ));
        output.push_str(&format!(
            "Total sequences: {}",
            format_number(
                self.chunk_index
                    .iter()
                    .map(|c| c.sequence_count)
                    .sum::<usize>()
            )
            .cyan()
        ));

        Ok(output)
    }

    fn inspect_chunk_distribution(&self) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Chunk Size Distribution:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Load taxonomy database for categorization
        let taxonomy_path = talaria_core::system::paths::talaria_databases_dir().join("taxonomy/current");
        let names_path = taxonomy_path.join("names.dmp");
        let nodes_path = taxonomy_path.join("nodes.dmp");

        let taxonomy_db: Option<TaxonomyDB> = if names_path.exists() && nodes_path.exists() {
            ncbi::parse_ncbi_taxonomy(&names_path, &nodes_path).ok()
        } else {
            None
        };

        // Initialize size buckets with category counts
        let mut tiny_counts = CategoryCounts::default(); // < 1MB
        let mut small_counts = CategoryCounts::default(); // 1-10MB
        let mut medium_counts = CategoryCounts::default(); // 10-50MB
        let mut large_counts = CategoryCounts::default(); // 50-200MB
        let mut xlarge_counts = CategoryCounts::default(); // > 200MB

        // Categorize each chunk by size and organism type
        for chunk in &self.chunk_index {
            let size_mb = chunk.size as f64 / (1024.0 * 1024.0);

            // Determine organism category for this chunk
            let mut is_model = false;
            let mut is_pathogen_chunk = false;
            let mut is_environmental_chunk = false;

            // Check each taxon in the chunk
            for taxon_id in &chunk.taxon_ids {
                let name = if let Some(ref db) = taxonomy_db {
                    if let Some(info) = db.get_taxon(taxon_id.0) {
                        info.scientific_name.clone()
                    } else {
                        format!("TaxID {}", taxon_id.0)
                    }
                } else {
                    format!("TaxID {}", taxon_id.0)
                };

                if is_model_organism(*taxon_id, &name) {
                    is_model = true;
                } else if is_pathogen(*taxon_id, &name) {
                    is_pathogen_chunk = true;
                } else if is_environmental(*taxon_id, &name) {
                    is_environmental_chunk = true;
                }
            }

            // Categorize into the appropriate size bucket with organism type
            let counts = if size_mb < 1.0 {
                &mut tiny_counts
            } else if size_mb < 10.0 {
                &mut small_counts
            } else if size_mb < 50.0 {
                &mut medium_counts
            } else if size_mb < 200.0 {
                &mut large_counts
            } else {
                &mut xlarge_counts
            };

            // Increment the appropriate category
            // Priority: model > pathogen > environmental > other
            if is_model {
                counts.model += 1;
            } else if is_pathogen_chunk {
                counts.pathogen += 1;
            } else if is_environmental_chunk {
                counts.environmental += 1;
            } else {
                counts.other += 1;
            }
        }

        // Build the categorized data
        let mut size_categories: Vec<(String, CategoryCounts)> = Vec::new();
        if tiny_counts.total() > 0 {
            size_categories.push(("< 1MB".to_string(), tiny_counts));
        }
        if small_counts.total() > 0 {
            size_categories.push(("1-10MB".to_string(), small_counts));
        }
        if medium_counts.total() > 0 {
            size_categories.push(("10-50MB".to_string(), medium_counts));
        }
        if large_counts.total() > 0 {
            size_categories.push(("50-200MB".to_string(), large_counts));
        }
        if xlarge_counts.total() > 0 {
            size_categories.push(("> 200MB".to_string(), xlarge_counts));
        }

        // Display categorized histogram
        let total_chunks = self.chunk_index.len();
        let histogram = ascii_histogram_categorized(&size_categories, 40, total_chunks);
        output.push_str(&histogram);

        // Show organism category legend
        output.push_str(&format!("\n{}\n", "Organism Categories:".bold()));
        output.push_str(&format!("  {} Model Organisms\n", "█".green()));
        output.push_str(&format!("  {} Pathogens\n", "█".yellow()));
        output.push_str(&format!("  {} Environmental\n", "█".blue()));
        output.push_str(&format!("  {} Other/Unknown", "█".white().dimmed()));

        Ok(output)
    }

    fn inspect_centrality_metrics(&self) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!(
            "{}\n",
            "Reference Selection Metrics:".bold().green()
        ));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Calculate basic metrics
        let total_chunks = self.chunk_index.len();
        if total_chunks == 0 {
            output.push_str("No chunks in manifest");
            return Ok(output);
        }

        let avg_sequences = self
            .chunk_index
            .iter()
            .map(|c| c.sequence_count)
            .sum::<usize>() as f64
            / total_chunks as f64;

        // Find chunks with most sequences (likely references)
        let mut sorted_chunks = self.chunk_index.clone();
        sorted_chunks.sort_by_key(|c| std::cmp::Reverse(c.sequence_count));

        output.push_str("Graph Centrality Formula:\n");
        output.push_str("  Score = α·Degree + β·Betweenness + γ·Coverage\n");
        output.push_str("  Weights: α=0.5, β=0.3, γ=0.2\n\n");

        output.push_str("Top Reference Candidates (by sequence count):\n");
        for (i, chunk) in sorted_chunks.iter().take(5).enumerate() {
            let estimated_centrality = calculate_estimated_centrality(chunk, avg_sequences);
            output.push_str(&format!(
                "  {}. Chunk {} \n",
                i + 1,
                &chunk.hash.to_string()[..8]
            ));
            output.push_str(&format!(
                "     Sequences: {}\n",
                format_number(chunk.sequence_count).cyan()
            ));
            output.push_str(&format!(
                "     Size: {:.2} MB\n",
                (chunk.size as f64 / (1024.0 * 1024.0))
            ));
            output.push_str(&format!("     Taxa: {} species\n", chunk.taxon_ids.len()));
            output.push_str(&format!(
                "     Est. Centrality: {:.3}\n",
                estimated_centrality
            ));
        }

        // Show coverage statistics
        let total_sequences: usize = self.chunk_index.iter().map(|c| c.sequence_count).sum();
        let reference_coverage = sorted_chunks
            .iter()
            .take(10)
            .map(|c| c.sequence_count)
            .sum::<usize>() as f64
            / total_sequences as f64;

        output.push_str("\nCoverage Statistics:\n");
        output.push_str(&format!(
            "  Total sequences: {}\n",
            format_number(total_sequences).cyan()
        ));
        output.push_str(&format!(
            "  Top 10 chunks coverage: {:.1}%",
            reference_coverage * 100.0
        ));

        Ok(output)
    }

    fn inspect_temporal_timeline(&self, base_path: &Path, database: &str) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!(
            "{}\n",
            "Temporal Version Timeline:".bold().green()
        ));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        let temporal_dir = base_path.join("temporal");

        // Try to load temporal data
        if temporal_dir.exists() {
            let timeline_file = temporal_dir.join("sequence_timeline.json");
            if timeline_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&timeline_file) {
                    if let Ok(timeline) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(versions) = timeline.as_array() {
                            let version_counts: Vec<f64> = versions
                                .iter()
                                .filter_map(|v| v.get("sequence_count").and_then(|c| c.as_f64()))
                                .collect();

                            if !version_counts.is_empty() {
                                let spark = sparkline(&version_counts, 40);
                                output.push_str(&format!("Sequence Evolution: {}\n", spark));
                                output
                                    .push_str(&format!("  Versions tracked: {}\n", versions.len()));
                            }
                        }
                    }
                }
            }

            // Show taxonomy timeline
            let tax_timeline_file = temporal_dir.join("taxonomy_timeline.json");
            if tax_timeline_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&tax_timeline_file) {
                    if let Ok(timeline) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(versions) = timeline.as_array() {
                            output.push_str(&format!("Taxonomy versions: {}\n", versions.len()));
                        }
                    }
                }
            }
        } else {
            output.push_str("  No temporal tracking data available\n");
        }

        // Show version history
        let versions_dir = base_path.join("versions").join(database);
        if versions_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&versions_dir) {
                let mut versions: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name();
                        let name_str = name.to_string_lossy();
                        e.path().is_dir() && name_str != "current"
                    })
                    .collect();

                versions.sort_by_key(|e| e.file_name());

                output.push_str("\nVersion History:\n");
                for (i, version) in versions.iter().rev().take(5).enumerate() {
                    let version_name = version.file_name();
                    if let Ok(metadata) = version.metadata() {
                        let created_time = metadata
                            .created()
                            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let datetime = chrono::DateTime::<chrono::Utc>::from(created_time);
                        output.push_str(&format!(
                            "  {}. {} (created: {})\n",
                            i + 1,
                            version_name.to_string_lossy().cyan(),
                            datetime.format("%Y-%m-%d %H:%M:%S UTC")
                        ));
                    }
                }
            }
        }

        Ok(output)
    }
}

impl ManifestInspector for talaria_sequoia::ReductionManifest {
    fn inspect_taxonomy(&self, max_depth: usize) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Taxonomic Organization:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Group reference chunks by taxonomy
        let mut taxonomy_groups: BTreeMap<(String, TaxonId), Vec<&talaria_sequoia::operations::ReferenceChunk>> = BTreeMap::new();

        // Try to load taxonomy database for better display
        let taxonomy_db = load_taxonomy_db().ok();

        // Group chunks by their taxon IDs
        for chunk in &self.reference_chunks {
            if let Some(first_taxon) = chunk.taxon_ids.first() {
                let name = get_taxonomy_name(*first_taxon, &taxonomy_db);
                taxonomy_groups
                    .entry((name, *first_taxon))
                    .or_default()
                    .push(chunk);
            }
        }

        // Convert to vector and categorize
        let mut all_groups: Vec<_> = taxonomy_groups
            .iter()
            .map(|((name, taxon_id), chunks)| {
                let total_sequences: usize = chunks.iter().map(|c| c.sequence_count).sum();
                let total_size: usize = chunks.iter().map(|c| c.size).sum();
                (
                    (name.clone(), *taxon_id),
                    chunks.clone(),
                    total_sequences,
                    total_size,
                )
            })
            .collect();

        // Sort by sequence count (descending)
        all_groups.sort_by(|a, b| b.2.cmp(&a.2));

        // Categorize taxa
        let mut model_organisms = Vec::new();
        let mut pathogens = Vec::new();
        let mut environmental = Vec::new();
        let mut others = Vec::new();

        for group in all_groups.iter() {
            let ((name, taxon_id), _, _, _) = group;

            if is_model_organism(*taxon_id, name) {
                model_organisms.push(group.clone());
            } else if is_pathogen(*taxon_id, name) {
                pathogens.push(group.clone());
            } else if is_environmental(*taxon_id, name) {
                environmental.push(group.clone());
            } else {
                others.push(group.clone());
            }
        }

        // Helper to format entries
        let format_entry = |name: &str, taxon_id: &TaxonId, chunks: &[&talaria_sequoia::operations::ReferenceChunk], total_sequences: usize, total_size: usize| -> String {
            let display_name = if name.starts_with("TaxID") {
                name.to_string()
            } else {
                format!("{} (taxid:{})", name, taxon_id.0)
            };
            format!(
                "    ├─ {} ({} chunks, {} sequences, {:.2} MB)\n",
                display_name.bold(),
                chunks.len(),
                format_number(total_sequences),
                total_size as f64 / 1_048_576.0
            )
        };

        // Display categories
        if !model_organisms.is_empty() {
            output.push_str(&format!("\n  {} Model Organisms:\n", "●".green()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in model_organisms.iter().take(max_depth.min(5)) {
                output.push_str(&format_entry(name, taxon_id, chunks, *total_sequences, *total_size));
            }
            if model_organisms.len() > 5 {
                output.push_str(&format!("    └─ ... {} more model organisms\n", model_organisms.len() - 5));
            }
        }

        if !pathogens.is_empty() {
            output.push_str(&format!("\n  {} Pathogens:\n", "●".red()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in pathogens.iter().take(max_depth.min(5)) {
                output.push_str(&format_entry(name, taxon_id, chunks, *total_sequences, *total_size));
            }
            if pathogens.len() > 5 {
                output.push_str(&format!("    └─ ... {} more pathogens\n", pathogens.len() - 5));
            }
        }

        if !environmental.is_empty() {
            output.push_str(&format!("\n  {} Environmental:\n", "●".blue()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in environmental.iter().take(max_depth.min(5)) {
                output.push_str(&format_entry(name, taxon_id, chunks, *total_sequences, *total_size));
            }
            if environmental.len() > 5 {
                output.push_str(&format!("    └─ ... {} more environmental organisms\n", environmental.len() - 5));
            }
        }

        if !others.is_empty() {
            output.push_str(&format!("\n  {} Other Organisms:\n", "●".yellow()));
            for ((name, taxon_id), chunks, total_sequences, total_size) in others.iter().take(max_depth.min(5)) {
                output.push_str(&format_entry(name, taxon_id, chunks, *total_sequences, *total_size));
            }
            if others.len() > 5 {
                output.push_str(&format!("    └─ ... {} more organisms\n", others.len() - 5));
            }
        }

        // Summary
        output.push_str(&format!("\nTotal reference chunks: {}\n", format_number(self.reference_chunks.len()).cyan()));
        output.push_str(&format!("Total reference sequences: {}\n", format_number(self.statistics.reference_sequences).cyan()));
        if !self.delta_chunks.is_empty() {
            output.push_str(&format!("Delta chunks: {}\n", format_number(self.delta_chunks.len()).cyan()));
            output.push_str(&format!("Delta-encoded sequences: {}\n", format_number(self.statistics.child_sequences).cyan()));
        }

        Ok(output)
    }

    fn inspect_chunk_distribution(&self) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Chunk Size Distribution:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Load taxonomy database for categorization
        let taxonomy_db = load_taxonomy_db().ok();

        // Initialize size buckets with category counts
        let mut tiny_counts = CategoryCounts::default(); // < 1MB
        let mut small_counts = CategoryCounts::default(); // 1-10MB
        let mut medium_counts = CategoryCounts::default(); // 10-50MB
        let mut large_counts = CategoryCounts::default(); // 50-100MB
        let mut xlarge_counts = CategoryCounts::default(); // > 100MB

        // Categorize reference chunks by size and organism type
        for chunk in &self.reference_chunks {
            let size_mb = chunk.size as f64 / (1024.0 * 1024.0);

            // Determine organism category for this chunk
            let mut is_model = false;
            let mut is_pathogen_chunk = false;
            let mut is_environmental_chunk = false;

            // Check each taxon in the chunk
            for taxon_id in &chunk.taxon_ids {
                let name = get_taxonomy_name(*taxon_id, &taxonomy_db);

                if is_model_organism(*taxon_id, &name) {
                    is_model = true;
                } else if is_pathogen(*taxon_id, &name) {
                    is_pathogen_chunk = true;
                } else if is_environmental(*taxon_id, &name) {
                    is_environmental_chunk = true;
                }
            }

            // Categorize into the appropriate size bucket with organism type
            let counts = if size_mb < 1.0 {
                &mut tiny_counts
            } else if size_mb < 10.0 {
                &mut small_counts
            } else if size_mb < 50.0 {
                &mut medium_counts
            } else if size_mb < 100.0 {
                &mut large_counts
            } else {
                &mut xlarge_counts
            };

            // Increment the appropriate category
            // Priority: model > pathogen > environmental > other
            if is_model {
                counts.model += 1;
            } else if is_pathogen_chunk {
                counts.pathogen += 1;
            } else if is_environmental_chunk {
                counts.environmental += 1;
            } else {
                counts.other += 1;
            }
        }

        // Also add delta chunks (as "other" category since they don't have taxon info)
        for delta in &self.delta_chunks {
            let size_mb = delta.size as f64 / (1024.0 * 1024.0);

            let counts = if size_mb < 1.0 {
                &mut tiny_counts
            } else if size_mb < 10.0 {
                &mut small_counts
            } else if size_mb < 50.0 {
                &mut medium_counts
            } else if size_mb < 100.0 {
                &mut large_counts
            } else {
                &mut xlarge_counts
            };

            counts.other += 1;
        }

        // Build the categorized data
        let mut size_categories: Vec<(String, CategoryCounts)> = Vec::new();
        if tiny_counts.total() > 0 {
            size_categories.push(("< 1MB".to_string(), tiny_counts));
        }
        if small_counts.total() > 0 {
            size_categories.push(("1-10MB".to_string(), small_counts));
        }
        if medium_counts.total() > 0 {
            size_categories.push(("10-50MB".to_string(), medium_counts));
        }
        if large_counts.total() > 0 {
            size_categories.push(("50-100MB".to_string(), large_counts));
        }
        if xlarge_counts.total() > 0 {
            size_categories.push(("> 100MB".to_string(), xlarge_counts));
        }

        // Display categorized histogram
        let total_chunks = self.reference_chunks.len() + self.delta_chunks.len();
        let histogram = ascii_histogram_categorized(&size_categories, 40, total_chunks);
        output.push_str(&histogram);

        // Show organism category legend
        output.push_str(&format!("\n{}\n", "Organism Categories:".bold()));
        output.push_str(&format!("  {} Model Organisms\n", "█".green()));
        output.push_str(&format!("  {} Pathogens\n", "█".yellow()));
        output.push_str(&format!("  {} Environmental\n", "█".blue()));
        output.push_str(&format!("  {} Other/Unknown", "█".white().dimmed()));

        Ok(output)
    }

    fn inspect_centrality_metrics(&self) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Reference Selection Metrics:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Display reduction parameters used
        output.push_str("Reduction Strategy:\n");
        if self.parameters.taxonomy_aware {
            output.push_str("  ✓ Taxonomy-aware clustering\n");
        }
        if self.parameters.align_select {
            output.push_str("  ✓ Alignment-based selection\n");
        }
        output.push_str(&format!("  Similarity threshold: {:.1}%\n", self.parameters.similarity_threshold * 100.0));

        output.push_str(&format!("\nTop Reference Chunks (by sequence count):\n"));

        // Sort reference chunks by sequence count
        let mut sorted_refs = self.reference_chunks.clone();
        sorted_refs.sort_by(|a, b| b.sequence_count.cmp(&a.sequence_count));

        for (i, chunk) in sorted_refs.iter().take(5).enumerate() {
            output.push_str(&format!(
                "  {}. Chunk {} \n",
                i + 1,
                &chunk.chunk_hash.to_hex()[..8]
            ));
            output.push_str(&format!("     Sequences: {}\n", format_number(chunk.sequence_count).cyan()));
            output.push_str(&format!("     Size: {:.2} MB\n", chunk.size as f64 / 1_048_576.0));
            if !chunk.taxon_ids.is_empty() {
                output.push_str(&format!("     Taxa: {} species\n", chunk.taxon_ids.len()));
            }
        }

        // Coverage statistics
        output.push_str(&format!("\nCoverage Statistics:\n"));
        output.push_str(&format!("  Total sequences: {}\n", format_number(self.statistics.original_sequences).cyan()));
        output.push_str(&format!("  Reference sequences: {} ({:.1}%)\n",
            format_number(self.statistics.reference_sequences).cyan(),
            (self.statistics.reference_sequences as f64 / self.statistics.original_sequences as f64) * 100.0
        ));

        Ok(output)
    }

    fn inspect_temporal_timeline(&self, _base_path: &Path, _database: &str) -> Result<String> {
        let mut output = String::new();
        output.push_str(&format!("{}\n", "Temporal Version Timeline:".bold().green()));
        output.push_str(&format!("{}\n", "─".repeat(40)));

        // Display creation and version info
        output.push_str(&format!("\nReduction Profile: {}\n", self.profile));
        output.push_str(&format!("Created: {}\n", self.created_at.format("%Y-%m-%d %H:%M:%S UTC")));
        output.push_str(&format!("Version: {}\n", self.version));

        if let Some(prev) = &self.previous_version {
            output.push_str(&format!("Previous version: {}\n", prev.to_hex()));
        }

        Ok(output)
    }
}

#[derive(Args)]
pub struct InspectArgs {
    /// Database to inspect (e.g., uniprot/swissprot or uniprot/swissprot:2024-03-15)
    pub database: String,

    /// Output format
    #[arg(short, long, value_enum, default_value = "summary")]
    pub format: OutputFormat,

    /// Show taxonomic tree visualization
    #[arg(long)]
    pub tree: bool,

    /// Show chunk size distribution histogram
    #[arg(long)]
    pub histogram: bool,

    /// Show graph centrality metrics
    #[arg(long)]
    pub centrality: bool,

    /// Show temporal version timeline
    #[arg(long)]
    pub temporal: bool,

    /// Show all visualizations
    #[arg(long)]
    pub all: bool,

    /// Maximum number of taxa to display (sorted by sequence count)
    #[arg(short = 'd', long, default_value = "10")]
    pub max_depth: usize,

    /// Path to TALARIA_HOME
    #[arg(long)]
    pub talaria_home: Option<String>,
}

// Use OutputFormat from talaria-core
use talaria_core::OutputFormat;

pub fn run(args: InspectArgs) -> Result<()> {
    // Parse database, version, and profile
    let (database, version, profile) = parse_database_spec(&args.database)?;

    println!(
        "{}",
        format!("SEQUOIA Chunk Inspection: {}", args.database)
            .bold()
            .cyan()
    );
    println!("{}", "═".repeat(60));
    println!();

    // Load database manager for initialization
    let spinner = create_spinner("Analyzing database structure...");
    let _manager = DatabaseManager::new(args.talaria_home.clone())?;

    // Parse database name (e.g., "uniprot/swissprot")
    let db_parts: Vec<&str> = database.split('/').collect();
    let (source, dataset) = if db_parts.len() == 2 {
        (db_parts[0], db_parts[1])
    } else {
        // Assume custom source with single name
        ("custom", database.as_str())
    };

    // Get database path from versions directory
    let base_path = talaria_core::system::paths::talaria_databases_dir();
    let db_path = if let Some(ver) = &version {
        // Version specified
        base_path.join("versions").join(source).join(dataset).join(ver)
    } else {
        // Default to current version
        base_path.join("versions").join(source).join(dataset).join("current")
    };

    if !db_path.exists() {
        anyhow::bail!("Database {} not found at {:?}", database, db_path);
    }

    // Check if this is a profile manifest first
    if let Some(prof) = &profile {
        // This is a reduction profile - handle it separately
        spinner.finish_and_clear();
        return handle_profile_manifest(&db_path, &database, version.as_deref(), prof, &args);
    }

    // Load database manifest (TemporalManifest)
    let manifest_path = db_path.join("manifest.tal");
    if !manifest_path.exists() {
        spinner.finish_and_clear();
        anyhow::bail!("No manifest.tal found in database directory");
    }

    spinner.set_message("Loading manifest...");

    // This is a database manifest - load as TemporalManifest
    let manifest_wrapper = Manifest::load_file(&manifest_path)?;
    let manifest = manifest_wrapper
        .get_data()
        .ok_or_else(|| anyhow::anyhow!("Manifest loaded but contains no data"))?;
    spinner.finish_and_clear();

    // Determine which visualizations to show
    let show_tree = args.tree || args.all;
    let show_histogram = args.histogram || args.all;
    let show_centrality = args.centrality || args.all;
    let show_temporal = args.temporal || args.all;

    // Display based on format
    match args.format {
        OutputFormat::Summary => {
            display_summary(&db_path, &database, version.as_deref(), profile.as_deref())?;

            if show_tree {
                println!();
                println!("{}", manifest.inspect_taxonomy(args.max_depth)?);
            }

            if show_histogram {
                println!();
                println!("{}", manifest.inspect_chunk_distribution()?);
            }

            if show_centrality {
                println!();
                println!("{}", manifest.inspect_centrality_metrics()?);
            }

            if show_temporal {
                println!();
                println!(
                    "{}",
                    manifest.inspect_temporal_timeline(&base_path, &database)?
                );
            }
        }
        OutputFormat::Detailed => {
            display_detailed(&db_path, &database, version.as_deref(), profile.as_deref(), true)?;

            // Show all visualizations in detailed mode
            println!();
            println!("{}", manifest.inspect_taxonomy(args.max_depth)?);
            println!();
            println!("{}", manifest.inspect_chunk_distribution()?);
            println!();
            println!("{}", manifest.inspect_centrality_metrics()?);
            println!();
            println!(
                "{}",
                manifest.inspect_temporal_timeline(&base_path, &database)?
            );
        }
        OutputFormat::Json => display_json(&db_path, &database, version.as_deref())?,
        OutputFormat::Text | OutputFormat::Yaml | OutputFormat::Csv | OutputFormat::Tsv | OutputFormat::Fasta | OutputFormat::HashOnly => {
            // Default to summary display for unsupported formats
            display_summary(&db_path, &database, version.as_deref(), profile.as_deref())?;
        }
    }

    Ok(())
}

/// Parse database specification (database[@version][:profile])
/// Handle profile manifest inspection
fn handle_profile_manifest(
    db_path: &Path,
    database: &str,
    version: Option<&str>,
    profile: &str,
    args: &InspectArgs,
) -> Result<()> {
    use talaria_sequoia::ReductionManifest;
    use anyhow::Context;

    // Load the reduction manifest
    let profile_path = db_path.join("profiles").join(format!("{}.tal", profile));
    if !profile_path.exists() {
        anyhow::bail!("Profile manifest not found at {:?}", profile_path);
    }

    // Read and parse the reduction manifest
    let mut content = std::fs::read(&profile_path)?;

    // Skip TAL header if present
    if content.starts_with(b"TAL") && content.len() > 4 {
        content = content[4..].to_vec();
    }

    let reduction_manifest: ReductionManifest = rmp_serde::from_slice(&content)
        .context("Failed to parse reduction manifest")?;

    // Determine which visualizations to show
    let show_tree = args.tree || args.all;
    let show_histogram = args.histogram || args.all;
    let show_centrality = args.centrality || args.all;
    let show_temporal = args.temporal || args.all;

    // Display based on format
    match args.format {
        OutputFormat::Summary => {
            // Display basic information
            println!("{}", "Reduction Profile Information:".bold().green());
            println!("  Profile: {}", profile.cyan());
            println!("  Database: {}", reduction_manifest.source_database);
            if let Some(v) = version {
                println!("  Version: {}", v);
            }
            println!();

            println!("{}", "Reduction Statistics:".bold().green());
            println!("  Original sequences: {}", format_number(reduction_manifest.statistics.original_sequences));
            println!("  Reference sequences: {}", format_number(reduction_manifest.statistics.reference_sequences));
            println!("  Delta-encoded sequences: {}", format_number(reduction_manifest.statistics.child_sequences));
            println!("  Reduction ratio: {:.1}%", reduction_manifest.statistics.actual_reduction_ratio * 100.0);

            println!("\n{}", "Size Information:".bold().green());
            println!("  Original size: {:.2} MB", reduction_manifest.statistics.original_size as f64 / 1_048_576.0);
            println!("  Reduced size (references only): {:.2} MB", reduction_manifest.statistics.reduced_size as f64 / 1_048_576.0);
            println!("  Total size with deltas: {:.2} MB", reduction_manifest.statistics.total_size_with_deltas as f64 / 1_048_576.0);

            // Use ManifestInspector trait methods for visualizations
            if show_tree {
                println!();
                println!("{}", reduction_manifest.inspect_taxonomy(args.max_depth)?);
            }

            if show_histogram {
                println!();
                println!("{}", reduction_manifest.inspect_chunk_distribution()?);
            }

            if show_centrality {
                println!();
                println!("{}", reduction_manifest.inspect_centrality_metrics()?);
            }

            if show_temporal {
                println!();
                println!("{}", reduction_manifest.inspect_temporal_timeline(&db_path, database)?);
            }

            // If no specific visualizations requested, show basic metadata
            if !show_tree && !show_histogram && !show_centrality && !show_temporal {
                println!("\n{}", "Storage Structure:".bold().green());
                println!("  Reference chunks: {}", format_number(reduction_manifest.reference_chunks.len()));
                println!("  Delta chunks: {}", format_number(reduction_manifest.delta_chunks.len()));
                println!("  Source manifest: {}", reduction_manifest.source_manifest.to_hex());

                println!("\n{}", "Metadata:".bold().green());
                println!("  Reduction ID: {}", reduction_manifest.reduction_id.to_hex());
                println!("  Created: {}", reduction_manifest.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
            }
        }
        OutputFormat::Json => {
            // Output as JSON
            println!("{}", serde_json::to_string_pretty(&reduction_manifest)?);
        }
        OutputFormat::Detailed => {
            // Show everything
            println!("{}", "Reduction Profile Information:".bold().green());
            println!("  Profile: {}", profile.cyan());
            println!("  Database: {}", reduction_manifest.source_database);
            if let Some(v) = version {
                println!("  Version: {}", v);
            }

            // Show all visualizations
            println!();
            println!("{}", reduction_manifest.inspect_taxonomy(args.max_depth)?);
            println!();
            println!("{}", reduction_manifest.inspect_chunk_distribution()?);
            println!();
            println!("{}", reduction_manifest.inspect_centrality_metrics()?);
            println!();
            println!("{}", reduction_manifest.inspect_temporal_timeline(&db_path, database)?);
        }
        OutputFormat::Text | OutputFormat::Yaml | OutputFormat::Csv | OutputFormat::Tsv | OutputFormat::Fasta | OutputFormat::HashOnly => {
            // Default to summary display for unsupported formats
            println!("{}", "Reduction Profile Information:".bold().green());
            println!("  Profile: {}", profile.cyan());
            println!("  Database: {}", reduction_manifest.source_database);
            if let Some(v) = version {
                println!("  Version: {}", v);
            }
        }
    }

    Ok(())
}

fn parse_database_spec(spec: &str) -> Result<(String, Option<String>, Option<String>)> {
    // First split by @ for version
    let (base, version) = if let Some(at_idx) = spec.find('@') {
        let base = &spec[..at_idx];
        let version = &spec[at_idx + 1..];
        (base, Some(version.to_string()))
    } else {
        (spec, None)
    };

    // Then split base by : for profile
    let (database, profile) = if let Some((db, prof)) = base.split_once(':') {
        (db.to_string(), Some(prof.to_string()))
    } else {
        (base.to_string(), None)
    };

    Ok((database, version, profile))
}

/// Display summary information
fn display_summary(db_path: &Path, database: &str, version: Option<&str>, profile: Option<&str>) -> Result<()> {
    // Database version files
    let manifest_path = db_path.join("manifest.tal");
    let version_json = db_path.join("version.json");

    // Global database directories
    let base_path = talaria_core::system::paths::talaria_databases_dir();
    let chunks_dir = base_path.join("chunks");
    let temporal_dir = base_path.join("temporal");

    println!("Database Information:");
    println!("  Name: {}", database);
    if let Some(ver) = version {
        println!("  Version: {}", ver);
    }
    if let Some(prof) = profile {
        println!("  Profile: {}", prof);
    }
    println!("  Path: {}", db_path.display());
    println!();

    println!("SEQUOIA Structure:");
    if manifest_path.exists() {
        println!("  {} Manifest found", "✓".green());
        let size = std::fs::metadata(&manifest_path)?.len();
        println!("    Size: {}", talaria_utils::display::format::format_bytes(size));
    } else {
        println!("  {} No manifest found", "✗".red());
    }

    // Read version.json for additional info
    if version_json.exists() {
        if let Ok(version_str) = std::fs::read_to_string(&version_json) {
            if let Ok(version_data) = serde_json::from_str::<serde_json::Value>(&version_str) {
                if let Some(created) = version_data.get("created").and_then(|v| v.as_str()) {
                    println!("  Created: {}", created);
                }
                if let Some(chunk_count) = version_data.get("chunk_count").and_then(|v| v.as_u64())
                {
                    println!("  Chunks: {}", chunk_count);
                }
                if let Some(total_size) = version_data.get("total_size").and_then(|v| v.as_u64()) {
                    println!(
                        "  Total Size: {:.2} MB",
                        total_size as f64 / (1024.0 * 1024.0)
                    );
                }
            }
        }
    }

    // Check for chunks directory
    if chunks_dir.exists() {
        let mut chunk_files = Vec::new();
        collect_chunk_files(&chunks_dir, &mut chunk_files)?;

        println!("  {} Chunks directory", "✓".green());
        println!("    Chunk files: {}", chunk_files.len());

        let total_size: u64 = chunk_files.iter().map(|(_, size)| size).sum();
        println!(
            "    Total size: {:.2} MB",
            total_size as f64 / (1024.0 * 1024.0)
        );
    } else {
        println!("  {} No chunks directory", "✗".red());
    }

    // Show chunk distribution
    println!();
    println!("Chunk Distribution (by hash prefix):");

    // Check for temporal tracking
    if temporal_dir.exists() {
        println!("  {} Temporal tracking enabled", "✓".green());
        let files: Vec<_> = std::fs::read_dir(&temporal_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();
        println!("    Temporal versions: {}", files.len());
    } else {
        println!("  {} No temporal tracking", "✗".red());
    }

    Ok(())
}

/// Display detailed information
fn display_detailed(
    db_path: &Path,
    database: &str,
    version: Option<&str>,
    profile: Option<&str>,
    verbose: bool,
) -> Result<()> {
    display_summary(db_path, database, version, profile)?;

    let base_path = talaria_core::system::paths::talaria_databases_dir();
    let chunks_dir = base_path.join("chunks");
    if !chunks_dir.exists() {
        return Ok(());
    }

    println!();
    println!("Detailed Chunk Information:");
    println!("{}", "─".repeat(40));

    // Collect all chunk files
    let mut chunk_files = Vec::new();
    collect_chunk_files(&chunks_dir, &mut chunk_files)?;

    // Sort by size for display
    chunk_files.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    // Show top chunks
    let display_count = if verbose { 20 } else { 10 };
    for (i, (path, size)) in chunk_files.iter().take(display_count).enumerate() {
        if let Some(filename) = path.file_name() {
            println!(
                "  {}. {} ({:.2} MB)",
                i + 1,
                filename.to_string_lossy(),
                *size as f64 / (1024.0 * 1024.0)
            );
        }
    }

    if chunk_files.len() > display_count {
        println!(
            "  ... and {} more chunks",
            chunk_files.len() - display_count
        );
    }

    Ok(())
}

/// Display JSON format
fn display_json(db_path: &Path, database: &str, version: Option<&str>) -> Result<()> {
    use serde_json::json;

    let base_path = talaria_core::system::paths::talaria_databases_dir();
    let chunks_dir = base_path.join("chunks");
    let manifest_path = db_path.join("manifest.tal");
    let version_json_path = db_path.join("version.json");

    let mut output = json!({
        "database": database,
        "version": version,
        "path": db_path.to_string_lossy(),
    });

    // Read version.json for metadata
    if version_json_path.exists() {
        if let Ok(version_str) = std::fs::read_to_string(&version_json_path) {
            if let Ok(version_data) = serde_json::from_str::<serde_json::Value>(&version_str) {
                output["metadata"] = version_data;
            }
        }
    }

    if manifest_path.exists() {
        let size = std::fs::metadata(&manifest_path)?.len();
        output["manifest"] = json!({
            "exists": true,
            "size_bytes": size,
            "size": talaria_utils::display::format::format_bytes(size),
        });
    }

    if chunks_dir.exists() {
        let mut chunk_files = Vec::new();
        collect_chunk_files(&chunks_dir, &mut chunk_files)?;

        let total_size: u64 = chunk_files.iter().map(|(_, size)| size).sum();
        output["chunks"] = json!({
            "count": chunk_files.len(),
            "total_size_bytes": total_size,
            "total_size": talaria_utils::display::format::format_bytes(total_size),
        });

        // Include hash distribution
        let mut hash_distribution = BTreeMap::new();
        for (path, size) in &chunk_files {
            if let Some(filename) = path.file_name() {
                let name = filename.to_string_lossy();
                if name.len() >= 2 {
                    let prefix = &name[..2];
                    let entry = hash_distribution
                        .entry(prefix.to_string())
                        .or_insert((0, 0u64));
                    entry.0 += 1;
                    entry.1 += size;
                }
            }
        }

        output["hash_distribution"] = json!(hash_distribution);
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Collect all chunk files with their sizes
fn collect_chunk_files(dir: &Path, files: &mut Vec<(std::path::PathBuf, u64)>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_chunk_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "chunk" || ext == "zst")
        {
            let size = entry.metadata()?.len();
            files.push((path, size));
        }
    }
    Ok(())
}

// Helper functions used by the trait implementation

/// Check if a taxon is a model organism
fn is_model_organism(taxon_id: TaxonId, name: &str) -> bool {
    // Common model organism taxon IDs
    match taxon_id.0 {
        9606 => true,   // Homo sapiens (Human)
        10090 => true,  // Mus musculus (Mouse)
        10116 => true,  // Rattus norvegicus (Rat)
        7227 => true,   // Drosophila melanogaster (Fruit fly)
        6239 => true,   // Caenorhabditis elegans (Nematode)
        7955 => true,   // Danio rerio (Zebrafish)
        3702 => true,   // Arabidopsis thaliana (Thale cress)
        4932 => true,   // Saccharomyces cerevisiae (Baker's yeast)
        559292 => true, // Saccharomyces cerevisiae S288C
        511145 => true, // Escherichia coli str. K-12 substr. MG1655
        83333 => true,  // Escherichia coli K-12
        224308 => true, // Bacillus subtilis subsp. subtilis str. 168
        _ => {
            // Check name patterns
            let lower = name.to_lowercase();
            lower.contains("model organism")
                || (lower.contains("laboratory")
                    && (lower.contains("strain") || lower.contains("isolate")))
        }
    }
}

/// Check if a taxon is a pathogen
fn is_pathogen(taxon_id: TaxonId, name: &str) -> bool {
    // Common pathogen taxon IDs
    match taxon_id.0 {
        1773 => true,    // Mycobacterium tuberculosis
        1280 => true,    // Staphylococcus aureus
        573 => true,     // Klebsiella pneumoniae
        287 => true,     // Pseudomonas aeruginosa
        470 => true,     // Acinetobacter baumannii
        666 => true,     // Vibrio cholerae
        662 => true,     // Vibrio parahaemolyticus
        90371 => true,   // Salmonella enterica
        28901 => true,   // Salmonella enterica serovar Typhimurium
        550 => true,     // Enterococcus faecalis
        1352 => true,    // Enterococcus faecium
        5207 => true,    // Cryptococcus neoformans
        5476 => true,    // Candida albicans
        36329 => true,   // Plasmodium falciparum
        5833 => true,    // Plasmodium vivax
        5811 => true,    // Toxoplasma gondii
        10245 => true,   // Variola virus (Smallpox)
        11320 => true,   // Influenza A virus
        11520 => true,   // Influenza B virus
        2697049 => true, // SARS-CoV-2
        694009 => true,  // SARS-CoV
        1335626 => true, // MERS-CoV
        11103 => true,   // Hepatitis C virus
        10407 => true,   // Hepatitis B virus
        11676 => true,   // HIV-1
        11709 => true,   // HIV-2
        10376 => true,   // Epstein-Barr virus
        10298 => true,   // Human herpesvirus 1 (HSV-1)
        10310 => true,   // Human herpesvirus 2 (HSV-2)
        _ => {
            // Check name patterns for pathogenic indicators
            let lower = name.to_lowercase();
            lower.contains("pathogen")
                || lower.contains("virulent")
                || lower.contains("disease")
                || lower.contains("infectious")
                || lower.contains("outbreak")
                || lower.contains("epidemic")
                || lower.contains("clinical isolate")
                || lower.contains("hospital")
                || (lower.contains("resistant") && !lower.contains("antibiotic-sensitive"))
        }
    }
}

/// Check if a taxon is environmental/ecological
fn is_environmental(taxon_id: TaxonId, name: &str) -> bool {
    // Environmental/ecological organism taxon IDs
    match taxon_id.0 {
        1883 => true,   // Streptomyces (soil bacteria)
        1423 => true,   // Bacillus subtilis (soil)
        316 => true,    // Pseudomonas stutzeri (soil)
        303 => true,    // Pseudomonas putida (soil/water)
        1735 => true,   // Chlorobium (green sulfur bacteria)
        1148 => true,   // Synechocystis (cyanobacteria)
        1140 => true,   // Synechococcus (marine cyanobacteria)
        146891 => true, // Prochlorococcus marinus
        312 => true,    // Azotobacter vinelandii (nitrogen-fixing)
        192 => true,    // Azospirillum brasilense (nitrogen-fixing)
        29413 => true,  // Cenarchaeum (marine archaea)
        2287 => true,   // Sulfolobus (thermophilic archaea)
        2234 => true,   // Methanobrevibacter (methanogen)
        _ => {
            // Check name patterns for environmental indicators
            let lower = name.to_lowercase();
            lower.contains("environmental")
                || lower.contains("uncultured")
                || lower.contains("marine")
                || lower.contains("ocean")
                || lower.contains("soil")
                || lower.contains("sediment")
                || lower.contains("freshwater")
                || lower.contains("lake")
                || lower.contains("river")
                || lower.contains("compost")
                || lower.contains("rhizosphere")
                || lower.contains("symbiont")
                || lower.contains("endophyte")
                || lower.contains("thermophil")
                || lower.contains("psychrophil")
                || lower.contains("halophil")
                || lower.contains("extremophil")
                || lower.contains("metagenom")
                || lower.contains("microbiome")
        }
    }
}

/// Calculate estimated centrality score
fn calculate_estimated_centrality(chunk: &ManifestMetadata, avg_sequences: f64) -> f64 {
    let alpha = 0.5; // Degree weight
    let beta = 0.3; // Betweenness weight
    let gamma = 0.2; // Coverage weight

    // Normalize sequence count as proxy for degree
    let degree_score = (chunk.sequence_count as f64 / avg_sequences).min(1.0);

    // Use taxon diversity as proxy for betweenness
    let betweenness_score = (chunk.taxon_ids.len() as f64 / 100.0).min(1.0);

    // Use size as proxy for coverage
    let coverage_score = (chunk.size as f64 / (100.0 * 1024.0 * 1024.0)).min(1.0);

    alpha * degree_score + beta * betweenness_score + gamma * coverage_score
}

/// Load full taxonomy database from NCBI dump files
fn load_taxonomy_db() -> Result<TaxonomyDB> {
    // Use the standard taxonomy location
    let taxonomy_dir = talaria_core::system::paths::talaria_databases_dir()
        .join("taxonomy")
        .join("current")
        .join("tree");

    let names_file = taxonomy_dir.join("names.dmp");
    let nodes_file = taxonomy_dir.join("nodes.dmp");

    if !names_file.exists() || !nodes_file.exists() {
        anyhow::bail!(
            "Taxonomy database not found. Run 'talaria database download ncbi/taxonomy' first."
        );
    }

    // Load the full taxonomy database
    ncbi::build_taxonomy_db(&names_file, &nodes_file)
        .map_err(|e| anyhow::anyhow!("Failed to load taxonomy database: {}", e))
}

/// Get taxonomy name for a taxon ID
fn get_taxonomy_name(taxon_id: TaxonId, taxonomy_db: &Option<TaxonomyDB>) -> String {
    if let Some(db) = taxonomy_db {
        if let Some(taxon_info) = db.get_taxon(taxon_id.0) {
            return taxon_info.scientific_name.clone();
        }
    }

    // Return the taxon ID itself when no taxonomy information is available
    format!("TaxID {}", taxon_id.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use talaria_sequoia::TaxonId;

    #[test]
    fn test_is_model_organism() {
        // Test known model organisms by taxon ID
        assert!(is_model_organism(TaxonId(9606), "Homo sapiens"));
        assert!(is_model_organism(TaxonId(10090), "Mus musculus"));
        assert!(is_model_organism(TaxonId(7227), "Drosophila melanogaster"));
        assert!(is_model_organism(TaxonId(6239), "Caenorhabditis elegans"));
        assert!(is_model_organism(TaxonId(4932), "Saccharomyces cerevisiae"));

        // Test by name patterns
        assert!(is_model_organism(TaxonId(0), "Laboratory strain ABC"));
        assert!(is_model_organism(TaxonId(0), "Model organism X"));

        // Test non-model organisms
        assert!(!is_model_organism(TaxonId(999999), "Unknown species"));
        assert!(!is_model_organism(TaxonId(0), "Wild type bacteria"));
    }

    #[test]
    fn test_is_pathogen() {
        // Test known pathogens by taxon ID
        assert!(is_pathogen(TaxonId(1773), "Mycobacterium tuberculosis"));
        assert!(is_pathogen(TaxonId(1280), "Staphylococcus aureus"));
        assert!(is_pathogen(TaxonId(2697049), "SARS-CoV-2"));
        assert!(is_pathogen(TaxonId(11676), "HIV-1"));

        // Test by name patterns
        assert!(is_pathogen(TaxonId(0), "Pathogenic strain"));
        assert!(is_pathogen(TaxonId(0), "Virulent isolate"));
        assert!(is_pathogen(TaxonId(0), "Clinical isolate from hospital"));
        assert!(is_pathogen(TaxonId(0), "Disease-causing bacteria"));

        // Test non-pathogens
        assert!(!is_pathogen(TaxonId(0), "Probiotic bacteria"));
        assert!(!is_pathogen(TaxonId(0), "Antibiotic-sensitive strain"));
    }

    #[test]
    fn test_is_environmental() {
        // Test known environmental organisms by taxon ID
        assert!(is_environmental(TaxonId(1883), "Streptomyces"));
        assert!(is_environmental(TaxonId(146891), "Prochlorococcus marinus"));
        assert!(is_environmental(TaxonId(2287), "Sulfolobus"));

        // Test by name patterns
        assert!(is_environmental(TaxonId(0), "Uncultured bacterium"));
        assert!(is_environmental(TaxonId(0), "Marine sediment isolate"));
        assert!(is_environmental(TaxonId(0), "Soil metagenome"));
        assert!(is_environmental(TaxonId(0), "Thermophilic archaea"));
        assert!(is_environmental(TaxonId(0), "Freshwater sample"));

        // Test non-environmental
        assert!(!is_environmental(TaxonId(0), "Laboratory strain"));
        assert!(!is_environmental(TaxonId(0), "Clinical isolate"));
    }
}
