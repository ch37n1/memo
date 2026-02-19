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
async fn ls_should_return_empty_entries_for_new_mount_root() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;

    let listed = client.ls(&MountPath::parse("VaultKB:/")?, None).await?;
    assert!(listed.entries.is_empty());

    Ok(())
}

#[tokio::test]
async fn read_should_return_written_bytes() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let path = MountPath::parse("VaultKB:/notes/read.md")?;

    client.write_bytes(&path, b"hello".to_vec()).await?;
    let bytes = client.read(&path).await?;

    assert_eq!(bytes, b"hello");
    Ok(())
}

#[tokio::test]
async fn write_should_overwrite_existing_file() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let path = MountPath::parse("VaultKB:/notes/overwrite.md")?;

    client.write_bytes(&path, b"before".to_vec()).await?;
    client.write_bytes(&path, b"after".to_vec()).await?;

    assert_eq!(client.read(&path).await?, b"after");
    Ok(())
}

#[tokio::test]
async fn mkdir_and_mv_should_move_directory_contents() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;

    let src_dir = MountPath::parse("VaultKB:/src")?;
    let src_file = MountPath::parse("VaultKB:/src/a.txt")?;
    let dst_dir = MountPath::parse("VaultKB:/dst")?;
    let dst_file = MountPath::parse("VaultKB:/dst/a.txt")?;

    client.mkdir(&src_dir).await?;
    client.write_bytes(&src_file, b"data".to_vec()).await?;
    client.mv(&src_dir, &dst_dir).await?;

    assert_eq!(client.read(&dst_file).await?, b"data");
    Ok(())
}

#[tokio::test]
async fn rm_non_empty_without_recursive_should_return_conflict() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let dir = MountPath::parse("VaultKB:/non-empty")?;
    let file = MountPath::parse("VaultKB:/non-empty/a.txt")?;

    client.mkdir(&dir).await?;
    client.write_bytes(&file, b"x".to_vec()).await?;

    let error = client
        .rm(&dir, Some(false))
        .await
        .expect_err("non-recursive remove should fail");
    assert!(matches!(error, MemoClientError::Api(ApiError::Conflict(_))));

    Ok(())
}

#[tokio::test]
async fn cp_should_copy_between_mounts() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let source = MountPath::parse("VaultKB:/notes/source.md")?;
    let target = MountPath::parse("Archive:/copies/target.md")?;

    client.write_bytes(&source, b"copy me".to_vec()).await?;
    client.cp(&source, &target).await?;

    assert_eq!(client.read(&target).await?, b"copy me");
    Ok(())
}
