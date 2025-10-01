/// SHA256 hash type used throughout Talaria for content addressing
use serde::{Deserialize, Serialize};
use std::fmt;

/// SHA256 hash type
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct SHA256Hash(#[serde(with = "serde_bytes")] pub [u8; 32]);

impl SHA256Hash {
    /// Compute SHA256 hash from raw data
    pub fn compute(data: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self(hash)
    }

    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Self(hash))
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Create from bytes slice
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, anyhow::Error> {
        if bytes.len() != 32 {
            anyhow::bail!("Invalid hash length: expected 32, got {}", bytes.len());
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(bytes);
        Ok(Self(hash))
    }

    /// Truncate to first N bytes for display
    pub fn truncated(&self, len: usize) -> String {
        let hex = self.to_hex();
        if hex.len() <= len {
            hex
        } else {
            format!("{}...", &hex[..len])
        }
    }
}

impl fmt::Display for SHA256Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.truncated(8))
    }
}

impl AsRef<[u8]> for SHA256Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for SHA256Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_computation() {
        let data = b"hello world";
        let hash = SHA256Hash::compute(data);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64); // 32 bytes * 2 hex chars per byte
    }

    #[test]
    fn test_hash_roundtrip() {
        let data = b"test data";
        let hash1 = SHA256Hash::compute(data);
        let hex = hash1.to_hex();
        let hash2 = SHA256Hash::from_hex(&hex).unwrap();
        assert_eq!(hash1, hash2);
    }
}
