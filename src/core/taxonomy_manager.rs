use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use dialoguer::Confirm;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Magic bytes for Talaria taxonomy manifest format: "TTM" + version byte
pub const TAXONOMY_MANIFEST_MAGIC: &[u8] = b"TTM\x01";

/// Manifest format for serialization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaxonomyManifestFormat {
    Json,
    Talaria, // MessagePack-based binary format
}

impl TaxonomyManifestFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &str {
        match self {
            Self::Json => "json",
            Self::Talaria => "tal",
        }
    }

    /// Detect format from file extension
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("tal") => Self::Talaria,
            Some("json") => Self::Json,
            _ => Self::Talaria, // Default to Talaria format
        }
    }
}

/// Component specification defining what should be in a complete taxonomy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSpec {
    pub name: String,
    pub required: bool,
    pub source: String,
    pub expected_update_frequency_days: u32,
    pub is_primary: bool, // Primary components trigger new versions
}

/// Installed component with metadata and provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledComponent {
    pub source: String,
    pub checksum: String,
    pub size: u64,
    pub downloaded_at: DateTime<Utc>,
    pub source_version: Option<String>, // Version from source (e.g., NCBI date)
    pub carried_from: Option<String>,   // Previous version if carried forward
    pub file_path: PathBuf,
    pub compressed: bool,
    pub format: FileFormat,
}

/// Detected file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileFormat {
    TarGz,
    Gzip,
    PlainText,
    Fasta,
}

/// Audit entry for tracking changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub component: String,
    pub details: String,
}

/// Complete taxonomy manifest with component tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyManifest {
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Expected components for a complete taxonomy
    pub expected_components: Vec<ComponentSpec>,

    /// Actually installed components
    pub installed_components: HashMap<String, InstalledComponent>,

    /// Audit trail of all operations
    pub history: Vec<AuditEntry>,

    /// Policy for version management
    pub policy: TaxonomyVersionPolicy,
}

/// Version management policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomyVersionPolicy {
    /// Time window for appending to same version (hours)
    pub session_window_hours: u32,

    /// Warn if secondary downloads are older than this (hours)
    pub staleness_warning_hours: u32,

    /// Whether to copy forward secondary components
    pub copy_forward_secondary: bool,
}

impl Default for TaxonomyVersionPolicy {
    fn default() -> Self {
        Self {
            session_window_hours: 48,
            staleness_warning_hours: 72,
            copy_forward_secondary: true,
        }
    }
}

/// Decision on whether to create new version or append
#[derive(Debug)]
pub enum VersionDecision {
    CreateNew { copy_forward: bool, reason: String },
    AppendToCurrent,
    UserCancelled,
}

/// Component staleness information
#[derive(Debug)]
pub struct StaleComponent {
    pub name: String,
    pub age_days: i64,
    pub recommendation: String,
}

/// Completeness report for taxonomy
#[derive(Debug, Default)]
pub struct CompletenessReport {
    pub complete: bool,
    pub missing_required: Vec<String>,
    pub missing_optional: Vec<String>,
    pub stale_components: Vec<StaleComponent>,
    pub carried_components: Vec<String>,
}

/// Taxonomy manager for version and component management
pub struct TaxonomyManager {
    base_path: PathBuf,
    policy: TaxonomyVersionPolicy,
}

