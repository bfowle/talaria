/// Result types for operations that implement Reportable
///
/// These types aggregate operation results and provide a clean interface
/// for generating reports across different output formats.

use super::reduction::{ReductionManifest, ReductionParameters, ReductionStatistics};
use super::selection::traits::SelectionStats;
use super::database_diff::DatabaseComparison;
use crate::types::SHA256Hash;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use talaria_bio::taxonomy::TaxonomyDiscrepancy;
use talaria_utils::report::{
    Cell, CellStyle, Metric, MetricSeverity, Report, Reportable, Section, Table,
};

/// Result of a database reduction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionResult {
    pub statistics: ReductionStatistics,
    pub parameters: ReductionParameters,
    pub selection_stats: Option<SelectionStats>,
    pub manifest: ReductionManifest,
    pub duration: Duration,
}

/// Result of a validation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub statistics: ValidationStatistics,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub message: String,
    pub location: Option<String>,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub message: String,
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationStatistics {
    pub total_sequences: usize,
    pub valid_sequences: usize,
    pub invalid_sequences: usize,
    pub total_deltas: usize,
    pub orphaned_deltas: usize,
    pub missing_references: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
}

/// Result of a garbage collection operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageCollectionResult {
    pub chunks_removed: usize,
    pub space_reclaimed: u64,
    pub orphaned_chunks: Vec<SHA256Hash>,
    pub compaction_performed: bool,
    pub duration: Duration,
}

/// Result of a verification operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub valid: bool,
    pub issues: Vec<VerificationIssue>,
    pub merkle_valid: bool,
    pub statistics: VerificationStatistics,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationIssue {
    pub issue_type: IssueType,
    pub message: String,
    pub location: Option<String>,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueType {
    MissingChunk,
    CorruptedChunk,
    InvalidHash,
    MerkleTreeMismatch,
    OrphanedData,
    InconsistentMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatistics {
    pub total_chunks: usize,
    pub verified_chunks: usize,
    pub corrupted_chunks: usize,
    pub missing_chunks: usize,
    pub total_bytes: u64,
    pub verified_bytes: u64,
}

/// Result of a database update operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateResult {
    pub updated_databases: Vec<String>,
    pub failed_databases: Vec<(String, String)>,
    pub dry_run: bool,
    pub comparison: DatabaseComparison,
    pub duration: Duration,
}

/// Result of a mirror/sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorResult {
    pub success: bool,
    pub total_chunks: usize,
    pub transferred_chunks: usize,
    pub skipped_chunks: usize,
    pub failed_chunks: usize,
    pub bytes_transferred: u64,
    pub errors: Vec<String>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub chunk_hash: SHA256Hash,
    pub conflict_type: ConflictType,
    pub resolution: ConflictResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictType {
    HashMismatch,
    TimestampConflict,
    SizeConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    KeepLocal,
    KeepRemote,
    Manual,
}

/// Result of a reconstruction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructionResult {
    pub sequences_reconstructed: usize,
    pub reconstructed_sequences: usize, // Alias for compatibility
    pub total_sequences: usize,
    pub failed_sequences: Vec<String>,
    pub output_size: u64,
    pub output_file: String,
    pub success: bool,
    pub duration: Duration,
}

/// Result of checking for database updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub latest_release_date: Option<String>,
    pub release_notes: Option<String>,
    pub new_sequences: usize,
    pub updated_sequences: usize,
    pub deprecated_sequences: usize,
    pub changes: Vec<String>,
    pub duration: Duration,
}

/// Result of checking for discrepancies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyResult {
    pub discrepancies_found: bool,
    pub sequences_checked: usize,
    pub discrepancies: Vec<TaxonomyDiscrepancy>,
    pub missing_sequences: Vec<String>,
    pub duplicate_sequences: Vec<String>,
    pub inconsistent_metadata: Vec<String>,
    pub total_issues: usize,
    pub duration: Duration,
}

/// Result of database optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub success: bool,
    pub space_before: u64,
    pub space_after: u64,
    pub chunks_compacted: usize,
    pub indices_rebuilt: usize,
    pub compaction_performed: bool,
    pub defragmentation_performed: bool,
    pub duration: Duration,
}

/// Result of database info query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfoResult {
    pub database_name: String,
    pub source: String,
    pub dataset: String,
    pub total_sequences: usize,
    pub total_chunks: usize,
    pub total_size: u64,
    pub versions: usize,
    pub current_version: Option<String>,
    pub last_updated: Option<String>,
    pub taxonomy_coverage: Option<TaxonomyCoverageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyCoverageInfo {
    pub unique_taxa: usize,
    pub coverage: f64,
    pub most_common_taxa: Option<Vec<String>>,
}

/// Result of sequence statistics analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResult {
    pub total_sequences: usize,
    pub total_size: u64,
    pub avg_length: f64,
    pub min_length: usize,
    pub max_length: usize,
    pub gc_content: f64,
    pub composition: CompositionStats,
    pub length_distribution: Vec<(String, usize)>, // (bin_label, count)
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionStats {
    pub a_count: usize,
    pub c_count: usize,
    pub g_count: usize,
    pub t_count: usize,
    pub n_count: usize,
    pub other_count: usize,
}

