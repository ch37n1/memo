mod common;

use memo_client::{CreateMountRequest, MemoClientError};
use memo_core::{ApiError, Audience, MountMode, MountName, MountPath};

use common::TestHarness;

#[tokio::test]
async fn policy_end_to_end_suite() -> Result<(), Box<dyn std::error::Error>> {
    let harness = TestHarness::start().await?;
    let client = harness.admin_client()?;

    let _ = client
        .create_mount(&CreateMountRequest {
            name: MountName::new("VaultKB")?,
            root_path: harness.mount_root.clone(),
            mode: MountMode::ReadWrite,
            audience: Audience::Shared,
            description: None,
            hide_globs: vec!["hidden/*".to_owned()],
            deny_read_globs: vec!["private/*".to_owned()],
            deny_write_globs: vec!["blocked/*".to_owned()],
            max_read_bytes: None,
            max_write_bytes: None,
        })
        .await?;

    let _ = client
        .create_mount(&CreateMountRequest {
            name: MountName::new("ReadOnly")?,
            root_path: harness.archive_root.clone(),
            mode: MountMode::ReadOnly,
            audience: Audience::Shared,
            description: None,
            hide_globs: vec![],
            deny_read_globs: vec![],
            deny_write_globs: vec![],
            max_read_bytes: None,
            max_write_bytes: None,
        })
        .await?;

    std::fs::create_dir_all(harness.mount_root.join("hidden"))?;
    std::fs::create_dir_all(harness.mount_root.join("private"))?;
    std::fs::create_dir_all(harness.mount_root.join("blocked"))?;
    std::fs::write(harness.mount_root.join("hidden/a.md"), b"secret")?;
    std::fs::write(harness.mount_root.join("private/a.md"), b"private")?;
    std::fs::write(harness.mount_root.join("blocked/a.md"), b"blocked")?;

    let raw = reqwest::Client::new();
    let traversal = raw
        .get(format!(
            "{}/v1/fs/stat?path=VaultKB:/../outside.md",
            harness.base_url
        ))
        .bearer_auth(&harness.bootstrap_token)
        .send()
        .await?;
    assert_eq!(traversal.status(), reqwest::StatusCode::BAD_REQUEST);

    let absolute = raw
        .get(format!(
            "{}/v1/fs/stat?path=VaultKB://etc/passwd",
            harness.base_url
        ))
        .bearer_auth(&harness.bootstrap_token)
        .send()
        .await?;
    assert_eq!(absolute.status(), reqwest::StatusCode::BAD_REQUEST);

    let _ = client
        .ls(&MountPath::parse("VaultKB:/")?, Some(false))
        .await?;

    let hidden_direct = client
        .read(&MountPath::parse("VaultKB:/hidden/a.md")?)
        .await;
    assert!(matches!(
        hidden_direct,
        Err(MemoClientError::Api(ApiError::NotFound(_)))
    ));

    let denied_read = client
        .read(&MountPath::parse("VaultKB:/private/a.md")?)
        .await;
    assert!(matches!(denied_read, Err(MemoClientError::Api(_))));

    let denied_write = client
        .write_bytes(&MountPath::parse("VaultKB:/blocked/a.md")?, b"x".to_vec())
        .await;
    assert!(matches!(denied_write, Err(MemoClientError::Api(_))));

    let ro_write = client
        .write_bytes(&MountPath::parse("ReadOnly:/a.md")?, b"x".to_vec())
        .await;
    assert!(matches!(ro_write, Ok(_) | Err(MemoClientError::Api(_))));

    let outside = tempfile::tempdir()?;
    let outside_file = outside.path().join("external.md");
    std::fs::write(&outside_file, b"escape")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = harness.mount_root.join("symlink.md");
        let _ = std::fs::remove_file(&link);
        symlink(&outside_file, &link)?;
        let symlink_result = client.read(&MountPath::parse("VaultKB:/symlink.md")?).await;
        assert!(matches!(
            symlink_result,
            Err(MemoClientError::Api(
                ApiError::SymlinkDenied | ApiError::OutOfBounds
            ))
        ));
    }

    Ok(())
}
