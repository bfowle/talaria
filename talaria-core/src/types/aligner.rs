//! Aligner type definitions

use serde::{Deserialize, Serialize};

/// Target aligner types for sequence alignment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TargetAligner {
    Lambda,
    Blast,
    Kraken,
    Diamond,
    MMseqs2,
    Generic,
}