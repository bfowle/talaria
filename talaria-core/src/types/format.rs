//! Output format types for formatting results

use serde::{Deserialize, Serialize};

/// Output format for command results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum OutputFormat {
    /// Human-readable text output
    Text,
    /// JSON output
    Json,
    /// YAML output
    Yaml,
    /// CSV output
    Csv,
    /// TSV (Tab-separated values) output
    Tsv,
    /// FASTA format (for sequences)
    Fasta,
    /// Summary format
    Summary,
    /// Detailed format with all information
    Detailed,
    /// Only output hashes
    HashOnly,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Text
    }
}

impl OutputFormat {
    /// Check if format is machine-readable
    pub fn is_machine_readable(&self) -> bool {
        matches!(self, Self::Json | Self::Yaml | Self::Csv | Self::Tsv)
    }

    /// Check if format is human-readable
    pub fn is_human_readable(&self) -> bool {
        matches!(self, Self::Text | Self::Summary | Self::Detailed)
    }

    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Fasta => "fasta",
            _ => "txt",
        }
    }
}