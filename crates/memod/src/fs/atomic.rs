use std::path::Path;

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// Atomically writes bytes to `target` by writing a temp file in the same
/// directory and renaming into place.
///
/// # Errors
///
/// Returns `std::io::Error` when creating parent dirs, writing, syncing,
/// renaming, or cleanup fails.
pub async fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
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
        temp_file.sync_all().await?;
        temp_file.flush().await?;
        drop(temp_file);
        tokio::fs::rename(&temp_path, target).await
    }
    .await;

    if write_result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    write_result
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    #[tokio::test]
    async fn writes_file_atomically() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let target = tempdir.path().join("notes").join("a.md");

        super::atomic_write_bytes(&target, b"hello").await?;

        let content = tokio::fs::read_to_string(target).await?;
        assert_eq!(content, "hello");

        Ok(())
    }
}
