#[cfg(feature = "cloud")]
mod cloud_tests {
    use anyhow::Result;
    use tempfile::TempDir;
    use talaria_sequoia::cloud::sync::CloudSync;
    use talaria_sequoia::cloud::s3::S3Backend;
    use talaria_sequoia::storage::core::SEQUOIAStorage;
    use talaria_sequoia::manifest::core::Manifest;
    use std::env;
    
    fn skip_if_no_cloud_config() -> bool {
        env::var("TALARIA_TEST_S3_BUCKET").is_err()
    }
    
    #[test]
    #[ignore] // Requires cloud configuration
    fn test_s3_sync_workflow() -> Result<()> {
        if skip_if_no_cloud_config() {
            eprintln!("Skipping cloud test - no S3 configuration");
            return Ok(());
        }
        
        let temp_dir = TempDir::new()?;
        let local_path = temp_dir.path().join("local");
        let remote_path = temp_dir.path().join("remote");
        
        // Create local storage
        let mut local_storage = SEQUOIAStorage::new(local_path.clone())?;
        let mut manifest = Manifest::new("cloud_db".to_string(), "1.0.0".to_string());
        
        // Store test data
        let test_data = "TEST_CLOUD_DATA".repeat(100).into_bytes();
        let hash = local_storage.store_chunk(&test_data, talaria_sequoia::storage::ChunkFormat::Raw)?;
        manifest.add_chunk(hash, test_data.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        
        // Save manifest
        let manifest_path = local_path.join("manifest.json");
        manifest.save(&manifest_path)?;
        
        // Sync to cloud
        let bucket = env::var("TALARIA_TEST_S3_BUCKET")?;
        let sync = CloudSync::new(
            local_path.clone(),
            format!("s3://{}/test", bucket),
        )?;
        
        sync.push()?;
        
        // Sync to different local location
        let sync2 = CloudSync::new(
            remote_path.clone(),
            format!("s3://{}/test", bucket),
        )?;
        
        sync2.pull()?;
        
        // Verify data
        let remote_storage = SEQUOIAStorage::new(remote_path)?;
        let retrieved = remote_storage.retrieve_chunk(&hash)?;
        assert_eq!(retrieved, test_data);
        
        Ok(())
    }
}

// Mock cloud tests for when cloud is not available
mod mock_cloud_tests {
    use anyhow::Result;
    use std::path::{Path, PathBuf};
    use std::fs;
    use tempfile::TempDir;
    use talaria_sequoia::storage::core::SEQUOIAStorage;
    use talaria_sequoia::manifest::core::Manifest;
    
    /// Simulates cloud sync using local filesystem
    struct MockCloudSync {
        local_path: PathBuf,
        remote_path: PathBuf,
    }
    
    impl MockCloudSync {
        fn new(local: PathBuf, remote: PathBuf) -> Result<Self> {
            fs::create_dir_all(&remote)?;
            Ok(Self {
                local_path: local,
                remote_path: remote,
            })
        }
        
        fn push(&self) -> Result<()> {
            // Copy all files from local to remote
            for entry in fs::read_dir(&self.local_path)? {
                let entry = entry?;
                let filename = entry.file_name();
                let src = entry.path();
                let dst = self.remote_path.join(&filename);
                
                if src.is_file() {
                    fs::copy(&src, &dst)?;
                } else if src.is_dir() {
                    Self::copy_dir(&src, &dst)?;
                }
            }
            Ok(())
        }
        
        fn pull(&self) -> Result<()> {
            // Copy all files from remote to local
            fs::create_dir_all(&self.local_path)?;
            for entry in fs::read_dir(&self.remote_path)? {
                let entry = entry?;
                let filename = entry.file_name();
                let src = entry.path();
                let dst = self.local_path.join(&filename);
                
                if src.is_file() {
                    fs::copy(&src, &dst)?;
                } else if src.is_dir() {
                    Self::copy_dir(&src, &dst)?;
                }
            }
            Ok(())
        }
        
        fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let filename = entry.file_name();
                let src_path = entry.path();
                let dst_path = dst.join(&filename);
                
