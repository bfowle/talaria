//! Temporal versioning and bi-temporal database functionality

pub mod bi_temporal;
pub mod core;
pub mod renderable;
pub mod retroactive;

// Re-export main types
pub use core::{
    HeaderChangeType, ManifestRef, SequenceMetadataHistory, SequenceVersion, TaxonomyVersion,
    TemporalCrossReference, TemporalIndex, TemporalQuery, TemporalState, TemporalStats, Timeline,
    TimelineEvent, TimelineEventType, TimestampedHeaderChange, VersionRef, VersionType,
};
// Re-export VersionInfo from core module
pub use bi_temporal::{BiTemporalDatabase, DatabaseSnapshot, TaxonomicChangeType, TemporalDiff};
pub use retroactive::RetroactiveAnalyzer;
pub use talaria_core::types::TemporalVersionInfo as VersionInfo;
