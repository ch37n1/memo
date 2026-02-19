mod common;

use memo_client::MemoClientError;
use memo_core::{ApiError, MountPath};

use common::TestHarness;

#[tokio::test]
async fn fs_ops_end_to_end_suite() -> Result<(), Box<dyn std::error::Error>> {
    let harness = match TestHarness::start().await {
        Ok(harness) => harness,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    harness.ensure_default_mounts().await?;
    let client = harness.admin_client()?;

    let notes = MountPath::parse("VaultKB:/notes")?;
    let _ = client.mkdir(&notes).await?;
    let listed_empty = client.ls(&notes, Some(false)).await?;
    assert!(listed_empty.entries.is_empty());

    let file = MountPath::parse("VaultKB:/notes/a.md")?;
    let _ = client.write_bytes(&file, b"hello".to_vec()).await?;
    let listed = client.ls(&notes, Some(false)).await?;
    assert_eq!(listed.entries.len(), 1);

    let read = client.read(&file).await?;
    assert_eq!(read, b"hello");

    let missing = client
        .read(&MountPath::parse("VaultKB:/notes/missing.md")?)
        .await;
    assert!(matches!(
        missing,
        Err(MemoClientError::Api(ApiError::NotFound(_)))
    ));

    let _ = client.write_bytes(&file, b"overwritten".to_vec()).await?;
    let read_overwritten = client.read(&file).await?;
    assert_eq!(read_overwritten, b"overwritten");

    let docs = MountPath::parse("VaultKB:/docs")?;
    let _ = client.mkdir(&docs).await?;

    let moved = MountPath::parse("VaultKB:/docs/moved.md")?;
    let _ = client.mv(&file, &moved).await?;

    let dir_from = MountPath::parse("VaultKB:/docs")?;
    let dir_to = MountPath::parse("VaultKB:/docs-renamed")?;
    let _ = client.mv(&dir_from, &dir_to).await?;

    let removed = MountPath::parse("VaultKB:/docs-renamed/moved.md")?;
    let _ = client.rm(&removed, Some(false)).await?;

    let non_empty = MountPath::parse("VaultKB:/dir")?;
    let nested = MountPath::parse("VaultKB:/dir/nested.txt")?;
    let _ = client.mkdir(&non_empty).await?;
    let _ = client.write_bytes(&nested, b"x".to_vec()).await?;
    let non_recursive = client.rm(&non_empty, Some(false)).await;
    assert!(matches!(
        non_recursive,
        Err(MemoClientError::Api(ApiError::Conflict(_)))
    ));
    let _ = client.rm(&non_empty, Some(true)).await?;

    let src = MountPath::parse("VaultKB:/copy-source.md")?;
    let dst_same = MountPath::parse("VaultKB:/copy-same.md")?;
    let _ = client.write_bytes(&src, b"copy".to_vec()).await?;
    let _ = client.cp(&src, &dst_same).await?;

    let dst_cross = MountPath::parse("Archive:/cross/copied.md")?;
    let _ = client.cp(&dst_same, &dst_cross).await?;

    Ok(())
}
