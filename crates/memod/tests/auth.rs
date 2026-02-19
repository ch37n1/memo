#![expect(
    clippy::expect_used,
    reason = "negative-path assertions intentionally panic on unexpected success"
)]

mod common;

use common::{scope_set, TestDaemon, TestResult};
use memo_client::{CreateTokenRequest, MemoClientError};
use memo_core::{ApiError, MountPath};
use time::{Duration, OffsetDateTime};

async fn daemon_with_mounts() -> TestResult<TestDaemon> {
    let daemon = TestDaemon::spawn().await?;
    daemon.create_default_mounts().await?;
    Ok(daemon)
}

#[tokio::test]
async fn missing_token_should_be_rejected() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.client_with_token(None)?;

    let error = client
        .list_tokens()
        .await
        .expect_err("missing token should fail");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::AuthRequired)
    ));

    Ok(())
}

#[tokio::test]
async fn invalid_token_should_be_rejected() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let client = daemon.client_with_token(Some("memo_invalid_token".to_owned()))?;

    let error = client
        .list_tokens()
        .await
        .expect_err("invalid token should fail");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::TokenInvalid)
    ));

    Ok(())
}

#[tokio::test]
async fn expired_token_should_be_rejected() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let admin = daemon.admin_client()?;
    let expires_at = OffsetDateTime::now_utc() - Duration::minutes(1);

    let created = admin
        .create_token(&CreateTokenRequest {
            name: "expired".to_owned(),
            scopes: scope_set(&["meta:*:read"])?,
            expires_at: Some(expires_at),
        })
        .await?;
    let expired_client = daemon.client_with_token(Some(created.token))?;

    let error = expired_client
        .list_mounts()
        .await
        .expect_err("expired token should fail");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::TokenExpired)
    ));

    Ok(())
}

#[tokio::test]
async fn wrong_scope_should_be_rejected() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let admin = daemon.admin_client()?;

    let created = admin
        .create_token(&CreateTokenRequest {
            name: "readonly".to_owned(),
            scopes: scope_set(&["fs:VaultKB:read"])?,
            expires_at: None,
        })
        .await?;
    let read_only_client = daemon.client_with_token(Some(created.token))?;

    let error = read_only_client
        .mkdir(&MountPath::parse("VaultKB:/nope")?)
        .await
        .expect_err("write with read-only scope should fail");
    assert!(matches!(
        error,
        MemoClientError::Api(ApiError::PermissionDenied)
    ));

    Ok(())
}

#[tokio::test]
async fn correct_scope_should_succeed() -> TestResult {
    let daemon = daemon_with_mounts().await?;
    let admin = daemon.admin_client()?;

    let created = admin
        .create_token(&CreateTokenRequest {
            name: "reader".to_owned(),
            scopes: scope_set(&["fs:VaultKB:read"])?,
            expires_at: None,
        })
        .await?;
    let reader = daemon.client_with_token(Some(created.token))?;

    let listed = reader.ls(&MountPath::parse("VaultKB:/")?, None).await?;
    assert!(listed.entries.is_empty());

    Ok(())
}