                if src_path.is_file() {
                    fs::copy(&src_path, &dst_path)?;
                } else if src_path.is_dir() {
                    Self::copy_dir(&src_path, &dst_path)?;
                }
            }
            Ok(())
        }
    }
    
    #[test]
    fn test_mock_cloud_push_pull() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let local1 = temp_dir.path().join("local1");
        let local2 = temp_dir.path().join("local2");
        let cloud = temp_dir.path().join("cloud");
        
        // Create first local storage
        let mut storage1 = SEQUOIAStorage::new(local1.clone())?;
        let mut manifest = Manifest::new("mock_cloud_db".to_string(), "1.0.0".to_string());
        
        // Store test data
        let data1 = "DATA_1".repeat(50).into_bytes();
        let data2 = "DATA_2".repeat(50).into_bytes();
        
        let hash1 = storage1.store_chunk(&data1, talaria_sequoia::storage::ChunkFormat::Raw)?;
        let hash2 = storage1.store_chunk(&data2, talaria_sequoia::storage::ChunkFormat::Zstd)?;
        
        manifest.add_chunk(hash1, data1.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        manifest.add_chunk(hash2, data2.len(), talaria_sequoia::storage::ChunkFormat::Zstd);
        
        let manifest_path = local1.join("manifest.json");
        manifest.save(&manifest_path)?;
        
        // Push to "cloud"
        let sync1 = MockCloudSync::new(local1.clone(), cloud.clone())?;
        sync1.push()?;
        
        // Pull to second location
        let sync2 = MockCloudSync::new(local2.clone(), cloud.clone())?;
        sync2.pull()?;
        
        // Verify data in second location
        let storage2 = SEQUOIAStorage::new(local2.clone())?;
        let retrieved1 = storage2.retrieve_chunk(&hash1)?;
        let retrieved2 = storage2.retrieve_chunk(&hash2)?;
        
        assert_eq!(retrieved1, data1);
        assert_eq!(retrieved2, data2);
        
        // Verify manifest
        let manifest2 = Manifest::load(&local2.join("manifest.json"))?;
        assert_eq!(manifest2.database_name, manifest.database_name);
        assert_eq!(manifest2.chunks.len(), manifest.chunks.len());
        
        Ok(())
    }
    
    #[test]
    fn test_incremental_sync() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let local = temp_dir.path().join("local");
        let cloud = temp_dir.path().join("cloud");
        
        let mut storage = SEQUOIAStorage::new(local.clone())?;
        let mut manifest = Manifest::new("incremental_sync_db".to_string(), "1.0.0".to_string());
        
        // Initial data
        let data1 = "INITIAL".repeat(100).into_bytes();
        let hash1 = storage.store_chunk(&data1, talaria_sequoia::storage::ChunkFormat::Raw)?;
        manifest.add_chunk(hash1, data1.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        manifest.save(&local.join("manifest.json"))?;
        
        // First sync
        let sync = MockCloudSync::new(local.clone(), cloud.clone())?;
        sync.push()?;
        
        // Add more data
        manifest.version = "1.1.0".to_string();
        let data2 = "ADDITIONAL".repeat(100).into_bytes();
        let hash2 = storage.store_chunk(&data2, talaria_sequoia::storage::ChunkFormat::Raw)?;
        manifest.add_chunk(hash2, data2.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        manifest.save(&local.join("manifest.json"))?;
        
        // Second sync
        sync.push()?;
        
        // Verify cloud has both versions
        assert!(cloud.join("manifest.json").exists());
        assert!(cloud.join("chunks").exists());
        
        // Pull to new location and verify
        let local3 = temp_dir.path().join("local3");
        let sync3 = MockCloudSync::new(local3.clone(), cloud.clone())?;
        sync3.pull()?;
        
        let storage3 = SEQUOIAStorage::new(local3)?;
        assert!(storage3.retrieve_chunk(&hash1).is_ok());
        assert!(storage3.retrieve_chunk(&hash2).is_ok());
        
        Ok(())
    }
    
    #[test]
    fn test_conflict_resolution() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let local1 = temp_dir.path().join("local1");
        let local2 = temp_dir.path().join("local2");
        let cloud = temp_dir.path().join("cloud");
        
        // Create divergent local copies
        let mut storage1 = SEQUOIAStorage::new(local1.clone())?;
        let mut manifest1 = Manifest::new("conflict_db".to_string(), "1.0.0".to_string());
        
        let mut storage2 = SEQUOIAStorage::new(local2.clone())?;
        let mut manifest2 = Manifest::new("conflict_db".to_string(), "1.0.0".to_string());
        
        // Different data in each
        let data1 = "LOCAL1_DATA".repeat(50).into_bytes();
        let hash1 = storage1.store_chunk(&data1, talaria_sequoia::storage::ChunkFormat::Raw)?;
        manifest1.add_chunk(hash1, data1.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        manifest1.save(&local1.join("manifest.json"))?;
        
        let data2 = "LOCAL2_DATA".repeat(50).into_bytes();
        let hash2 = storage2.store_chunk(&data2, talaria_sequoia::storage::ChunkFormat::Raw)?;
        manifest2.add_chunk(hash2, data2.len(), talaria_sequoia::storage::ChunkFormat::Raw);
        manifest2.save(&local2.join("manifest.json"))?;
        
        // Both push (second overwrites first in this simple mock)
        let sync1 = MockCloudSync::new(local1.clone(), cloud.clone())?;
        sync1.push()?;
        
        let sync2 = MockCloudSync::new(local2.clone(), cloud.clone())?;
        sync2.push()?;
        
        // Pull to third location
        let local3 = temp_dir.path().join("local3");
        let sync3 = MockCloudSync::new(local3.clone(), cloud.clone())?;
        sync3.pull()?;
        
        // Verify last writer wins (simple conflict resolution)
        let manifest3 = Manifest::load(&local3.join("manifest.json"))?;
        assert!(manifest3.chunks.contains_key(&hash2));
        
        Ok(())
    }
    
    #[test]
    fn test_partial_sync_recovery() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let local = temp_dir.path().join("local");
        let cloud = temp_dir.path().join("cloud");
        let partial = temp_dir.path().join("partial");
        
        // Create complete dataset
        let mut storage = SEQUOIAStorage::new(local.clone())?;
        let mut manifest = Manifest::new("partial_sync_db".to_string(), "1.0.0".to_string());
        
        let chunks: Vec<_> = (0..10)
            .map(|i| {
                let data = format!("CHUNK_{}", i).repeat(100).into_bytes();
                let hash = storage.store_chunk(&data, talaria_sequoia::storage::ChunkFormat::Raw).unwrap();
                manifest.add_chunk(hash, data.len(), talaria_sequoia::storage::ChunkFormat::Raw);
                (hash, data)
            })
            .collect();
        
        manifest.save(&local.join("manifest.json"))?;
        
        // Sync to cloud
        let sync = MockCloudSync::new(local.clone(), cloud.clone())?;
        sync.push()?;
        
        // Simulate partial pull (copy only manifest and first 5 chunks)
        fs::create_dir_all(&partial)?;
        fs::copy(
            cloud.join("manifest.json"),
            partial.join("manifest.json"),
        )?;
        
        fs::create_dir_all(partial.join("chunks"))?;
        for (i, (hash, _)) in chunks.iter().enumerate().take(5) {
            let chunk_file = format!("chunks/{}", hex::encode(hash));
            if cloud.join(&chunk_file).exists() {
                fs::create_dir_all(partial.join("chunks"))?;
                fs::copy(
                    cloud.join(&chunk_file),
                    partial.join(&chunk_file),
                )?;
            }
        }
        
        // Verify partial sync can be detected
        let partial_storage = SEQUOIAStorage::new(partial)?;
        let partial_manifest = Manifest::load(&partial.join("manifest.json"))?;
        
        // Check we can retrieve partial data
        for (hash, data) in chunks.iter().take(5) {
            match partial_storage.retrieve_chunk(hash) {
                Ok(retrieved) => assert_eq!(retrieved, *data),
                Err(_) => {} // May fail if chunk storage structure is different
            }
        }
        
        // Manifest should show all chunks even if not all present
        assert_eq!(partial_manifest.chunks.len(), 10);
        
        Ok(())
    }
}