/// Result of taxonomy coverage analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyCoverageResult {
    pub total_sequences: usize,
    pub unique_taxa: usize,
    pub coverage_by_rank: Vec<(String, usize)>, // (rank, count)
    pub most_common_taxa: Vec<(String, usize)>, // (taxon_name, sequence_count)
    pub rare_taxa: Vec<(String, usize)>,
    pub comparison: Option<TaxonomyComparison>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyComparison {
    pub primary_name: String,
    pub comparison_name: String,
    pub shared_taxa: usize,
    pub unique_to_primary: usize,
    pub unique_to_comparison: usize,
}

/// Result of version history query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryResult {
    pub total_versions: usize,
    pub date_range: (String, String), // (oldest, newest)
    pub versions: Vec<VersionHistoryEntry>,
    pub storage_evolution: Vec<(String, u64)>, // (date, size)
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistoryEntry {
    pub version_id: String,
    pub timestamp: String,
    pub sequences: usize,
    pub chunks: usize,
    pub size: u64,
    pub changes: String,
}

// ============================================================================
// Reportable Implementations
// ============================================================================

impl Reportable for ReductionResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Reduction", "reduce")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("profile", self.manifest.profile.clone());

        // Summary metrics
        let reduction_ratio = (1.0 - (self.statistics.reference_sequences as f64 / self.statistics.original_sequences as f64)) * 100.0;
        let summary_metrics = vec![
            Metric::new("Original Sequences", self.statistics.original_sequences)
                .with_severity(MetricSeverity::Info),
            Metric::new("Reference Sequences", self.statistics.reference_sequences)
                .with_severity(MetricSeverity::Success),
            Metric::new("Child Sequences", self.statistics.child_sequences)
                .with_severity(MetricSeverity::Info),
            Metric::new("Reduction Ratio", format!("{:.1}%", reduction_ratio))
                .with_severity(if reduction_ratio > 80.0 {
                    MetricSeverity::Success
                } else if reduction_ratio > 50.0 {
                    MetricSeverity::Warning
                } else {
                    MetricSeverity::Normal
                }),
            Metric::new("Deduplication Ratio", format!("{:.2}x", self.statistics.deduplication_ratio))
                .with_severity(MetricSeverity::Success),
            Metric::new("Unique Taxa", self.statistics.unique_taxa),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Size statistics table
        let mut size_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
        size_table.add_row(vec![
            Cell::new("Original Size"),
            Cell::new(format_bytes(self.statistics.original_size)),
        ]);
        size_table.add_row(vec![
            Cell::new("Reduced Size"),
            Cell::new(format_bytes(self.statistics.reduced_size)),
        ]);
        size_table.add_row(vec![
            Cell::new("Total with Deltas"),
            Cell::new(format_bytes(self.statistics.total_size_with_deltas)),
        ]);
        let space_saved = self.statistics.original_size.saturating_sub(self.statistics.total_size_with_deltas);
        size_table.add_row(vec![
            Cell::new("Space Saved"),
            Cell::new(format_bytes(space_saved)).with_style(CellStyle::Success),
        ]);
        report = report.section(Section::table("Size Statistics", size_table));

        // Parameters
        let params_items = vec![
            ("Similarity Threshold".to_string(), format!("{:.1}%", self.parameters.similarity_threshold * 100.0)),
            ("Reduction Ratio".to_string(), format!("{:.1}%", self.parameters.reduction_ratio * 100.0)),
            ("Min Length".to_string(), self.parameters.min_length.to_string()),
            ("Taxonomy Aware".to_string(), self.parameters.taxonomy_aware.to_string()),
        ];
        report = report.section(Section::key_value("Parameters", params_items));

        // Selection stats if available
        if let Some(stats) = &self.selection_stats {
            let sel_items = vec![
                ("Total Sequences".to_string(), stats.total_sequences.to_string()),
                ("References Selected".to_string(), stats.references_selected.to_string()),
                ("Average Identity".to_string(), format!("{:.1}%", stats.avg_identity * 100.0)),
                ("Coverage".to_string(), format!("{:.1}%", stats.coverage * 100.0)),
            ];
            report = report.section(Section::key_value("Selection Statistics", sel_items));
        }

        report.build()
    }
}

