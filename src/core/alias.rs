/// Trait for version alias management
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

use crate::download::DatabaseSource;

/// Trait for resolving version aliases to concrete versions
#[async_trait]
pub trait AliasResolver: Send + Sync {
    /// Resolve an alias to a concrete version ID
    async fn resolve(&self, database: &DatabaseSource, alias: &str) -> Result<String>;

    /// List all aliases for a database
    async fn list_aliases(&self, database: &DatabaseSource) -> Result<HashMap<String, String>>;

    /// Create or update an alias
    async fn set_alias(
        &mut self,
        database: &DatabaseSource,
        alias: &str,
        version: &str,
    ) -> Result<()>;

    /// Remove an alias
    async fn remove_alias(&mut self, database: &DatabaseSource, alias: &str) -> Result<()>;

    /// Validate alias name (no conflicts with version format)
    fn validate_alias_name(&self, alias: &str) -> Result<()> {
        // Aliases shouldn't look like timestamp versions
        if alias.len() == 15 && alias.chars().nth(8) == Some('_') {
            anyhow::bail!("Alias cannot use timestamp version format");
        }

        // Reserved aliases
        let reserved = vec![".", "..", "chunks", "manifests", "versions"];
        if reserved.contains(&alias) {
            anyhow::bail!("Alias '{}' is reserved", alias);
        }

        // Alias should be filesystem-safe
        if alias.contains('/') || alias.contains('\\') {
            anyhow::bail!("Alias cannot contain path separators");
        }

        Ok(())
    }

    /// Check if an alias exists
    async fn alias_exists(&self, database: &DatabaseSource, alias: &str) -> bool {
        self.resolve(database, alias).await.is_ok()
    }

    /// Get all standard aliases (current, stable, latest)
    fn standard_aliases() -> Vec<&'static str> {
        vec!["current", "stable", "latest", "previous"]
    }
}

/// Filesystem-based alias resolver implementation
pub struct FilesystemAliasResolver {
    base_path: std::path::PathBuf,
}

impl FilesystemAliasResolver {
    pub fn new(base_path: std::path::PathBuf) -> Self {
        Self { base_path }
    }

    fn alias_path(&self, database: &DatabaseSource) -> std::path::PathBuf {
        use crate::core::resolver::{DatabaseResolver, StandardDatabaseResolver};

        let resolver = StandardDatabaseResolver::new(self.base_path.clone());
        let db_ref = resolver.from_source(database);

        self.base_path
            .join("databases")
            .join("data")
            .join(&db_ref.source)
            .join(&db_ref.dataset)
            .join("aliases.json")
    }

    fn load_aliases(&self, database: &DatabaseSource) -> Result<HashMap<String, String>> {
        let path = self.alias_path(database);
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let content = std::fs::read_to_string(&path)?;
        let aliases: HashMap<String, String> = serde_json::from_str(&content)?;
        Ok(aliases)
    }

    fn save_aliases(
        &self,
        database: &DatabaseSource,
        aliases: &HashMap<String, String>,
    ) -> Result<()> {
        let path = self.alias_path(database);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(aliases)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

#[async_trait]
impl AliasResolver for FilesystemAliasResolver {
    async fn resolve(&self, database: &DatabaseSource, alias: &str) -> Result<String> {
        let aliases = self.load_aliases(database)?;
        aliases
            .get(alias)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Alias '{}' not found", alias))
    }

    async fn list_aliases(&self, database: &DatabaseSource) -> Result<HashMap<String, String>> {
        self.load_aliases(database)
    }

    async fn set_alias(
        &mut self,
        database: &DatabaseSource,
        alias: &str,
        version: &str,
    ) -> Result<()> {
        self.validate_alias_name(alias)?;

        let mut aliases = self.load_aliases(database)?;
        aliases.insert(alias.to_string(), version.to_string());
        self.save_aliases(database, &aliases)?;

        Ok(())
    }

    async fn remove_alias(&mut self, database: &DatabaseSource, alias: &str) -> Result<()> {
        let mut aliases = self.load_aliases(database)?;
        aliases.remove(alias);
        self.save_aliases(database, &aliases)?;

        Ok(())
    }
}
