mod common;

use memo_client::MemoClientError;
use memo_core::{ApiError, MountPath};

use common::TestHarness;

#[tokio::test]
async fn atomic_write_concurrent_and_cleanup_suite() -> Result<(), Box<dyn std::error::Error>> {
    let harness = TestHarness::start().await?;
    harness.ensure_default_mounts().await?;
    let client = harness.admin_client()?;

    let dir_target = harness.mount_root.join("dir-target");
    std::fs::create_dir_all(&dir_target)?;

    let interrupted = client
        .write_bytes(&MountPath::parse("VaultKB:/dir-target")?, b"x".to_vec())
        .await;
    assert!(matches!(
        interrupted,
        Err(MemoClientError::Api(ApiError::Internal(_)))
    ));

    let temp_leftovers = std::fs::read_dir(&harness.mount_root)?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".memo_tmp_")
        })
        .count();
    assert_eq!(temp_leftovers, 0);

    let path = MountPath::parse("VaultKB:/race/file.txt")?;
    let mut tasks = Vec::new();
    for idx in 0..12 {
        let client = harness.admin_client()?;
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            let body = format!("payload-{idx}").into_bytes();
            client.write_bytes(&path, body.clone()).await.map(|_| body)
        }));
    }

    let mut expected = Vec::new();
    for task in tasks {
        let body = task.await??;
        expected.push(body);
    }

    let final_read = client.read(&path).await?;
    assert!(expected.iter().any(|candidate| candidate == &final_read));

    let race_dir = harness.mount_root.join("race");
    let race_temps = std::fs::read_dir(race_dir)?
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".memo_tmp_")
        })
        .count();
    assert_eq!(race_temps, 0);

    Ok(())
}