impl Reportable for ValidationResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Validation", "validate")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("valid", self.valid.to_string());

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Status", if self.valid { "VALID" } else { "INVALID" })
                .with_severity(if self.valid {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
            Metric::new("Total Sequences", self.statistics.total_sequences),
            Metric::new("Valid Sequences", self.statistics.valid_sequences)
                .with_severity(MetricSeverity::Success),
            Metric::new("Invalid Sequences", self.statistics.invalid_sequences)
                .with_severity(if self.statistics.invalid_sequences > 0 {
                    MetricSeverity::Error
                } else {
                    MetricSeverity::Success
                }),
            Metric::new("Errors", self.errors.len())
                .with_severity(if self.errors.is_empty() {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
            Metric::new("Warnings", self.warnings.len())
                .with_severity(if self.warnings.is_empty() {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Warning
                }),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Errors table
        if !self.errors.is_empty() {
            let mut errors_table = Table::new(vec![
                "Severity".to_string(),
                "Message".to_string(),
                "Location".to_string(),
            ]);
            for error in self.errors.iter().take(20) {
                let severity_str = match error.severity {
                    ErrorSeverity::Critical => "CRITICAL",
                    ErrorSeverity::High => "HIGH",
                    ErrorSeverity::Medium => "MEDIUM",
                    ErrorSeverity::Low => "LOW",
                };
                errors_table.add_row(vec![
                    Cell::new(severity_str).with_style(CellStyle::Error),
                    Cell::new(&error.message),
                    Cell::new(error.location.as_deref().unwrap_or("-")),
                ]);
            }
            if self.errors.len() > 20 {
                errors_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.errors.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Errors ({} total)", self.errors.len()),
                errors_table,
            ));
        }

        // Warnings table
        if !self.warnings.is_empty() {
            let mut warnings_table = Table::new(vec![
                "Message".to_string(),
                "Location".to_string(),
            ]);
            for warning in self.warnings.iter().take(20) {
                warnings_table.add_row(vec![
                    Cell::new(&warning.message).with_style(CellStyle::Warning),
                    Cell::new(warning.location.as_deref().unwrap_or("-")),
                ]);
            }
            if self.warnings.len() > 20 {
                warnings_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.warnings.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Warnings ({} total)", self.warnings.len()),
                warnings_table,
            ));
        }

        report.build()
    }
}

impl Reportable for GarbageCollectionResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Garbage Collection", "gc")
            .metadata("duration", format!("{:.2?}", self.duration));

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Chunks Removed", self.chunks_removed)
                .with_severity(if self.chunks_removed > 0 {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Info
                }),
            Metric::new("Space Reclaimed", format_bytes(self.space_reclaimed))
                .with_severity(if self.space_reclaimed > 0 {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Info
                }),
            Metric::new("Compaction", if self.compaction_performed { "Yes" } else { "No" })
                .with_severity(if self.compaction_performed {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Info
                }),
            Metric::new("Orphaned Chunks", self.orphaned_chunks.len()),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Orphaned chunks (if any, show sample)
        if !self.orphaned_chunks.is_empty() {
            let items: Vec<String> = self.orphaned_chunks
                .iter()
                .take(10)
                .map(|h| h.to_hex()[..16].to_string())
                .collect();
            let mut all_items = items;
            if self.orphaned_chunks.len() > 10 {
                all_items.push(format!("... and {} more", self.orphaned_chunks.len() - 10));
            }
            report = report.section(Section::text(
                format!("Orphaned Chunks ({} total)", self.orphaned_chunks.len()),
                all_items.join("\n"),
            ));
        }

        report.build()
    }
}

impl Reportable for VerificationResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Verification", "verify")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("valid", self.valid.to_string());

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Status", if self.valid { "VALID" } else { "INVALID" })
                .with_severity(if self.valid {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
            Metric::new("Total Chunks", self.statistics.total_chunks),
            Metric::new("Verified Chunks", self.statistics.verified_chunks)
                .with_severity(MetricSeverity::Success),
            Metric::new("Corrupted Chunks", self.statistics.corrupted_chunks)
                .with_severity(if self.statistics.corrupted_chunks > 0 {
                    MetricSeverity::Error
                } else {
                    MetricSeverity::Success
                }),
            Metric::new("Missing Chunks", self.statistics.missing_chunks)
                .with_severity(if self.statistics.missing_chunks > 0 {
                    MetricSeverity::Error
                } else {
                    MetricSeverity::Success
                }),
            Metric::new("Merkle Tree", if self.merkle_valid { "Valid" } else { "Invalid" })
                .with_severity(if self.merkle_valid {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Issues table
        if !self.issues.is_empty() {
            let mut issues_table = Table::new(vec![
                "Severity".to_string(),
                "Type".to_string(),
                "Message".to_string(),
                "Location".to_string(),
            ]);
            for issue in self.issues.iter().take(20) {
                let severity_str = match issue.severity {
                    ErrorSeverity::Critical => "CRITICAL",
                    ErrorSeverity::High => "HIGH",
                    ErrorSeverity::Medium => "MEDIUM",
                    ErrorSeverity::Low => "LOW",
                };
                let type_str = match &issue.issue_type {
                    IssueType::MissingChunk => "Missing Chunk",
                    IssueType::CorruptedChunk => "Corrupted Chunk",
                    IssueType::InvalidHash => "Invalid Hash",
                    IssueType::MerkleTreeMismatch => "Merkle Mismatch",
                    IssueType::OrphanedData => "Orphaned Data",
                    IssueType::InconsistentMetadata => "Inconsistent Metadata",
                };
                issues_table.add_row(vec![
                    Cell::new(severity_str).with_style(CellStyle::Error),
                    Cell::new(type_str),
                    Cell::new(&issue.message),
                    Cell::new(issue.location.as_deref().unwrap_or("-")),
                ]);
            }
            if self.issues.len() > 20 {
                issues_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.issues.len() - 20))
                        .with_style(CellStyle::Muted),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table(
                format!("Issues ({} total)", self.issues.len()),
                issues_table,
            ));
        }

        // Statistics table
        let mut stats_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
        stats_table.add_row(vec![
            Cell::new("Total Data"),
            Cell::new(format_bytes(self.statistics.total_bytes)),
        ]);
        stats_table.add_row(vec![
            Cell::new("Verified Data"),
            Cell::new(format_bytes(self.statistics.verified_bytes)),
        ]);
        if self.statistics.total_bytes > 0 {
            let percentage = (self.statistics.verified_bytes as f64 / self.statistics.total_bytes as f64) * 100.0;
            stats_table.add_row(vec![
                Cell::new("Verification Coverage"),
                Cell::new(format!("{:.1}%", percentage)),
            ]);
        }
        report = report.section(Section::table("Statistics", stats_table));

        report.build()
    }
}

