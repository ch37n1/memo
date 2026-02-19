mod common;

use memo_client::{MemoClient, MemoClientError};
use memo_core::ApiError;
use time::{Duration, OffsetDateTime};

use common::TestHarness;

#[tokio::test]
async fn auth_end_to_end_suite() -> Result<(), Box<dyn std::error::Error>> {
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

    let no_token_client = MemoClient::for_base_url(&harness.base_url)?;
    let no_token = no_token_client.list_mounts().await;
    assert!(matches!(
        no_token,
        Err(MemoClientError::Api(ApiError::AuthRequired))
    ));

    let invalid_client = harness.client_with_token("memo_invalid")?;
    let invalid = invalid_client.list_mounts().await;
    assert!(matches!(
        invalid,
        Err(MemoClientError::Api(ApiError::TokenInvalid))
    ));

    let expired_token = harness
        .create_token(
            "expired",
            &["meta:*:read"],
            Some(OffsetDateTime::now_utc() - Duration::minutes(5)),
        )
        .await?;
    let expired_client = harness.client_with_token(expired_token)?;
    let expired = expired_client.list_mounts().await;
    assert!(matches!(
        expired,
        Err(MemoClientError::Api(ApiError::TokenExpired))
    ));

    let wrong_scope = harness
        .create_token("reader", &["fs:VaultKB:read"], None)
        .await?;
    let wrong_scope_client = harness.client_with_token(wrong_scope)?;
    let wrong_scope_result = wrong_scope_client.list_mounts().await;
    assert!(matches!(
        wrong_scope_result,
        Err(MemoClientError::Api(ApiError::PermissionDenied))
    ));

    let valid_token = harness
        .create_token("meta-reader", &["meta:*:read"], None)
        .await?;
    let valid_client = harness.client_with_token(valid_token)?;
    let mounts = valid_client.list_mounts().await?;
    assert!(!mounts.is_empty());

    Ok(())
}
