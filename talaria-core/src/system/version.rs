//! Version management for Talaria

use semver::Version;
use crate::TalariaError;

/// Parse and validate a version string
pub fn parse_version(version_str: &str) -> Result<Version, TalariaError> {
    Version::parse(version_str)
        .map_err(|e| TalariaError::Version(format!("Invalid version format: {}", e)))
}

/// Check if two versions are compatible
/// Versions are compatible if they have the same major version
pub fn is_compatible(v1: &Version, v2: &Version) -> bool {
    v1.major == v2.major
}

/// Get the current version of Talaria
pub fn current_version() -> Version {
    Version::parse(crate::VERSION).expect("Invalid version in Cargo.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let version = parse_version("1.2.3").unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_compatibility() {
        let v1 = parse_version("1.2.3").unwrap();
        let v2 = parse_version("1.3.0").unwrap();
        let v3 = parse_version("2.0.0").unwrap();

        assert!(is_compatible(&v1, &v2));
        assert!(!is_compatible(&v1, &v3));
    }
}