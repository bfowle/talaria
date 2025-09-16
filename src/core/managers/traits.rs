/// Trait definitions for various manager components
///
/// Provides abstractions for managing databases, tools, taxonomy,
/// and other core components of the system.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use crate::casg::types::TaxonId;
use crate::download::DatabaseSource;
use crate::core::database_manager::DownloadResult;

/// Base manager trait with common lifecycle operations
pub trait Manager: Send + Sync {
    /// Initialize the manager
    fn initialize(&mut self) -> Result<()>;

    /// Verify manager state and dependencies
    fn verify(&self) -> Result<()>;

    /// Clean up resources
    fn cleanup(&mut self) -> Result<()>;

    /// Get current status as a string
    fn status(&self) -> Result<String>;

    /// Get manager name
    fn name(&self) -> &str;

    /// Check if manager is initialized
    fn is_initialized(&self) -> bool;
}

/// Database management operations
pub trait DatabaseManager: Manager {
    /// Download a database
    fn download(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult>;

    /// Assemble database from chunks
    fn assemble_database(
        &self,
        source: &DatabaseSource,
        output: &Path,
    ) -> Result<()>;

    /// Get taxonomy mapping for a database
    fn get_taxonomy_mapping(
        &self,
        source: &DatabaseSource,
    ) -> Result<HashMap<String, TaxonId>>;

    /// List available databases
    fn list_databases(&self) -> Result<Vec<DatabaseInfo>>;

    /// Get database info
    fn get_database_info(&self, source: &DatabaseSource) -> Result<Option<DatabaseInfo>>;

    /// Update database
    fn update_database(
        &mut self,
        source: &DatabaseSource,
        force: bool,
    ) -> Result<UpdateResult>;

    /// Delete database
    fn delete_database(&mut self, source: &DatabaseSource) -> Result<()>;

    /// Get storage path for database
    fn get_database_path(&self, source: &DatabaseSource) -> PathBuf;
}

/// Tool management operations
pub trait ToolManager: Manager {
    /// Install a tool
    fn install_tool(
        &mut self,
        tool_name: &str,
        version: Option<&str>,
    ) -> Result<()>;

    /// Uninstall a tool
    fn uninstall_tool(&mut self, tool_name: &str) -> Result<()>;

    /// List installed tools
    fn list_tools(&self) -> Result<Vec<ToolInfo>>;

    /// Get tool info
    fn get_tool_info(&self, tool_name: &str) -> Result<Option<ToolInfo>>;

    /// Update a tool
    fn update_tool(
        &mut self,
        tool_name: &str,
        version: Option<&str>,
    ) -> Result<()>;

    /// Get tool binary path
    fn get_tool_path(&self, tool_name: &str) -> Result<Option<PathBuf>>;

    /// Verify tool installation
    fn verify_tool(&self, tool_name: &str) -> Result<bool>;
}

/// Taxonomy management operations
pub trait TaxonomyManager: Manager {
    /// Load taxonomy database
    fn load_taxonomy(&mut self, path: &Path) -> Result<()>;

    /// Get taxon by ID
    fn get_taxon(&self, id: TaxonId) -> Option<&TaxonomyNode>;

    /// Get taxon by name
    fn get_taxon_by_name(&self, name: &str) -> Option<&TaxonomyNode>;

    /// Get lineage for a taxon
    fn get_lineage(&self, id: TaxonId) -> Vec<TaxonId>;

    /// Get children of a taxon
    fn get_children(&self, id: TaxonId) -> Vec<TaxonId>;

    /// Search taxa by pattern
    fn search_taxa(&self, pattern: &str) -> Vec<&TaxonomyNode>;

    /// Update taxonomy database
    fn update_taxonomy(&mut self) -> Result<()>;

    /// Get taxonomy statistics
    fn get_stats(&self) -> TaxonomyStats;
}

/// Configuration management
pub trait ConfigManager: Manager {
    /// Load configuration from file
    fn load_config(&mut self, path: &Path) -> Result<()>;

    /// Save configuration to file
    fn save_config(&self, path: &Path) -> Result<()>;

    /// Get configuration value
    fn get(&self, key: &str) -> Option<String>;

    /// Set configuration value
    fn set(&mut self, key: &str, value: String) -> Result<()>;

    /// Remove configuration value
    fn remove(&mut self, key: &str) -> Result<()>;

    /// List all configuration keys
    fn list_keys(&self) -> Vec<String>;

    /// Reset to defaults
    fn reset_to_defaults(&mut self) -> Result<()>;

    /// Validate configuration
    fn validate(&self) -> Result<Vec<ConfigError>>;
}

/// Cache management
pub trait CacheManager: Manager {
    /// Clear all cache
    fn clear_all(&mut self) -> Result<CacheStats>;

    /// Clear cache for specific item
    fn clear_item(&mut self, key: &str) -> Result<bool>;

    /// Get cache statistics
    fn get_cache_stats(&self) -> Result<CacheStats>;

    /// Set cache size limit
    fn set_size_limit(&mut self, bytes: usize) -> Result<()>;

    /// Prune old entries
    fn prune_old(&mut self, max_age_days: u32) -> Result<CacheStats>;

    /// Export cache contents
    fn export(&self, path: &Path) -> Result<()>;

    /// Import cache contents
    fn import(&mut self, path: &Path) -> Result<()>;
}

// Supporting types

#[derive(Debug, Clone)]
pub struct DatabaseInfo {
    pub name: String,
    pub source: DatabaseSource,
    pub version: String,
    pub size: usize,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub chunk_count: usize,
    pub sequence_count: usize,
}

#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub updated: bool,
    pub old_version: String,
    pub new_version: String,
    pub chunks_added: usize,
    pub chunks_removed: usize,
    pub bytes_downloaded: usize,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub installed_date: chrono::DateTime<chrono::Utc>,
    pub is_valid: bool,
}

#[derive(Debug, Clone)]
pub struct TaxonomyNode {
    pub id: TaxonId,
    pub name: String,
    pub rank: String,
    pub parent_id: Option<TaxonId>,
    pub children: Vec<TaxonId>,
}

#[derive(Debug, Clone)]
pub struct TaxonomyStats {
    pub total_nodes: usize,
    pub species_count: usize,
    pub genus_count: usize,
    pub family_count: usize,
    pub max_depth: usize,
}

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub key: String,
    pub message: String,
    pub severity: ConfigErrorSeverity,
}

#[derive(Debug, Clone)]
pub enum ConfigErrorSeverity {
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_size: usize,
    pub item_count: usize,
    pub oldest_item: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_item: Option<chrono::DateTime<chrono::Utc>>,
    pub freed_bytes: Option<usize>,
}