impl TaxonomyManager {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            policy: TaxonomyVersionPolicy::default(),
        }
    }

    /// Get the default expected components for a complete taxonomy
    pub fn default_components() -> Vec<ComponentSpec> {
        vec![
            ComponentSpec {
                name: "taxdump".to_string(),
                required: true,
                source: "ncbi/taxonomy".to_string(),
                expected_update_frequency_days: 30,
                is_primary: true,
            },
            ComponentSpec {
                name: "prot_accession2taxid".to_string(),
                required: false,
                source: "ncbi/prot-accession2taxid".to_string(),
                expected_update_frequency_days: 30,
                is_primary: false,
            },
            ComponentSpec {
                name: "nucl_accession2taxid".to_string(),
                required: false,
                source: "ncbi/nucl-accession2taxid".to_string(),
                expected_update_frequency_days: 30,
                is_primary: false,
            },
            ComponentSpec {
                name: "idmapping".to_string(),
                required: false,
                source: "uniprot/idmapping".to_string(),
                expected_update_frequency_days: 30,
                is_primary: false,
            },
        ]
    }

    /// Load current taxonomy manifest
    pub fn load_current_manifest(&self) -> Result<Option<TaxonomyManifest>> {
        let current_dir = self.base_path.join("taxonomy").join("current");
        if !current_dir.exists() {
            return Ok(None);
        }

        // Try .tal first, then .json for backwards compatibility
        let tal_path = current_dir.join("manifest.tal");
        let json_path = current_dir.join("manifest.json");

        if tal_path.exists() {
            let manifest = TaxonomyManifest::read_from_file(&tal_path)?;
            Ok(Some(manifest))
        } else if json_path.exists() {
            let manifest = TaxonomyManifest::read_from_file(&json_path)?;
            Ok(Some(manifest))
        } else {
            Ok(None)
        }
    }

    /// Check if we should create a new version
    pub fn should_create_new_version(
        &self,
        component_name: &str,
        interactive: bool,
    ) -> Result<VersionDecision> {
        let current_manifest = self.load_current_manifest()?;

        // Find component spec
        let specs = Self::default_components();
        let component_spec = specs
            .iter()
            .find(|s| s.name == component_name)
            .ok_or_else(|| anyhow!("Unknown component: {}", component_name))?;

        match current_manifest {
            None => Ok(VersionDecision::CreateNew {
                copy_forward: false,
                reason: "No existing taxonomy version".to_string(),
            }),

            Some(manifest) => {
                let age_hours = (Utc::now() - manifest.created_at).num_hours();

                if component_spec.is_primary {
                    // Primary component - check if we should create new version
                    if age_hours < 24 && interactive {
                        println!(
                            "\n⚠ Warning: A taxonomy version was created {} hours ago.",
                            age_hours
                        );
                        println!("  Version: {}", manifest.version);
                        println!(
                            "  Created: {}",
                            manifest.created_at.format("%Y-%m-%d %H:%M")
                        );

                        if !Confirm::new()
                            .with_prompt("Create a new version anyway?")
                            .default(false)
                            .interact()?
                        {
                            return Ok(VersionDecision::UserCancelled);
                        }
                    }

                    // Check if we should carry forward
                    let carry_forward = if self.policy.copy_forward_secondary && interactive {
                        self.prompt_for_carry_forward(&manifest)?
                    } else {
                        self.policy.copy_forward_secondary
                    };

                    Ok(VersionDecision::CreateNew {
                        copy_forward: carry_forward,
                        reason: "Primary component update".to_string(),
                    })
                } else {
                    // Secondary component - check staleness
                    if age_hours > self.policy.staleness_warning_hours as i64 && interactive {
                        println!(
                            "\n⚠ Warning: Current taxonomy version is {} days old.",
                            age_hours / 24
                        );
                        println!("  Version: {}", manifest.version);

                        if !Confirm::new()
                            .with_prompt("Add to existing version? (No = create new)")
                            .default(true)
                            .interact()?
                        {
                            return Ok(VersionDecision::CreateNew {
                                copy_forward: false,
                                reason: "User rejected stale version".to_string(),
                            });
                        }
                    }

                    // Check if within session window
                    if age_hours <= self.policy.session_window_hours as i64 {
                        Ok(VersionDecision::AppendToCurrent)
                    } else {
                        // Outside session window
                        if interactive {
                            println!(
                                "\n⚠ Current version is outside the session window ({} hours).",
                                self.policy.session_window_hours
                            );

                            if Confirm::new()
                                .with_prompt("Add to existing version anyway?")
                                .default(false)
                                .interact()?
                            {
                                Ok(VersionDecision::AppendToCurrent)
                            } else {
                                Ok(VersionDecision::CreateNew {
                                    copy_forward: false,
                                    reason: "Outside session window".to_string(),
                                })
                            }
                        } else {
                            Ok(VersionDecision::CreateNew {
                                copy_forward: false,
                                reason: "Outside session window".to_string(),
                            })
                        }
                    }
                }
            }
        }
    }

    /// Prompt user about carrying forward secondary components
    fn prompt_for_carry_forward(&self, manifest: &TaxonomyManifest) -> Result<bool> {
        let secondary: Vec<_> = manifest
            .installed_components
            .iter()
            .filter(|(name, _)| {
                Self::default_components()
                    .iter()
                    .any(|s| s.name == **name && !s.is_primary)
            })
            .collect();

        if secondary.is_empty() {
            return Ok(false);
        }

        println!("\nThe following secondary components exist in the current version:");
        for (name, component) in &secondary {
            let age_days = (Utc::now() - component.downloaded_at).num_days();
            println!("  • {} ({} days old)", name, age_days);

            if let Some(carried) = &component.carried_from {
                println!("    (previously carried from {})", carried);
            }
        }

        Confirm::new()
            .with_prompt("Carry these forward to the new version?")
            .default(true)
            .interact()
            .map_err(Into::into)
    }

    /// Check completeness of current taxonomy
    pub fn check_completeness(&self) -> Result<CompletenessReport> {
        let manifest = self
            .load_current_manifest()?
            .ok_or_else(|| anyhow!("No current taxonomy version"))?;

        let mut report = CompletenessReport::default();
        report.complete = true;

        for spec in &manifest.expected_components {
            match manifest.installed_components.get(&spec.name) {
                Some(component) => {
                    let age_days = (Utc::now() - component.downloaded_at).num_days();

                    if age_days > spec.expected_update_frequency_days as i64 {
                        report.stale_components.push(StaleComponent {
                            name: spec.name.clone(),
                            age_days,
                            recommendation: format!(
                                "Update recommended (expected every {} days)",
                                spec.expected_update_frequency_days
                            ),
                        });
                        report.complete = false;
                    }

                    if component.carried_from.is_some() {
                        report.carried_components.push(spec.name.clone());
                    }
                }
                None => {
                    if spec.required {
                        report.missing_required.push(spec.name.clone());
                        report.complete = false;
                    } else {
                        report.missing_optional.push(spec.name.clone());
                    }
                }
            }
        }

        Ok(report)
    }

    /// Detect file format from magic bytes
    pub fn detect_file_format(path: &Path) -> Result<FileFormat> {
        use std::io::Read;

        let mut file = fs::File::open(path)?;
        let mut magic = [0u8; 512];
        let bytes_read = file.read(&mut magic)?;

        if bytes_read < 4 {
            return Ok(FileFormat::PlainText);
        }

        // Check for gzip magic bytes (1f 8b)
        if magic[0] == 0x1f && magic[1] == 0x8b {
            // Try to determine if it's tar.gz
            // This is a simplified check - full implementation would decompress and check
            if path.extension().and_then(|s| s.to_str()) == Some("gz") {
                if let Some(stem) = path.file_stem() {
                    if stem.to_string_lossy().ends_with(".tar") {
                        return Ok(FileFormat::TarGz);
                    }
                }
            }
            return Ok(FileFormat::Gzip);
        }

        // Check for FASTA (starts with '>')
        if magic[0] == b'>' {
            return Ok(FileFormat::Fasta);
        }

        // Default to plain text
        Ok(FileFormat::PlainText)
    }
}

