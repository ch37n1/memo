mod common;

use memo_client::MemoClientError;
use memo_core::{ApiError, MountPath};

use common::TestHarness;

#[tokio::test]
#[ignore = "security"]
async fn security_traversal_symlink_and_unicode_suite() -> Result<(), Box<dyn std::error::Error>> {
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

    let corpus = [
        "VaultKB:/../secret.md",
        "VaultKB:/....//secret.md",
        "VaultKB:/%2e%2e%2fsecret.md",
    ];
    let raw = reqwest::Client::new();
    for path in corpus {
        let response = raw
            .get(format!("{}/v1/fs/stat?path={}", harness.base_url, path))
            .bearer_auth(&harness.bootstrap_token)
            .send()
            .await?;
        assert!(response.status().is_client_error());
        let body: serde_json::Value = response.json().await?;
        let code = body["error"]["code"].as_str().unwrap_or("");
        assert!(matches!(
            code,
            "invalid_path" | "out_of_bounds" | "symlink_denied"
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let outside = tempfile::tempdir()?;
        let outside_file = outside.path().join("outside.md");
        std::fs::write(&outside_file, b"outside")?;
        let link = harness.mount_root.join("escape.md");
        let _ = std::fs::remove_file(&link);
        symlink(&outside_file, &link)?;

        let escaped = client.read(&MountPath::parse("VaultKB:/escape.md")?).await;
        assert!(matches!(
            escaped,
            Err(MemoClientError::Api(
                ApiError::SymlinkDenied | ApiError::OutOfBounds
            ))
        ));
    }

    let nfc = "caf\u{00E9}.md";
    let nfd = "cafe\u{0301}.md";
    let nfc_path = MountPath::parse(format!("VaultKB:/{nfc}"))?;
    let nfd_path = MountPath::parse(format!("VaultKB:/{nfd}"))?;

    let _ = client.write_bytes(&nfc_path, b"unicode".to_vec()).await?;
    let canonical_result = client.stat(&nfc_path).await;
    let decomposed_result = client.stat(&nfd_path).await;

    assert!(canonical_result.is_ok());
    if let Err(error) = decomposed_result {
        assert!(matches!(error, MemoClientError::Api(ApiError::NotFound(_))));
    }

    Ok(())
}