impl Reportable for UpdateResult {
    fn to_report(&self) -> Report {
        let success = self.failed_databases.is_empty();
        let mut report = Report::builder("Database Update", "update")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("success", success.to_string())
            .metadata("dry_run", self.dry_run.to_string());

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Status", if success { "SUCCESS" } else { "PARTIAL/FAILED" })
                .with_severity(if success {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Warning
                }),
            Metric::new("Updated Databases", self.updated_databases.len())
                .with_severity(MetricSeverity::Success),
            Metric::new("Failed Databases", self.failed_databases.len())
                .with_severity(if self.failed_databases.is_empty() {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Updated databases
        if !self.updated_databases.is_empty() {
            let mut updated_table = Table::new(vec!["Database".to_string()]);
            for db in &self.updated_databases {
                updated_table.add_row(vec![Cell::new(db).with_style(CellStyle::Success)]);
            }
            report = report.section(Section::table("Updated Databases", updated_table));
        }

        // Failed databases
        if !self.failed_databases.is_empty() {
            let mut failed_table = Table::new(vec!["Database".to_string(), "Error".to_string()]);
            for (db, err) in &self.failed_databases {
                failed_table.add_row(vec![
                    Cell::new(db).with_style(CellStyle::Error),
                    Cell::new(err),
                ]);
            }
            report = report.section(Section::table("Failed Databases", failed_table));
        }

        report.build()
    }
}

impl Reportable for ReconstructionResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Sequence Reconstruction", "reconstruct")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("success", self.success.to_string());

        let success_rate = if self.total_sequences > 0 {
            (self.sequences_reconstructed as f64 / self.total_sequences as f64) * 100.0
        } else {
            0.0
        };

        let summary_metrics = vec![
            Metric::new("Status", if self.success { "SUCCESS" } else { "FAILED" })
                .with_severity(if self.success {
                    MetricSeverity::Success
                } else {
                    MetricSeverity::Error
                }),
            Metric::new("Total Sequences", self.total_sequences),
            Metric::new("Reconstructed", self.sequences_reconstructed)
                .with_severity(MetricSeverity::Success),
            Metric::new("Failed", self.failed_sequences.len())
                .with_severity(if !self.failed_sequences.is_empty() {
                    MetricSeverity::Error
                } else {
                    MetricSeverity::Success
                }),
            Metric::new("Success Rate", format!("{:.1}%", success_rate))
                .with_severity(if success_rate >= 99.0 {
                    MetricSeverity::Success
                } else if success_rate >= 95.0 {
                    MetricSeverity::Warning
                } else {
                    MetricSeverity::Error
                }),
            Metric::new("Output Size", format_bytes(self.output_size)),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Failed sequences (if any)
        if !self.failed_sequences.is_empty() {
            let items: Vec<String> = self.failed_sequences
                .iter()
                .take(20)
                .map(|s| s.clone())
                .collect();
            let mut all_items = items;
            if self.failed_sequences.len() > 20 {
                all_items.push(format!("... and {} more", self.failed_sequences.len() - 20));
            }
            report = report.section(Section::text(
                format!("Failed Sequences ({} total)", self.failed_sequences.len()),
                all_items.join("\n"),
            ));
        }

        report.build()
    }
}

impl Reportable for MirrorResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Mirror", "mirror")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("status", if self.success { "success" } else { "failed" });

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Total Chunks", self.total_chunks)
                .with_severity(MetricSeverity::Info),
            Metric::new("Transferred Chunks", self.transferred_chunks)
                .with_severity(if self.transferred_chunks > 0 { MetricSeverity::Success } else { MetricSeverity::Info }),
            Metric::new("Skipped Chunks", self.skipped_chunks)
                .with_severity(MetricSeverity::Info),
            Metric::new("Failed Chunks", self.failed_chunks)
                .with_severity(if self.failed_chunks > 0 { MetricSeverity::Error } else { MetricSeverity::Success }),
            Metric::new("Data Transferred", format_bytes(self.bytes_transferred))
                .with_severity(MetricSeverity::Success),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Transfer statistics
        let mut stats_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
        stats_table.add_row(vec![
            Cell::new("Total chunks"),
            Cell::new(self.total_chunks.to_string()),
        ]);
        stats_table.add_row(vec![
            Cell::new("Transferred"),
            Cell::new(self.transferred_chunks.to_string()).with_style(CellStyle::Success),
        ]);
        stats_table.add_row(vec![
            Cell::new("Skipped (already synced)"),
            Cell::new(self.skipped_chunks.to_string()),
        ]);
        if self.failed_chunks > 0 {
            stats_table.add_row(vec![
                Cell::new("Failed"),
                Cell::new(self.failed_chunks.to_string()).with_style(CellStyle::Error),
            ]);
        }
        stats_table.add_row(vec![
            Cell::new("Data transferred"),
            Cell::new(format_bytes(self.bytes_transferred)),
        ]);
        stats_table.add_row(vec![
            Cell::new("Duration"),
            Cell::new(format!("{:.2?}", self.duration)),
        ]);
        report = report.section(Section::table("Transfer Statistics", stats_table));

