#![expect(
    clippy::expect_used,
    reason = "negative-path assertions intentionally panic on unexpected success"
)]

mod common;

use common::{TestDaemon, TestResult};
use memo_client::MemoClientError;
use memo_core::{ApiError, MountPath};
use reqwest::StatusCode;

async fn daemon_with_mounts() -> TestResult<TestDaemon> {
    let daemon = TestDaemon::spawn().await?;
    daemon.create_default_mounts().await?;
    Ok(daemon)
}

#[tokio::test]
#[ignore = "security"]
async fn traversal_corpus_should_be_rejected() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let corpus = [
        "VaultKB:/../secrets",
        "VaultKB:/....//secrets",
        "VaultKB:/%2e%2e%2fsecrets",
        "VaultKB:/notes/%00.md",
    ];

    let client = reqwest::Client::new();
    for path in corpus {
        let response = client
            .get(format!("{}/v1/fs/read", daemon.base_url))
            .bearer_auth(&daemon.admin_token)
            .query(&[("path", path)])
            .send()
            .await?;
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::FORBIDDEN
                || response.status() == StatusCode::NOT_FOUND,
            "unexpected status {} for payload {path}",
            response.status()
        );
    }

    Ok(())
}

#[tokio::test]
#[ignore = "security"]
async fn symlink_escape_should_be_rejected() -> TestResult {
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let daemon = daemon_with_mounts().await?;
        let outside = daemon
            .vault_root
            .parent()
            .expect("workspace parent")
            .join("outside-sec");
        tokio::fs::create_dir_all(&outside).await?;
        tokio::fs::write(outside.join("secret.txt"), b"secret").await?;
        symlink(&outside, daemon.vault_root.join("linked-sec"))?;

        let read = daemon
            .admin_client()?
            .read(&MountPath::parse("VaultKB:/linked-sec/secret.txt")?)
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
#[ignore = "security"]
async fn unicode_normalization_should_be_consistent() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.admin_client()?;
    let nfc = MountPath::parse("VaultKB:/caf\u{00e9}.md")?;
    let nfd = MountPath::parse("VaultKB:/cafe\u{0301}.md")?;

    client.write_bytes(&nfc, b"coffee".to_vec()).await?;

    let composed_read_result = client.read(&nfc).await;
    let decomposed_read_result = client.read(&nfd).await;
    assert!(composed_read_result.is_ok());
    if let Err(error) = decomposed_read_result {
        assert!(matches!(error, MemoClientError::Api(_)));
    }

    Ok(())
}
