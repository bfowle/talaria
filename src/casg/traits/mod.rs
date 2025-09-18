/// Traits for CASG operations
///
/// This module contains traits that define capabilities for various
/// CASG operations including temporal queries, rendering, and analysis.

pub mod temporal;
pub mod renderable;

pub use temporal::{
    TemporalQueryable,
    RetroactiveAnalyzable,
    TemporalSnapshot,
    TemporalDiff,
    EvolutionHistory,
    TemporalJoinQuery,
    TemporalJoinResult,
    RetroactiveResult,
    ClassificationConflict,
    TaxonomyImpactAnalysis,
};

pub use renderable::{
    TemporalRenderable,
    RenderFormat,
};