        // Errors if any
        if !self.errors.is_empty() {
            let mut errors_table = Table::new(vec!["Error".to_string()]);
            for error in &self.errors {
                errors_table.add_row(vec![Cell::new(error).with_style(CellStyle::Error)]);
            }
            report = report.section(Section::table("Errors", errors_table));
        }

        report.build()
    }
}

impl Reportable for UpdateCheckResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Update Check", "check-updates")
            .metadata("duration", format!("{:.2?}", self.duration));

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Update Available", if self.update_available { "Yes" } else { "No" })
                .with_severity(if self.update_available { MetricSeverity::Warning } else { MetricSeverity::Success }),
            Metric::new("Current Version", self.current_version.clone())
                .with_severity(MetricSeverity::Info),
            Metric::new("Latest Version", self.latest_version.clone())
                .with_severity(if self.update_available { MetricSeverity::Warning } else { MetricSeverity::Info }),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Version information
        let mut version_table = Table::new(vec!["Property".to_string(), "Current".to_string(), "Latest".to_string()]);
        version_table.add_row(vec![
            Cell::new("Version"),
            Cell::new(&self.current_version),
            Cell::new(&self.latest_version).with_style(if self.update_available { CellStyle::Warning } else { CellStyle::Normal }),
        ]);
        if let Some(ref release_date) = self.latest_release_date {
            version_table.add_row(vec![
                Cell::new("Release Date"),
                Cell::new(""),
                Cell::new(release_date),
            ]);
        }
        if let Some(ref notes) = self.release_notes {
            version_table.add_row(vec![
                Cell::new("Release Notes"),
                Cell::new(""),
                Cell::new(notes),
            ]);
        }
        report = report.section(Section::table("Version Information", version_table));

        // Changes if available
        if !self.changes.is_empty() {
            let mut changes_table = Table::new(vec!["Change".to_string()]);
            for change in &self.changes {
                changes_table.add_row(vec![Cell::new(change)]);
            }
            report = report.section(Section::table("Changes", changes_table));
        }

        report.build()
    }
}

impl Reportable for DiscrepancyResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Taxonomy Discrepancy Check", "check-discrepancies")
            .metadata("duration", format!("{:.2?}", self.duration));

        // Summary metrics
        let discrepancy_count = self.discrepancies.len();
        let summary_metrics = vec![
            Metric::new("Sequences Checked", self.sequences_checked)
                .with_severity(MetricSeverity::Info),
            Metric::new("Discrepancies Found", discrepancy_count)
                .with_severity(if discrepancy_count > 0 { MetricSeverity::Warning } else { MetricSeverity::Success }),
            Metric::new("Accuracy", format!("{:.2}%", (1.0 - (discrepancy_count as f64 / self.sequences_checked as f64)) * 100.0))
                .with_severity(if discrepancy_count == 0 { MetricSeverity::Success } else { MetricSeverity::Warning }),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Discrepancies table
        if !self.discrepancies.is_empty() {
            let mut disc_table = Table::new(vec![
                "Sequence ID".to_string(),
                "Conflicts".to_string(),
                "Resolution".to_string(),
            ]);

            // Show up to 50 discrepancies
            let show_count = std::cmp::min(50, self.discrepancies.len());
            for discrepancy in &self.discrepancies[..show_count] {
                let conflicts_str = discrepancy.conflicts.iter()
                    .map(|(src, taxid)| format!("{:?}:{}", src, taxid))
                    .collect::<Vec<_>>()
                    .join(", ");
                disc_table.add_row(vec![
                    Cell::new(&discrepancy.sequence_id),
                    Cell::new(&conflicts_str).with_style(CellStyle::Warning),
                    Cell::new(&discrepancy.resolution_strategy),
                ]);
            }

            if self.discrepancies.len() > show_count {
                disc_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.discrepancies.len() - show_count)),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }

            report = report.section(Section::table("Discrepancies", disc_table));
        }

        report.build()
    }
}

