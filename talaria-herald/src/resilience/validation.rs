use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
/// State validation and recovery mechanisms
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// Result of state validation
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// State is valid and consistent
    Valid,
    /// State has recoverable issues
    Recoverable(Vec<ValidationIssue>),
    /// State is corrupted beyond recovery
    Corrupted(Vec<ValidationIssue>),
}

/// Issue found during validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: IssueSeverity,
    pub component: String,
    pub description: String,
    pub recovery_suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Warning,
    Error,
    Critical,
}

/// Strategy for recovering from validation issues
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Continue with warnings logged
    ContinueWithWarnings,
    /// Attempt automatic repair
    AutoRepair,
    /// Reset to clean state
    Reset,
    /// Abort operation
    Abort,
}

/// State validator for different components
pub trait StateValidator {
    /// Validate the component state
    fn validate(&self) -> Result<ValidationResult>;

    /// Attempt to recover from issues
    fn recover(&mut self, issues: &[ValidationIssue], strategy: RecoveryStrategy) -> Result<()>;

    /// Get component name for logging
    fn component_name(&self) -> &str;
}

/// Download state validator
pub struct DownloadStateValidator {
    state_path: PathBuf,
    workspace_path: PathBuf,
}

impl DownloadStateValidator {
    pub fn new(state_path: PathBuf, workspace_path: PathBuf) -> Self {
        Self {
            state_path,
            workspace_path,
        }
    }

    fn check_file_consistency(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check if state file exists
        if !self.state_path.exists() {
            return issues; // No state to validate
        }

        // Load state and check files
        match fs::read(&self.state_path) {
            Ok(data) => {
                match serde_json::from_slice::<serde_json::Value>(&data) {
                    Ok(state) => {
                        // Check referenced files exist
                        if let Some(files) = state.get("files").and_then(|f| f.as_object()) {
                            for (name, info) in files {
                                if let Some(path) = info.get("compressed").and_then(|p| p.as_str())
                                {
                                    let file_path = PathBuf::from(path);
                                    if !file_path.exists() {
                                        issues.push(ValidationIssue {
                                            severity: IssueSeverity::Warning,
                                            component: "download_state".to_string(),
                                            description: format!(
                                                "Referenced file missing: {}",
                                                name
                                            ),
                                            recovery_suggestion: Some(
                                                "Will re-download missing file".to_string(),
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        issues.push(ValidationIssue {
                            severity: IssueSeverity::Critical,
                            component: "download_state".to_string(),
                            description: format!("Invalid state JSON: {}", e),
                            recovery_suggestion: Some("Reset download state".to_string()),
                        });
                    }
                }
            }
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Error,
                    component: "download_state".to_string(),
                    description: format!("Cannot read state file: {}", e),
                    recovery_suggestion: Some("Check file permissions".to_string()),
                });
            }
        }

        issues
    }

    fn check_workspace_integrity(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        if !self.workspace_path.exists() {
            return issues; // No workspace yet
        }

        // Check workspace permissions
        let metadata = match fs::metadata(&self.workspace_path) {
            Ok(m) => m,
            Err(e) => {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Critical,
                    component: "workspace".to_string(),
                    description: format!("Cannot access workspace: {}", e),
                    recovery_suggestion: Some("Check directory permissions".to_string()),
                });
                return issues;
            }
        };

        if metadata.permissions().readonly() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Error,
                component: "workspace".to_string(),
                description: "Workspace is read-only".to_string(),
                recovery_suggestion: Some("Change workspace permissions".to_string()),
            });
        }

        // Check for orphaned temp files
        if let Ok(entries) = fs::read_dir(&self.workspace_path) {
            let temp_files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|s| s.starts_with(".tmp_") || s.ends_with(".partial"))
                        .unwrap_or(false)
                })
                .collect();

            if temp_files.len() > 10 {
                issues.push(ValidationIssue {
                    severity: IssueSeverity::Warning,
                    component: "workspace".to_string(),
                    description: format!("{} orphaned temporary files found", temp_files.len()),
                    recovery_suggestion: Some("Clean up old temporary files".to_string()),
                });
            }
        }

        issues
    }
}

impl StateValidator for DownloadStateValidator {
    fn validate(&self) -> Result<ValidationResult> {
        let mut all_issues = Vec::new();

        // Check file consistency
        all_issues.extend(self.check_file_consistency());

        // Check workspace integrity
        all_issues.extend(self.check_workspace_integrity());

        // Determine overall result
        let has_critical = all_issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Critical);
        let has_error = all_issues
            .iter()
            .any(|i| i.severity == IssueSeverity::Error);

