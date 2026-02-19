#![expect(
    clippy::expect_used,
    reason = "negative-path assertions intentionally panic on unexpected success"
)]

mod common;

use common::{TestDaemon, TestResult};
use memo_client::MemoClientError;
use memo_core::{ApiError, MountPath};

async fn daemon_with_mounts() -> TestResult<TestDaemon> {
    let daemon = TestDaemon::spawn().await?;
    daemon.create_default_mounts().await?;
    Ok(daemon)
}

#[tokio::test]
async fn concurrent_writes_should_leave_file_in_valid_state() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let path = MountPath::parse("VaultKB:/concurrent/data.bin")?;

    let payload_a = vec![b'A'; 256 * 1024];
    let payload_b = vec![b'B'; 256 * 1024];

    let mut handles = Vec::new();
    for _ in 0..8 {
        let client_a = client.clone();
        let path_a = path.clone();
        let bytes_a = payload_a.clone();
        handles.push(tokio::spawn(async move {
            client_a.write_bytes(&path_a, bytes_a).await
        }));

        let client_b = client.clone();
        let path_b = path.clone();
        let bytes_b = payload_b.clone();
        handles.push(tokio::spawn(async move {
            client_b.write_bytes(&path_b, bytes_b).await
        }));
    }

    for handle in handles {
        handle.await??;
    }

    let final_bytes = client.read(&path).await?;
    assert!(final_bytes == payload_a || final_bytes == payload_b);

    let concurrent_dir = daemon.vault_root.join("concurrent");
    let mut entries = tokio::fs::read_dir(concurrent_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.starts_with(".memo_tmp_"),
            "dangling temp file: {name}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn failed_write_should_cleanup_temp_file() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let target_dir = daemon.vault_root.join("notes").join("dir-target");
    tokio::fs::create_dir_all(&target_dir).await?;

    let client = daemon.admin_client()?;
    let write = client
        .write_bytes(
            &MountPath::parse("VaultKB:/notes/dir-target")?,
            b"data".to_vec(),
        )
        .await;
    let error = write.expect_err("writing into existing directory should fail");
    assert!(matches!(error, MemoClientError::Api(ApiError::Internal(_))));

    let mut entries = tokio::fs::read_dir(daemon.vault_root.join("notes")).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.starts_with(".memo_tmp_"),
            "dangling temp file: {name}"
        );
    }

    Ok(())
}