impl Reportable for OptimizationResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Optimization", "optimize")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("status", if self.success { "success" } else { "failed" });

        // Summary metrics
        let space_saved_pct = if self.space_before > 0 {
            ((self.space_before - self.space_after) as f64 / self.space_before as f64) * 100.0
        } else {
            0.0
        };

        let summary_metrics = vec![
            Metric::new("Space Before", format_bytes(self.space_before))
                .with_severity(MetricSeverity::Info),
            Metric::new("Space After", format_bytes(self.space_after))
                .with_severity(MetricSeverity::Success),
            Metric::new("Space Saved", format_bytes(self.space_before - self.space_after))
                .with_severity(if space_saved_pct > 5.0 { MetricSeverity::Success } else { MetricSeverity::Info }),
            Metric::new("Chunks Compacted", self.chunks_compacted)
                .with_severity(MetricSeverity::Info),
            Metric::new("Indices Rebuilt", self.indices_rebuilt)
                .with_severity(MetricSeverity::Info),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Optimization details
        let mut details_table = Table::new(vec!["Operation".to_string(), "Result".to_string()]);
        details_table.add_row(vec![
            Cell::new("Chunks compacted"),
            Cell::new(self.chunks_compacted.to_string()),
        ]);
        details_table.add_row(vec![
            Cell::new("Indices rebuilt"),
            Cell::new(self.indices_rebuilt.to_string()),
        ]);
        details_table.add_row(vec![
            Cell::new("Storage compaction"),
            Cell::new(if self.compaction_performed { "Yes" } else { "No" }),
        ]);
        details_table.add_row(vec![
            Cell::new("Defragmentation"),
            Cell::new(if self.defragmentation_performed { "Yes" } else { "No" }),
        ]);
        report = report.section(Section::table("Optimization Details", details_table));

        // Space breakdown
        let mut space_table = Table::new(vec!["State".to_string(), "Size".to_string()]);
        space_table.add_row(vec![
            Cell::new("Before optimization"),
            Cell::new(format_bytes(self.space_before)),
        ]);
        space_table.add_row(vec![
            Cell::new("After optimization"),
            Cell::new(format_bytes(self.space_after)).with_style(CellStyle::Success),
        ]);
        space_table.add_row(vec![
            Cell::new("Space saved"),
            Cell::new(format!("{} ({:.1}%)", format_bytes(self.space_before - self.space_after), space_saved_pct))
                .with_style(CellStyle::Success),
        ]);
        report = report.section(Section::table("Space Savings", space_table));

        report.build()
    }
}

impl Reportable for DatabaseInfoResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Database Information", "info")
            .metadata("database", self.database_name.clone());

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Total Sequences", self.total_sequences)
                .with_severity(MetricSeverity::Info),
            Metric::new("Total Size", format_bytes(self.total_size))
                .with_severity(MetricSeverity::Info),
            Metric::new("Total Chunks", self.total_chunks)
                .with_severity(MetricSeverity::Info),
            Metric::new("Versions", self.versions)
                .with_severity(MetricSeverity::Info),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Basic information
        let mut basic_table = Table::new(vec!["Property".to_string(), "Value".to_string()]);
        basic_table.add_row(vec![
            Cell::new("Database Name"),
            Cell::new(&self.database_name),
        ]);
        basic_table.add_row(vec![
            Cell::new("Source"),
            Cell::new(&self.source),
        ]);
        basic_table.add_row(vec![
            Cell::new("Dataset"),
            Cell::new(&self.dataset),
        ]);
        if let Some(ref version) = self.current_version {
            basic_table.add_row(vec![
                Cell::new("Current Version"),
                Cell::new(version),
            ]);
        }
        if let Some(ref date) = self.last_updated {
            basic_table.add_row(vec![
                Cell::new("Last Updated"),
                Cell::new(date),
            ]);
        }
        report = report.section(Section::table("Basic Information", basic_table));

        // Storage statistics
        let mut storage_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
        storage_table.add_row(vec![
            Cell::new("Total sequences"),
            Cell::new(self.total_sequences.to_string()),
        ]);
        storage_table.add_row(vec![
            Cell::new("Total size"),
            Cell::new(format_bytes(self.total_size)),
        ]);
        storage_table.add_row(vec![
            Cell::new("Total chunks"),
            Cell::new(self.total_chunks.to_string()),
        ]);
        if self.total_chunks > 0 {
            let avg_chunk_size = self.total_size / self.total_chunks as u64;
            storage_table.add_row(vec![
                Cell::new("Average chunk size"),
                Cell::new(format_bytes(avg_chunk_size)),
            ]);
        }
        storage_table.add_row(vec![
            Cell::new("Temporal versions"),
            Cell::new(self.versions.to_string()),
        ]);
        report = report.section(Section::table("Storage Statistics", storage_table));

        // Taxonomy coverage if available
        if let Some(ref taxonomy) = self.taxonomy_coverage {
            let mut taxonomy_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
            taxonomy_table.add_row(vec![
                Cell::new("Unique taxa"),
                Cell::new(taxonomy.unique_taxa.to_string()),
            ]);
            taxonomy_table.add_row(vec![
                Cell::new("Coverage"),
                Cell::new(format!("{:.1}%", taxonomy.coverage * 100.0)),
            ]);
            if let Some(ref common) = taxonomy.most_common_taxa {
                taxonomy_table.add_row(vec![
                    Cell::new("Most common taxa"),
                    Cell::new(common.join(", ")),
                ]);
            }
            report = report.section(Section::table("Taxonomy Coverage", taxonomy_table));
        }

        report.build()
    }
}

