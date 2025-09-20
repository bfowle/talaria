pub mod lambda;
/// Tool management for external bioinformatics tools
///
/// This module provides functionality to download, install, and manage
/// external tools like LAMBDA, BLAST, and DIAMOND that are used for
/// alignment-based sequence reduction.
pub mod tool_manager;
pub mod traits;

pub use lambda::LambdaAligner;
pub use tool_manager::{ToolInfo, ToolManager};
pub use traits::{Aligner, AlignmentConfig, AlignmentResult, ConfigurableAligner};

/// Mock aligner for testing and graph-based selection
pub struct MockAligner;

impl MockAligner {
    pub fn new() -> Self {
        MockAligner
    }
}

impl Aligner for MockAligner {
    fn search(
        &mut self,
        _query: &[crate::bio::sequence::Sequence],
        _reference: &[crate::bio::sequence::Sequence],
    ) -> Result<Vec<AlignmentResult>> {
        // Return empty results - actual alignments are provided separately
        Ok(Vec::new())
    }

    fn version(&self) -> Result<String> {
        Ok("MockAligner 1.0.0".to_string())
    }

    fn is_available(&self) -> bool {
        true
    }
}

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Supported tools that can be managed by Talaria
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    Lambda,
    Blast,
    Diamond,
    Mmseqs2,
}

impl Tool {
    /// Get the name of the tool
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Lambda => "lambda",
            Tool::Blast => "blast",
            Tool::Diamond => "diamond",
            Tool::Mmseqs2 => "mmseqs2",
        }
    }

    /// Get the display name of the tool
    pub fn display_name(&self) -> &'static str {
        match self {
            Tool::Lambda => "LAMBDA",
            Tool::Blast => "BLAST+",
            Tool::Diamond => "DIAMOND",
            Tool::Mmseqs2 => "MMseqs2",
        }
    }

    /// Get the GitHub repository for the tool
    pub fn github_repo(&self) -> &'static str {
        match self {
            Tool::Lambda => "seqan/lambda",
            Tool::Blast => "ncbi/blast",
            Tool::Diamond => "bbuchfink/diamond",
            Tool::Mmseqs2 => "soedinglab/MMseqs2",
        }
    }

    /// Get the binary name for the tool
    pub fn binary_name(&self) -> &'static str {
        match self {
            Tool::Lambda => "lambda3",
            Tool::Blast => "blastp",
            Tool::Diamond => "diamond",
            Tool::Mmseqs2 => "mmseqs",
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for Tool {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "lambda" | "lambda3" => Ok(Tool::Lambda),
            "blast" | "blastp" => Ok(Tool::Blast),
            "diamond" => Ok(Tool::Diamond),
            "mmseqs" | "mmseqs2" => Ok(Tool::Mmseqs2),
            _ => anyhow::bail!("Unknown tool: {}", s),
        }
    }
}
