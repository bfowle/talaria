/// Manager components module

pub mod traits;

pub use traits::{
    Manager, DatabaseManager, ToolManager, TaxonomyManager,
    ConfigManager, CacheManager, DatabaseInfo, UpdateResult,
    ToolInfo, TaxonomyNode, TaxonomyStats, ConfigError,
    ConfigErrorSeverity, CacheStats,
};