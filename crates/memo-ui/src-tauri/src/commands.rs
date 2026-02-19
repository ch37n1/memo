#![expect(
    clippy::missing_errors_doc,
    reason = "Tauri command functions are internal IPC handlers"
)]

use std::path::PathBuf;
use std::str::FromStr;

use memo_client::{
    AuditQuery, AuditResponse, CreateMountRequest, CreateTokenRequest, MemoClient, MemoClientConfig,
};
use memo_core::{
    Audience, Mount, MountMode, MountName, MountPath, Scope, ScopeSet, TokenId, TokenView,
};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMountInput {
    pub name: String,
    pub root_path: String,
    pub mode: MountMode,
    pub audience: Audience,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenInput {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilterInput {
    pub mount: Option<String>,
    pub operation: Option<String>,
    pub result: Option<memo_client::AuditResult>,
    pub limit: Option<u64>,
}

fn client(base_url: String, token: Option<String>) -> Result<MemoClient, String> {
    MemoClient::new(MemoClientConfig {
        base_url,
        token,
        ..MemoClientConfig::default()
    })
    .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn health(base_url: String) -> Result<memo_client::HealthResponse, String> {
    client(base_url, None)?
        .health()
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn list_mounts(base_url: String, token: String) -> Result<Vec<Mount>, String> {
    client(base_url, Some(token))?
        .list_mounts()
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn create_mount(
    base_url: String,
    token: String,
    input: CreateMountInput,
) -> Result<Mount, String> {
    let mount_name = MountName::new(&input.name).map_err(|error| error.to_string())?;
    let request = CreateMountRequest {
        name: mount_name,
        root_path: PathBuf::from(input.root_path),
        mode: input.mode,
        audience: input.audience,
        description: input.description,
        hide_globs: Vec::new(),
        deny_read_globs: Vec::new(),
        deny_write_globs: Vec::new(),
        max_read_bytes: None,
        max_write_bytes: None,
    };

    client(base_url, Some(token))?
        .create_mount(&request)
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn remove_mount(base_url: String, token: String, name: String) -> Result<bool, String> {
    let mount_name = MountName::new(&name).map_err(|error| error.to_string())?;
    let response = client(base_url, Some(token))?
        .remove_mount(&mount_name)
        .await
        .map_err(|error| error.to_string())?;
    Ok(response.removed)
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn list_tokens(base_url: String, token: String) -> Result<Vec<TokenView>, String> {
    client(base_url, Some(token))?
        .list_tokens()
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn create_token(
    base_url: String,
    token: String,
    input: CreateTokenInput,
) -> Result<memo_client::CreatedToken, String> {
    let parsed_scopes = input
        .scopes
        .iter()
        .map(|scope| Scope::from_str(scope))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let expires_at = input
        .expires_at
        .as_deref()
        .map(|value| OffsetDateTime::parse(value, &Rfc3339))
        .transpose()
        .map_err(|error| error.to_string())?;

    let request = CreateTokenRequest {
        name: input.name,
        scopes: ScopeSet::new(parsed_scopes),
        expires_at,
    };

    client(base_url, Some(token))?
        .create_token(&request)
        .await
        .map_err(|error| error.to_string())
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn revoke_token(base_url: String, token: String, id: String) -> Result<bool, String> {
    let token_id = uuid::Uuid::parse_str(&id)
        .map(TokenId::from_uuid)
        .map_err(|error| error.to_string())?;

    let response = client(base_url, Some(token))?
        .revoke_token(token_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(response.revoked)
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn query_audit(
    base_url: String,
    token: String,
    filter: AuditFilterInput,
) -> Result<Vec<memo_client::AuditEntry>, String> {
    let mount = filter
        .mount
        .as_deref()
        .map(MountName::new)
        .transpose()
        .map_err(|error| error.to_string())?;

    let response: AuditResponse = client(base_url, Some(token))?
        .query_audit(&AuditQuery {
            mount,
            token_id: None,
            operation: filter.operation,
            result: filter.result,
            limit: filter.limit,
            before: None,
            after: None,
            after_id: None,
        })
        .await
        .map_err(|error| error.to_string())?;

    Ok(response.entries)
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserTreeResponse {
    pub path: String,
    pub depth: u8,
    pub truncated: bool,
    pub tree: memo_client::TreeNode,
}

#[cfg_attr(all(feature = "tauri-app", not(clippy), not(test)), tauri::command)]
pub async fn browse_tree(
    base_url: String,
    token: String,
    mount: String,
    depth: u8,
) -> Result<BrowserTreeResponse, String> {
    let mount_name = MountName::new(&mount).map_err(|error| error.to_string())?;
    let path = MountPath::parse(format!("{mount_name}:/")).map_err(|error| error.to_string())?;
    let response = client(base_url, Some(token))?
        .tree(&path, Some(depth))
        .await
        .map_err(|error| error.to_string())?;

    Ok(BrowserTreeResponse {
        path: response.path.to_string(),
        depth: response.depth,
        truncated: response.truncated,
        tree: response.tree,
    })
}
