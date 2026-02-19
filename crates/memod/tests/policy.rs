#![expect(
    clippy::expect_used,
    reason = "negative-path assertions intentionally panic on unexpected success"
)]

mod common;

use common::{TestDaemon, TestResult};
use memo_client::{MemoClientError, PatchValue, UpdateMountRequest};
use memo_core::{ApiError, MountMode, MountName, MountPath};
use reqwest::StatusCode;

async fn daemon_with_mounts() -> TestResult<TestDaemon> {
    let daemon = TestDaemon::spawn().await?;
    daemon.create_default_mounts().await?;
    Ok(daemon)
}

#[tokio::test]
async fn traversal_path_should_return_bad_request() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/v1/fs/read", daemon.base_url))
        .bearer_auth(&daemon.admin_token)
        .query(&[("path", "VaultKB:/../secret.txt")])
        .send()
        .await?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn symlink_escape_should_be_rejected() -> TestResult {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let daemon = daemon_with_mounts().await?;
        let outside_root = daemon.vault_root.parent().expect("workspace parent");
        let outside = outside_root.join("outside");
        tokio::fs::create_dir_all(&outside).await?;
        tokio::fs::write(outside.join("secret.txt"), b"secret").await?;
        symlink(&outside, daemon.vault_root.join("linked"))?;

        let read = daemon
            .admin_client()?
            .read(&MountPath::parse("VaultKB:/linked/secret.txt")?)
            .await;
        let error = read.expect_err("symlink escape should fail");
        assert!(matches!(
            error,
            MemoClientError::Api(ApiError::OutOfBounds | ApiError::SymlinkDenied)
        ));
    }
    Ok(())
}

#[tokio::test]
async fn hide_glob_should_hide_file_from_ls_and_direct_read() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    tokio::fs::write(daemon.vault_root.join("secret.txt"), b"hidden").await?;
    let client = daemon.admin_client()?;
    let name = MountName::new("VaultKB")?;

    client
        .update_mount(
            &name,
            &UpdateMountRequest {
                hide_globs: Some(vec!["secret.txt".to_owned()]),
                ..UpdateMountRequest::default()
            },
        )
        .await?;

    let listed = client.ls(&MountPath::parse("VaultKB:/")?, None).await?;
    assert!(listed
        .entries
        .iter()
        .all(|entry| entry.name != "secret.txt"));

    let error = client
        .read(&MountPath::parse("VaultKB:/secret.txt")?)
        .await
        .expect_err("hidden file should not be readable");
    assert!(matches!(error, MemoClientError::Api(ApiError::NotFound(_))));

    Ok(())
}

#[tokio::test]
async fn deny_read_glob_should_return_permission_denied() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    tokio::fs::write(daemon.vault_root.join("deny.md"), b"blocked").await?;
    let client = daemon.admin_client()?;

    client
        .update_mount(
            &MountName::new("VaultKB")?,
            &UpdateMountRequest {
                deny_read_globs: Some(vec!["**".to_owned()]),
                ..UpdateMountRequest::default()
            },
        )
        .await?;

    let error = client
        .read(&MountPath::parse("VaultKB:/deny.md")?)
        .await
        .expect_err("deny read glob should block reads");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::PolicyViolated(_))
    ));

    Ok(())
}

#[tokio::test]
async fn deny_write_glob_should_return_permission_denied() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;

    client
        .update_mount(
            &MountName::new("VaultKB")?,
            &UpdateMountRequest {
                deny_write_globs: Some(vec!["**".to_owned()]),
                ..UpdateMountRequest::default()
            },
        )
        .await?;

    let error = client
        .write_bytes(
            &MountPath::parse("VaultKB:/readonly/no-write.md")?,
            b"x".to_vec(),
        )
        .await
        .expect_err("deny write glob should block writes");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::PolicyViolated(_))
    ));

    Ok(())
}

#[tokio::test]
async fn read_only_mount_should_reject_writes() -> TestResult {
    let daemon = TestDaemon::spawn().await?;
    let read_only_root = daemon
        .vault_root
        .parent()
        .expect("workspace parent")
        .join("ro");
    tokio::fs::create_dir_all(&read_only_root).await?;
    daemon
        .create_mount(
            MountName::new("ReadOnly")?,
            read_only_root,
            MountMode::ReadOnly,
            vec![],
            vec![],
            vec![],
        )
        .await?;

    let error = daemon
        .admin_client()?
        .write_bytes(&MountPath::parse("ReadOnly:/blocked.md")?, b"x".to_vec())
        .await
        .expect_err("write on ro mount should fail");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::PermissionDenied)
    ));

    Ok(())
}

#[tokio::test]
async fn update_mount_should_allow_clearing_description() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let updated = client
        .update_mount(
            &MountName::new("VaultKB")?,
            &UpdateMountRequest {
                description: Some(PatchValue::Null),
                ..UpdateMountRequest::default()
            },
        )
        .await?;

    assert!(updated.description.is_none());
    Ok(())
}
