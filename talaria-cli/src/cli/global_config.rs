/// Global configuration trait for accessing CLI settings throughout the application
///
/// This trait provides a consistent way to access global configuration set by
/// the CLI parser, avoiding the need to pass configuration through command chains.
#[allow(dead_code)]
pub trait GlobalConfig {
    /// Get the current verbose level (0 = quiet, 1+ = increasing verbosity)
    fn verbose_level() -> u8 {
        std::env::var("TALARIA_VERBOSE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    /// Check if verbose mode is enabled (level > 0)
    fn is_verbose() -> bool {
        Self::verbose_level() > 0
    }

    /// Check if extra verbose mode is enabled (level > 1)
    fn is_extra_verbose() -> bool {
        Self::verbose_level() > 1
    }

    /// Get the configured number of threads
    fn thread_count() -> usize {
        std::env::var("TALARIA_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(num_cpus::get)
    }

    /// Check if running in debug/trace mode
    fn is_debug() -> bool {
        matches!(
            std::env::var("TALARIA_LOG").as_deref(),
            Ok("debug") | Ok("trace")
        )
    }
}

/// Standard implementation of GlobalConfig
pub struct StandardGlobalConfig;

impl GlobalConfig for StandardGlobalConfig {}

/// Convenience function to get verbose level without importing the trait
#[allow(dead_code)]
pub fn verbose_level() -> u8 {
    StandardGlobalConfig::verbose_level()
}

/// Convenience function to check if verbose mode is enabled
pub fn is_verbose() -> bool {
    StandardGlobalConfig::is_verbose()
}

/// Convenience function to check if extra verbose mode is enabled
#[allow(dead_code)]
pub fn is_extra_verbose() -> bool {
    StandardGlobalConfig::is_extra_verbose()
}

/// Convenience function to get thread count
#[allow(dead_code)]
pub fn thread_count() -> usize {
    StandardGlobalConfig::thread_count()
}

/// Convenience function to check if in debug mode
#[allow(dead_code)]
pub fn is_debug() -> bool {
    StandardGlobalConfig::is_debug()
}
