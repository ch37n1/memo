use std::path::Path;

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct AtomicWriteOptions {
    pub fsync: bool,
    pub dir_sync: bool,
}

impl Default for AtomicWriteOptions {
    fn default() -> Self {
        Self {
            fsync: true,
            dir_sync: true,
        }
    }
}

/// Atomically writes bytes to `target` by writing a temp file in the same
/// directory and renaming into place.
///
/// # Errors
///
/// Returns `std::io::Error` when creating parent dirs, writing, syncing,
/// renaming, or cleanup fails.
pub async fn atomic_write_bytes(
    target: &Path,
    bytes: &[u8],
    options: AtomicWriteOptions,
) -> Result<(), std::io::Error> {
    let parent = target.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path has no parent directory",
        )
    })?;

    tokio::fs::create_dir_all(parent).await?;

    let temp_name = format!(".memo_tmp_{}", Uuid::new_v4());
    let temp_path = parent.join(temp_name);

    let write_result = async {
        let mut temp_file = tokio::fs::File::create(&temp_path).await?;
        temp_file.write_all(bytes).await?;
        if options.fsync {
            if let Err(error) = temp_file.sync_all().await {
                if !is_non_fatal_sync_error(&error) {
                    return Err(error);
                }
            }
        }
        temp_file.flush().await?;
        drop(temp_file);
        tokio::fs::rename(&temp_path, target).await?;
        if options.dir_sync {
            match tokio::fs::File::open(parent).await {
                Ok(dir) => {
                    if let Err(error) = dir.sync_all().await {
                        if !is_non_fatal_sync_error(&error) {
                            return Err(error);
                        }
                    }
                }
                Err(error) => {
                    if !is_non_fatal_sync_error(&error) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    if write_result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    write_result
}

fn is_non_fatal_sync_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::Unsupported
            | std::io::ErrorKind::InvalidInput
    )
}

#[cfg(test)]
mod tests {
    use crate::fs::atomic::AtomicWriteOptions;
    use tempfile::tempdir;

    #[tokio::test]
    async fn writes_file_atomically() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let target = tempdir.path().join("notes").join("a.md");

        super::atomic_write_bytes(&target, b"hello", AtomicWriteOptions::default()).await?;

        let content = tokio::fs::read_to_string(target).await?;
        assert_eq!(content, "hello");

        Ok(())
    }

    #[test]
    fn treats_selected_sync_errors_as_non_fatal() {
        let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "perm");
        assert!(super::is_non_fatal_sync_error(&err));

        let err = std::io::Error::new(std::io::ErrorKind::Unsupported, "unsupported");
        assert!(super::is_non_fatal_sync_error(&err));

        let err = std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid");
        assert!(super::is_non_fatal_sync_error(&err));

        let err = std::io::Error::other("other");
        assert!(!super::is_non_fatal_sync_error(&err));
    }
}
