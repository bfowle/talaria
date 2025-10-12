/// Remote storage module for chunk downloading
/// Supports S3, GCS, Azure Blob Storage, and HTTP(S)
pub mod chunk_client;

pub use chunk_client::{ChunkClient, ChunkDownloadError, Protocol};