impl TaxonomyManifest {
    /// Write manifest to file in specified format
    pub fn write_to_file(&self, path: &Path, format: TaxonomyManifestFormat) -> Result<()> {
        match format {
            TaxonomyManifestFormat::Json => {
                let content = serde_json::to_string_pretty(self)?;
                fs::write(path, content).context("Failed to write JSON manifest")
            }
            TaxonomyManifestFormat::Talaria => {
                let mut data = Vec::with_capacity(TAXONOMY_MANIFEST_MAGIC.len() + 1024 * 1024);
                data.extend_from_slice(TAXONOMY_MANIFEST_MAGIC); // Add magic header

                let content = rmp_serde::to_vec(self)?;
                data.extend_from_slice(&content);

                fs::write(path, data).context("Failed to write Talaria manifest")
            }
        }
    }

    /// Read manifest from file in any supported format
    pub fn read_from_file(path: &Path) -> Result<Self> {
        let format = TaxonomyManifestFormat::from_path(path);

        match format {
            TaxonomyManifestFormat::Json => {
                let content = fs::read_to_string(path).context("Failed to read JSON manifest")?;
                serde_json::from_str(&content).context("Failed to parse JSON manifest")
            }
            TaxonomyManifestFormat::Talaria => {
                let mut content = fs::read(path).context("Failed to read Talaria manifest")?;

                // Check for and skip magic header if present
                if content.starts_with(TAXONOMY_MANIFEST_MAGIC) {
                    content = content[TAXONOMY_MANIFEST_MAGIC.len()..].to_vec();
                }

                rmp_serde::from_slice(&content).context("Failed to parse Talaria manifest")
            }
        }
    }
}