impl Reportable for StatsResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Sequence Statistics", "stats")
            .metadata("duration", format!("{:.2?}", self.duration));

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Total Sequences", self.total_sequences)
                .with_severity(MetricSeverity::Info),
            Metric::new("Total Size", format_bytes(self.total_size))
                .with_severity(MetricSeverity::Info),
            Metric::new("Average Length", format!("{:.1} bp", self.avg_length))
                .with_severity(MetricSeverity::Info),
            Metric::new("GC Content", format!("{:.2}%", self.gc_content))
                .with_severity(MetricSeverity::Info),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Length statistics
        let mut length_table = Table::new(vec!["Statistic".to_string(), "Value".to_string()]);
        length_table.add_row(vec![
            Cell::new("Minimum length"),
            Cell::new(format!("{} bp", self.min_length)),
        ]);
        length_table.add_row(vec![
            Cell::new("Maximum length"),
            Cell::new(format!("{} bp", self.max_length)),
        ]);
        length_table.add_row(vec![
            Cell::new("Average length"),
            Cell::new(format!("{:.1} bp", self.avg_length)),
        ]);
        length_table.add_row(vec![
            Cell::new("Total length"),
            Cell::new(format!("{} bp", self.total_size)),
        ]);
        report = report.section(Section::table("Length Statistics", length_table));

        // Composition statistics
        let total_bases = self.composition.a_count
            + self.composition.c_count
            + self.composition.g_count
            + self.composition.t_count
            + self.composition.n_count
            + self.composition.other_count;

        let mut comp_table = Table::new(vec!["Base".to_string(), "Count".to_string(), "Percentage".to_string()]);
        comp_table.add_row(vec![
            Cell::new("A (Adenine)"),
            Cell::new(self.composition.a_count.to_string()),
            Cell::new(format!("{:.2}%", (self.composition.a_count as f64 / total_bases as f64) * 100.0)),
        ]);
        comp_table.add_row(vec![
            Cell::new("C (Cytosine)"),
            Cell::new(self.composition.c_count.to_string()),
            Cell::new(format!("{:.2}%", (self.composition.c_count as f64 / total_bases as f64) * 100.0)),
        ]);
        comp_table.add_row(vec![
            Cell::new("G (Guanine)"),
            Cell::new(self.composition.g_count.to_string()),
            Cell::new(format!("{:.2}%", (self.composition.g_count as f64 / total_bases as f64) * 100.0)),
        ]);
        comp_table.add_row(vec![
            Cell::new("T (Thymine)"),
            Cell::new(self.composition.t_count.to_string()),
            Cell::new(format!("{:.2}%", (self.composition.t_count as f64 / total_bases as f64) * 100.0)),
        ]);
        if self.composition.n_count > 0 {
            comp_table.add_row(vec![
                Cell::new("N (Unknown)"),
                Cell::new(self.composition.n_count.to_string()).with_style(CellStyle::Warning),
                Cell::new(format!("{:.2}%", (self.composition.n_count as f64 / total_bases as f64) * 100.0)),
            ]);
        }
        if self.composition.other_count > 0 {
            comp_table.add_row(vec![
                Cell::new("Other"),
                Cell::new(self.composition.other_count.to_string()).with_style(CellStyle::Warning),
                Cell::new(format!("{:.2}%", (self.composition.other_count as f64 / total_bases as f64) * 100.0)),
            ]);
        }
        comp_table.add_row(vec![
            Cell::new("GC Content"),
            Cell::new(format!("{:.2}%", self.gc_content)),
            Cell::new(""),
        ]);
        report = report.section(Section::table("Base Composition", comp_table));

        // Length distribution (show top 10 bins)
        if !self.length_distribution.is_empty() {
            let mut dist_table = Table::new(vec!["Length Range".to_string(), "Count".to_string()]);
            let show_count = std::cmp::min(10, self.length_distribution.len());
            for (bin, count) in &self.length_distribution[..show_count] {
                dist_table.add_row(vec![
                    Cell::new(format!("{} bp", bin)),
                    Cell::new(count.to_string()),
                ]);
            }
            if self.length_distribution.len() > show_count {
                dist_table.add_row(vec![
                    Cell::new(format!("... and {} more bins", self.length_distribution.len() - show_count)),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table("Length Distribution", dist_table));
        }

        report.build()
    }
}

