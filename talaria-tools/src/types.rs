//! Common types for tool management

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
