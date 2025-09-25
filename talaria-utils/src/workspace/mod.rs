//! Workspace management utilities

pub mod temp;
pub mod sequoia;

// Re-export main workspace types
pub use temp::{
    TempWorkspace, WorkspaceConfig, WorkspaceMetadata,
    WorkspaceStatus, WorkspaceStats,
    list_workspaces, find_workspace,
};

pub use sequoia::{
    SequoiaWorkspaceManager, SequoiaStatistics, SequoiaTransaction
};