impl Reportable for TaxonomyCoverageResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Taxonomy Coverage", "taxa-coverage")
            .metadata("duration", format!("{:.2?}", self.duration));

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Total Sequences", self.total_sequences)
                .with_severity(MetricSeverity::Info),
            Metric::new("Unique Taxa", self.unique_taxa)
                .with_severity(MetricSeverity::Info),
            Metric::new("Coverage Rate", format!("{:.1}%", (self.unique_taxa as f64 / self.total_sequences as f64) * 100.0))
                .with_severity(MetricSeverity::Success),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Coverage by rank
        if !self.coverage_by_rank.is_empty() {
            let mut rank_table = Table::new(vec!["Taxonomic Rank".to_string(), "Count".to_string()]);
            for (rank, count) in &self.coverage_by_rank {
                rank_table.add_row(vec![
                    Cell::new(rank),
                    Cell::new(count.to_string()),
                ]);
            }
            report = report.section(Section::table("Coverage by Rank", rank_table));
        }

        // Most common taxa
        if !self.most_common_taxa.is_empty() {
            let mut common_table = Table::new(vec!["Taxon".to_string(), "Sequences".to_string(), "Percentage".to_string()]);
            let show_count = std::cmp::min(20, self.most_common_taxa.len());
            for (taxon, count) in &self.most_common_taxa[..show_count] {
                let percentage = (*count as f64 / self.total_sequences as f64) * 100.0;
                common_table.add_row(vec![
                    Cell::new(taxon),
                    Cell::new(count.to_string()),
                    Cell::new(format!("{:.2}%", percentage)),
                ]);
            }
            if self.most_common_taxa.len() > show_count {
                common_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.most_common_taxa.len() - show_count)),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table("Most Common Taxa", common_table));
        }

        // Rare taxa
        if !self.rare_taxa.is_empty() {
            let mut rare_table = Table::new(vec!["Taxon".to_string(), "Sequences".to_string()]);
            let show_count = std::cmp::min(10, self.rare_taxa.len());
            for (taxon, count) in &self.rare_taxa[..show_count] {
                rare_table.add_row(vec![
                    Cell::new(taxon),
                    Cell::new(count.to_string()),
                ]);
            }
            if self.rare_taxa.len() > show_count {
                rare_table.add_row(vec![
                    Cell::new(format!("... and {} more", self.rare_taxa.len() - show_count)),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table("Rare Taxa", rare_table));
        }

        // Comparison if available
        if let Some(ref comparison) = self.comparison {
            let mut comp_table = Table::new(vec!["Metric".to_string(), "Value".to_string()]);
            comp_table.add_row(vec![
                Cell::new("Primary database"),
                Cell::new(&comparison.primary_name),
            ]);
            comp_table.add_row(vec![
                Cell::new("Comparison database"),
                Cell::new(&comparison.comparison_name),
            ]);
            comp_table.add_row(vec![
                Cell::new("Shared taxa"),
                Cell::new(comparison.shared_taxa.to_string()).with_style(CellStyle::Success),
            ]);
            comp_table.add_row(vec![
                Cell::new("Unique to primary"),
                Cell::new(comparison.unique_to_primary.to_string()),
            ]);
            comp_table.add_row(vec![
                Cell::new("Unique to comparison"),
                Cell::new(comparison.unique_to_comparison.to_string()),
            ]);
            report = report.section(Section::table("Database Comparison", comp_table));
        }

        report.build()
    }
}

impl Reportable for HistoryResult {
    fn to_report(&self) -> Report {
        let mut report = Report::builder("Version History", "history")
            .metadata("duration", format!("{:.2?}", self.duration))
            .metadata("date_range", format!("{} to {}", self.date_range.0, self.date_range.1));

        // Summary metrics
        let summary_metrics = vec![
            Metric::new("Total Versions", self.total_versions)
                .with_severity(MetricSeverity::Info),
            Metric::new("Date Range", format!("{} to {}", self.date_range.0, self.date_range.1))
                .with_severity(MetricSeverity::Info),
        ];
        report = report.section(Section::summary("Summary", summary_metrics));

        // Version history table
        if !self.versions.is_empty() {
            let mut version_table = Table::new(vec![
                "Version".to_string(),
                "Date".to_string(),
                "Sequences".to_string(),
                "Chunks".to_string(),
                "Size".to_string(),
            ]);
            let show_count = std::cmp::min(20, self.versions.len());
            for version in &self.versions[..show_count] {
                version_table.add_row(vec![
                    Cell::new(&version.version_id),
                    Cell::new(&version.timestamp),
                    Cell::new(version.sequences.to_string()),
                    Cell::new(version.chunks.to_string()),
                    Cell::new(format_bytes(version.size)),
                ]);
            }
            if self.versions.len() > show_count {
                version_table.add_row(vec![
                    Cell::new(format!("... and {} more versions", self.versions.len() - show_count)),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(""),
                ]);
            }
            report = report.section(Section::table("Version History", version_table));
        }

        // Storage evolution
        if !self.storage_evolution.is_empty() {
            let mut storage_table = Table::new(vec!["Date".to_string(), "Storage Size".to_string()]);
            for (date, size) in &self.storage_evolution {
                storage_table.add_row(vec![
                    Cell::new(date),
                    Cell::new(format_bytes(*size)),
                ]);
            }
            report = report.section(Section::table("Storage Evolution", storage_table));
        }

        report.build()
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