        if all_issues.is_empty() {
            Ok(ValidationResult::Valid)
        } else if has_critical {
            Ok(ValidationResult::Corrupted(all_issues))
        } else if has_error {
            Ok(ValidationResult::Recoverable(all_issues))
        } else {
            // Only warnings
            for issue in &all_issues {
                warn!("{}: {}", issue.component, issue.description);
            }
            Ok(ValidationResult::Valid)
        }
    }

    fn recover(&mut self, issues: &[ValidationIssue], strategy: RecoveryStrategy) -> Result<()> {
        match strategy {
            RecoveryStrategy::ContinueWithWarnings => {
                for issue in issues {
                    warn!("Continuing despite: {}", issue.description);
                }
                Ok(())
            }
            RecoveryStrategy::AutoRepair => {
                info!("Attempting automatic repair for {} issues", issues.len());

                for issue in issues {
                    match issue.severity {
                        IssueSeverity::Warning => {
                            // Clean up temp files if mentioned
                            if issue.description.contains("orphaned temporary") {
                                self.cleanup_temp_files()?;
                            }
                        }
                        IssueSeverity::Error => {
                            // Try to fix permissions
                            if issue.description.contains("read-only") {
                                self.fix_permissions()?;
                            }
                        }
                        IssueSeverity::Critical => {
                            // Cannot auto-repair critical issues
                            error!("Cannot auto-repair critical issue: {}", issue.description);
                            return Err(anyhow::anyhow!("Critical issue cannot be auto-repaired"));
                        }
                    }
                }
                Ok(())
            }
            RecoveryStrategy::Reset => {
                info!("Resetting to clean state");

                // Remove state file
                if self.state_path.exists() {
                    fs::remove_file(&self.state_path).context("Failed to remove state file")?;
                }

                // Clean workspace
                if self.workspace_path.exists() {
                    fs::remove_dir_all(&self.workspace_path)
                        .context("Failed to clean workspace")?;
                    fs::create_dir_all(&self.workspace_path)
                        .context("Failed to recreate workspace")?;
                }

                Ok(())
            }
            RecoveryStrategy::Abort => Err(anyhow::anyhow!(
                "Operation aborted due to validation issues"
            )),
        }
    }

    fn component_name(&self) -> &str {
        "DownloadState"
    }
}

impl DownloadStateValidator {
    fn cleanup_temp_files(&self) -> Result<()> {
        if !self.workspace_path.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.workspace_path)?;
        let mut cleaned = 0;

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_str().unwrap_or("");

            if name_str.starts_with(".tmp_") || name_str.ends_with(".partial") {
                if let Ok(metadata) = entry.metadata() {
                    // Only remove if older than 1 hour
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed.as_secs() > 3600 {
                                fs::remove_file(entry.path())?;
                                cleaned += 1;
                            }
                        }
                    }
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} temporary files", cleaned);
        }

        Ok(())
    }

    fn fix_permissions(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.workspace_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&self.workspace_path, perms)?;
            info!("Fixed workspace permissions");
        }
        Ok(())
    }
}

/// Database state validator
pub struct DatabaseStateValidator {
    database_path: PathBuf,
    manifest_path: PathBuf,
}

impl DatabaseStateValidator {
    pub fn new(database_path: PathBuf) -> Self {
        let manifest_path = database_path.join("manifest.json");
        Self {
            database_path,
            manifest_path,
        }
    }
}

impl StateValidator for DatabaseStateValidator {
    fn validate(&self) -> Result<ValidationResult> {
        let mut issues = Vec::new();

        // Check database directory exists
        if !self.database_path.exists() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Critical,
                component: "database".to_string(),
                description: "Database directory does not exist".to_string(),
                recovery_suggestion: Some("Initialize database first".to_string()),
            });
            return Ok(ValidationResult::Corrupted(issues));
        }

        // Check manifest exists and is valid
        if !self.manifest_path.exists() {
            issues.push(ValidationIssue {
                severity: IssueSeverity::Error,
                component: "database".to_string(),
                description: "Manifest file missing".to_string(),
                recovery_suggestion: Some("Regenerate manifest from chunks".to_string()),
            });
        } else {
            // Try to parse manifest
            match fs::read(&self.manifest_path) {
                Ok(data) => {
                    if let Err(e) = serde_json::from_slice::<serde_json::Value>(&data) {
                        issues.push(ValidationIssue {
                            severity: IssueSeverity::Critical,
                            component: "database".to_string(),
                            description: format!("Invalid manifest JSON: {}", e),
                            recovery_suggestion: Some("Restore from backup or rebuild".to_string()),
                        });
                    }
                }
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: IssueSeverity::Error,
                        component: "database".to_string(),
                        description: format!("Cannot read manifest: {}", e),
                        recovery_suggestion: Some("Check file permissions".to_string()),
                    });
                }
            }
        }

        // Check chunks directory
        let chunks_dir = self.database_path.join("chunks");
        if chunks_dir.exists() {
            match fs::read_dir(&chunks_dir) {
                Ok(entries) => {
                    let chunk_count = entries.count();
                    if chunk_count == 0 {
                        issues.push(ValidationIssue {
                            severity: IssueSeverity::Warning,
                            component: "database".to_string(),
                            description: "No chunks found in database".to_string(),
                            recovery_suggestion: Some("Database may be empty".to_string()),
                        });
                    }
                    debug!("Found {} chunks in database", chunk_count);
                }
                Err(e) => {
                    issues.push(ValidationIssue {
                        severity: IssueSeverity::Error,
                        component: "database".to_string(),
                        description: format!("Cannot read chunks directory: {}", e),
                        recovery_suggestion: Some("Check directory permissions".to_string()),
                    });
                }
            }
        }

        // Determine result
        let has_critical = issues.iter().any(|i| i.severity == IssueSeverity::Critical);

        if issues.is_empty() {
            Ok(ValidationResult::Valid)
        } else if has_critical {
            Ok(ValidationResult::Corrupted(issues))
        } else {
            Ok(ValidationResult::Recoverable(issues))
        }
    }

    fn recover(&mut self, issues: &[ValidationIssue], strategy: RecoveryStrategy) -> Result<()> {
        match strategy {
            RecoveryStrategy::ContinueWithWarnings => {
                for issue in issues {
                    warn!("Database: {}", issue.description);
                }
                Ok(())
            }
            RecoveryStrategy::AutoRepair => {
                for issue in issues {
                    if issue.description.contains("Manifest file missing") {
                        warn!("Cannot auto-generate manifest, needs manual intervention");
                    }
                }
                Ok(())
            }
            RecoveryStrategy::Reset => {
                error!("Database reset requested - this would delete all data!");
                Err(anyhow::anyhow!("Database reset must be done manually"))
            }
            RecoveryStrategy::Abort => Err(anyhow::anyhow!("Database validation failed")),
        }
    }

    fn component_name(&self) -> &str {
        "Database"
    }
}

