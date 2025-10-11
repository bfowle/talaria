//! Audit logging module for comprehensive function tracing and data flow analysis
//!
//! This module provides detailed audit logging capabilities to trace all function calls,
//! data transformations, and algorithm executions throughout Talaria's operation.

pub mod layer;
pub mod macros;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

/// Global audit logger instance
static AUDIT_LOGGER: Mutex<Option<AuditLogger>> = Mutex::new(None);

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditEventType {
    /// Function entry
    Entry,
    /// Function exit
    Exit,
    /// Data transformation
    Data,
    /// Algorithm execution
    Algorithm,
    /// Error occurred
    Error,
    /// Performance metric
    Metric,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Log level
    pub level: String,
    /// Module path
    pub module: String,
    /// Function name
    pub function: String,
    /// Source file
    pub file: String,
    /// Line number
    pub line: u32,
    /// Event type
    pub event: AuditEventType,
    /// Event data (JSON value)
    pub data: serde_json::Value,
    /// Thread ID
    pub thread_id: String,
    /// Duration (for exit events)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Audit logger configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Path to audit log file
    pub log_path: PathBuf,
    /// Maximum log file size before rotation (bytes)
    pub max_size: u64,
    /// Number of rotated logs to keep
    pub max_files: usize,
    /// Include data values in logs
    pub include_data: bool,
    /// Include performance metrics
    pub include_metrics: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        let talaria_home = crate::system::paths::talaria_home();
        let logs_dir = talaria_home.join("logs");

        Self {
            log_path: logs_dir.join(format!(
                "audit-{}.log",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            )),
            max_size: 100 * 1024 * 1024, // 100MB
            max_files: 10,
            include_data: true,
            include_metrics: true,
        }
    }
}

/// Audit logger
pub struct AuditLogger {
    config: AuditConfig,
    file: Mutex<std::fs::File>,
    current_size: Mutex<u64>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(config: AuditConfig) -> std::io::Result<Self> {
        // Ensure logs directory exists
        if let Some(parent) = config.log_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&config.log_path)?;

        let current_size = file.metadata()?.len();

        Ok(Self {
            config,
            file: Mutex::new(file),
            current_size: Mutex::new(current_size),
        })
    }

    /// Initialize the global audit logger
    pub fn init(config: AuditConfig) -> std::io::Result<()> {
        let logger = Self::new(config)?;
        let mut global = AUDIT_LOGGER.lock().unwrap();
        *global = Some(logger);
        Ok(())
    }

    /// Get the global audit logger
    pub fn global() -> Option<std::sync::MutexGuard<'static, Option<AuditLogger>>> {
        Some(AUDIT_LOGGER.lock().unwrap())
    }

    /// Write an audit entry
    pub fn write_entry(&self, entry: &AuditEntry) -> std::io::Result<()> {
        let json = serde_json::to_string(entry)?;
        let line = format!("{}\n", json);
        let line_bytes = line.as_bytes();

        let mut file = self.file.lock().unwrap();
        let mut size = self.current_size.lock().unwrap();

        // Check if rotation is needed
        if *size + line_bytes.len() as u64 > self.config.max_size {
            self.rotate_logs()?;
            *size = 0;
        }

        file.write_all(line_bytes)?;
        file.flush()?;
        *size += line_bytes.len() as u64;

        Ok(())
    }

    /// Rotate audit logs
    fn rotate_logs(&self) -> std::io::Result<()> {
        let base_path = &self.config.log_path;
        let parent = base_path.parent().unwrap();
        let stem = base_path.file_stem().unwrap().to_str().unwrap();

        // Rotate existing logs
        for i in (1..self.config.max_files).rev() {
            let old_path = parent.join(format!("{}.{}", stem, i));
            let new_path = parent.join(format!("{}.{}", stem, i + 1));
            if old_path.exists() {
                fs::rename(old_path, new_path)?;
            }
        }

        // Rename current log to .1
        let rotated_path = parent.join(format!("{}.1", stem));
        fs::rename(base_path, &rotated_path)?;

        // Create new log file
        let mut file = self.file.lock().unwrap();
        *file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(base_path)?;

        // Remove old logs beyond max_files
        let oldest = parent.join(format!("{}.{}", stem, self.config.max_files));
        if oldest.exists() {
            fs::remove_file(oldest)?;
        }

        Ok(())
    }
}

/// Create an audit entry for function entry
pub fn audit_entry(
    module: &str,
    function: &str,
    file: &str,
    line: u32,
    data: serde_json::Value,
) -> AuditEntry {
    AuditEntry {
        timestamp: Utc::now(),
        level: "AUDIT".to_string(),
        module: module.to_string(),
        function: function.to_string(),
        file: file.to_string(),
        line,
        event: AuditEventType::Entry,
        data,
        thread_id: format!("{:?}", std::thread::current().id()),
        duration_ms: None,
    }
}

/// Create an audit entry for function exit
pub fn audit_exit(
    module: &str,
    function: &str,
    file: &str,
    line: u32,
    data: serde_json::Value,
    duration: Option<Duration>,
) -> AuditEntry {
    AuditEntry {
        timestamp: Utc::now(),
        level: "AUDIT".to_string(),
        module: module.to_string(),
        function: function.to_string(),
        file: file.to_string(),
        line,
        event: AuditEventType::Exit,
        data,
        thread_id: format!("{:?}", std::thread::current().id()),
        duration_ms: duration.map(|d| d.as_millis() as u64),
    }
}

/// Create an audit entry for data transformation
pub fn audit_data(
    module: &str,
    function: &str,
    file: &str,
    line: u32,
    description: &str,
    data: serde_json::Value,
) -> AuditEntry {
    let mut data_with_desc = data;
    if let serde_json::Value::Object(ref mut map) = data_with_desc {
        map.insert(
            "description".to_string(),
            serde_json::Value::String(description.to_string()),
        );
    }

    AuditEntry {
        timestamp: Utc::now(),
        level: "AUDIT".to_string(),
        module: module.to_string(),
        function: function.to_string(),
        file: file.to_string(),
        line,
        event: AuditEventType::Data,
        data: data_with_desc,
        thread_id: format!("{:?}", std::thread::current().id()),
        duration_ms: None,
    }
}

/// Create an audit entry for algorithm execution
pub fn audit_algorithm(
    module: &str,
    function: &str,
    file: &str,
    line: u32,
    algorithm: &str,
    data: serde_json::Value,
) -> AuditEntry {
    let mut data_with_algo = data;
    if let serde_json::Value::Object(ref mut map) = data_with_algo {
        map.insert(
            "algorithm".to_string(),
            serde_json::Value::String(algorithm.to_string()),
        );
    }

    AuditEntry {
        timestamp: Utc::now(),
        level: "AUDIT".to_string(),
        module: module.to_string(),
        function: function.to_string(),
        file: file.to_string(),
        line,
        event: AuditEventType::Algorithm,
        data: data_with_algo,
        thread_id: format!("{:?}", std::thread::current().id()),
        duration_ms: None,
    }
}

/// Write an audit entry to the global logger
pub fn write_audit(entry: AuditEntry) {
    if let Some(ref logger) = *AUDIT_LOGGER.lock().unwrap() {
        if let Err(e) = logger.write_entry(&entry) {
            eprintln!("Failed to write audit log: {}", e);
        }
    }
}

/// Check if audit logging is enabled
pub fn is_audit_enabled() -> bool {
    AUDIT_LOGGER.lock().unwrap().is_some()
}
