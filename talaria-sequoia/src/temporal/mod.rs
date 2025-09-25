//! Temporal versioning and bi-temporal database functionality

pub mod core;
pub mod bi_temporal;
pub mod retroactive;
pub mod renderable;
pub mod version_store;

// Re-export main types
pub use core::{
    VersionInfo, TemporalIndex, SequenceVersion, TaxonomyVersion,
    TemporalCrossReference, SequenceMetadataHistory, TimestampedHeaderChange,
    HeaderChangeType, ManifestRef, VersionRef, VersionType, TemporalStats,
    TemporalState, TemporalQuery, Timeline, TimelineEvent, TimelineEventType
};
pub use bi_temporal::{BiTemporalDatabase, DatabaseSnapshot, TemporalDiff, TaxonomicChangeType};
pub use retroactive::RetroactiveAnalyzer;
// TemporalRenderable is exported from traits module instead
pub use version_store::{Version, ListOptions, VersionOperation, VersionStore, FilesystemVersionStore};