/// General validation utilities
pub struct ValidationUtils;

impl ValidationUtils {
    /// Validate all components before operation
    pub fn validate_all(validators: Vec<Box<dyn StateValidator>>) -> Result<()> {
        let mut has_errors = false;
        let mut all_issues = Vec::new();

        for validator in validators {
            info!("Validating {}", validator.component_name());

            match validator.validate()? {
                ValidationResult::Valid => {
                    debug!("{} validation passed", validator.component_name());
                }
                ValidationResult::Recoverable(issues) => {
                    warn!(
                        "{} has {} recoverable issues",
                        validator.component_name(),
                        issues.len()
                    );
                    all_issues.extend(issues);
                }
                ValidationResult::Corrupted(issues) => {
                    error!(
                        "{} validation failed with {} critical issues",
                        validator.component_name(),
                        issues.len()
                    );
                    has_errors = true;
                    all_issues.extend(issues);
                }
            }
        }

        if has_errors {
            Err(anyhow::anyhow!(
                "Validation failed with {} issues",
                all_issues.len()
            ))
        } else if !all_issues.is_empty() {
            warn!("Validation completed with {} warnings", all_issues.len());
            Ok(())
        } else {
            info!("All validations passed");
            Ok(())
        }
    }

    /// Create validation report
    pub fn create_report(results: HashMap<String, ValidationResult>) -> String {
        let mut report = String::from("Validation Report\n");
        report.push_str("=================\n\n");

        for (component, result) in results {
            report.push_str(&format!("Component: {}\n", component));

            match result {
                ValidationResult::Valid => {
                    report.push_str("Status: ✓ Valid\n");
                }
                ValidationResult::Recoverable(issues) => {
                    report.push_str(&format!(
                        "Status: ⚠ Recoverable ({} issues)\n",
                        issues.len()
                    ));
                    for issue in issues {
                        report.push_str(&format!(
                            "  - {}: {}\n",
                            issue.severity as i32, issue.description
                        ));
                        if let Some(suggestion) = &issue.recovery_suggestion {
                            report.push_str(&format!("    → {}\n", suggestion));
                        }
                    }
                }
                ValidationResult::Corrupted(issues) => {
                    report.push_str(&format!("Status: ✗ Corrupted ({} issues)\n", issues.len()));
                    for issue in issues {
                        report.push_str(&format!(
                            "  - {}: {}\n",
                            issue.severity as i32, issue.description
                        ));
                        if let Some(suggestion) = &issue.recovery_suggestion {
                            report.push_str(&format!("    → {}\n", suggestion));
                        }
                    }
                }
            }
            report.push_str("\n");
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_download_state_validator() {
        let temp_dir = TempDir::new().unwrap();
        let state_path = temp_dir.path().join("state.json");
        let workspace_path = temp_dir.path().join("workspace");

        fs::create_dir_all(&workspace_path).unwrap();

        let validator = DownloadStateValidator::new(state_path, workspace_path);
        let result = validator.validate().unwrap();

        match result {
            ValidationResult::Valid => {
                // Expected for empty state
            }
            _ => panic!("Expected valid state for new workspace"),
        }
    }

    #[test]
    fn test_database_validator() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("database");

        let validator = DatabaseStateValidator::new(db_path.clone());
        let result = validator.validate().unwrap();

        match result {
            ValidationResult::Corrupted(issues) => {
                assert!(issues
                    .iter()
                    .any(|i| i.description.contains("does not exist")));
            }
            _ => panic!("Expected corrupted state for missing database"),
        }

        // Create database directory and revalidate
        fs::create_dir_all(&db_path).unwrap();
        let validator = DatabaseStateValidator::new(db_path);
        let result = validator.validate().unwrap();

        match result {
            ValidationResult::Recoverable(_) => {
                // Expected for database without manifest
            }
            _ => panic!("Expected recoverable state for empty database"),
        }
    }
}
