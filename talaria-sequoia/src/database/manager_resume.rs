/// Extension methods for DatabaseManager to support resume
use crate::database::{DatabaseManager, DownloadResult};
use crate::download::DatabaseSource;
use anyhow::Result;

impl DatabaseManager {
    /// Download with explicit resume support
    pub async fn download_with_resume(
        &mut self,
        source: &DatabaseSource,
        resume: bool,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // For now, pass resume flag through existing download method
        // In the future, this will use ResumableDownloader directly

        if resume {
            progress_callback("Resume mode enabled - checking for partial downloads...");

            // Check for resumable operations
            let resumable_ops = self.list_resumable_operations()?;
            if !resumable_ops.is_empty() {
                progress_callback(&format!(
                    "Found {} resumable operation(s)",
                    resumable_ops.len()
                ));

                // Find matching operation for this source
                for (op_id, state) in &resumable_ops {
                    if state.source_info.database == format!("{}", source) {
                        progress_callback(&format!("Resuming operation: {}", op_id));
                        // Continue with the download, state will be used automatically
                        break;
                    }
                }
            }
        }

        // Call regular download which will check for resumable state internally
        self.download(source, progress_callback).await
    }

    /// Force download, clearing any resume state
    pub async fn force_download_clear_resume(
        &mut self,
        source: &DatabaseSource,
        progress_callback: impl Fn(&str) + Send + Sync,
    ) -> Result<DownloadResult> {
        // Clear any existing resume state for this database
        let db_name = format!("{}", source);
        let resumable_ops = self.list_resumable_operations()?;

        for (op_id, state) in resumable_ops {
            if state.source_info.database == db_name {
                progress_callback(&format!("Clearing resume state for {}", op_id));
                // Clear the state by completing the operation
                // Note: We need to expose this through a public method
                self.get_storage().complete_processing()?;
            }
        }

        // Now force download
        self.force_download(source, progress_callback).await
    }
}
