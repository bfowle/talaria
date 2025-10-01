//! Temporal versioning and bi-temporal database functionality

pub mod bi_temporal;
pub mod core;
pub mod renderable;
pub mod retroactive;
pub mod version_store;

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
// TemporalRenderable is exported from traits module instead
pub use version_store::{
    FilesystemVersionStore, ListOptions, Version, VersionOperation, VersionStore,
};
