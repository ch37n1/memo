mod error;
mod types;

pub use error::MemoClientError;
pub use types::{
    AuditEntry, AuditQuery, AuditResponse, AuditResult, CopyResponse, CreateMountRequest,
    CreateTokenRequest, CreatedToken, FindResponse, FindResult, FsEntry, FsEntryKind, GrepMatch,
    GrepResponse, HealthResponse, LsResponse, MkdirResponse, MountListResponse, MoveResponse,
    PatchValue, RemoveMountResponse, RemoveResponse, RevokeTokenResponse, StatResponse,
    TokenListResponse, TreeNode, TreeResponse, UpdateMountRequest, WriteResponse,
};

use std::time::Duration;

use memo_core::{ApiError, Mount, MountName, MountPath, TokenId, TokenView};
use reqwest::{Body, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{map_api_error, ErrorResponse};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:18301";
const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct MemoClientConfig {
    pub base_url: String,
    pub token: Option<String>,
    pub timeout: Duration,
}

impl Default for MemoClientConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            token: None,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoClient {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl MemoClient {
    /// Builds a typed client for the memo daemon REST API.
    ///
    /// # Errors
    /// Returns [`MemoClientError::InvalidBaseUrl`] when `base_url` is malformed,
    /// or [`MemoClientError::Request`] when reqwest client construction fails.
    pub fn new(config: MemoClientConfig) -> Result<Self, MemoClientError> {
        let base_url = sanitize_base_url(&config.base_url)?;
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(MemoClientError::Request)?;

        Ok(Self {
            base_url,
            token: config.token,
            client,
        })
    }

    /// Creates a client using defaults and the provided base URL.
    ///
    /// # Errors
    /// Returns an error when URL validation or reqwest client creation fails.
    pub fn for_base_url(base_url: impl Into<String>) -> Result<Self, MemoClientError> {
        Self::new(MemoClientConfig {
            base_url: base_url.into(),
            ..MemoClientConfig::default()
        })
    }

    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Rebuilds the underlying HTTP client with a new timeout.
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] if reqwest client construction fails.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, MemoClientError> {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(MemoClientError::Request)?;
        Ok(self)
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    /// GET /v1/fs/ls
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn ls(
        &self,
        path: &MountPath,
        info: Option<bool>,
    ) -> Result<LsResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
            #[serde(skip_serializing_if = "Option::is_none")]
            info: Option<bool>,
        }

        self.get_json("/v1/fs/ls", Some(&Query { path, info }))
            .await
    }

    /// GET /v1/fs/tree
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn tree(
        &self,
        path: &MountPath,
        depth: Option<u8>,
    ) -> Result<TreeResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
            #[serde(skip_serializing_if = "Option::is_none")]
            depth: Option<u8>,
        }

        self.get_json("/v1/fs/tree", Some(&Query { path, depth }))
            .await
    }

    /// GET /v1/fs/stat
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn stat(&self, path: &MountPath) -> Result<StatResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
        }

        self.get_json("/v1/fs/stat", Some(&Query { path })).await
    }

    /// GET /v1/fs/read (raw bytes)
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure or body read failure,
    /// or [`MemoClientError::Api`] when the daemon returns an error response.
    pub async fn read(&self, path: &MountPath) -> Result<Vec<u8>, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
        }

        let response = self
            .request(Method::GET, "/v1/fs/read")
            .query(&Query { path })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        let response = self.ensure_success(response).await?;
        let bytes = response.bytes().await.map_err(MemoClientError::Request)?;
        Ok(bytes.to_vec())
    }

    /// GET /v1/fs/read (streaming response)
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// or [`MemoClientError::Api`] when the daemon returns an error response.
    pub async fn read_response(
        &self,
        path: &MountPath,
    ) -> Result<reqwest::Response, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
        }

        let response = self
            .request(Method::GET, "/v1/fs/read")
            .query(&Query { path })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.ensure_success(response).await
    }

    /// PUT /v1/fs/write (owned bytes)
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn write_bytes(
        &self,
        path: &MountPath,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<WriteResponse, MemoClientError> {
        self.write_body(path, Body::from(bytes.into())).await
    }

    /// PUT /v1/fs/write (stream/body)
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn write_body(
        &self,
        path: &MountPath,
        body: Body,
    ) -> Result<WriteResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
        }

        let response = self
            .request(Method::PUT, "/v1/fs/write")
            .query(&Query { path })
            .body(body)
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// POST /v1/fs/mkdir
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn mkdir(&self, path: &MountPath) -> Result<MkdirResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
        }

        let response = self
            .request(Method::POST, "/v1/fs/mkdir")
            .query(&Query { path })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// POST /v1/fs/mv
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn mv(
        &self,
        src: &MountPath,
        dst: &MountPath,
    ) -> Result<MoveResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            src: &'a MountPath,
            dst: &'a MountPath,
        }

        let response = self
            .request(Method::POST, "/v1/fs/mv")
            .query(&Query { src, dst })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// DELETE /v1/fs/rm
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn rm(
        &self,
        path: &MountPath,
        recursive: Option<bool>,
    ) -> Result<RemoveResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
            #[serde(skip_serializing_if = "Option::is_none")]
            recursive: Option<bool>,
        }

        let response = self
            .request(Method::DELETE, "/v1/fs/rm")
            .query(&Query { path, recursive })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// POST /v1/fs/cp
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn cp(
        &self,
        src: &MountPath,
        dst: &MountPath,
    ) -> Result<CopyResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            src: &'a MountPath,
            dst: &'a MountPath,
        }

        let response = self
            .request(Method::POST, "/v1/fs/cp")
            .query(&Query { src, dst })
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// GET /v1/fs/grep
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn grep(
        &self,
        path: &MountPath,
        pattern: &str,
        recursive: Option<bool>,
        case_sensitive: Option<bool>,
        max_results: Option<u64>,
    ) -> Result<GrepResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
            pattern: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            recursive: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            case_sensitive: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_results: Option<u64>,
        }

        self.get_json(
            "/v1/fs/grep",
            Some(&Query {
                path,
                pattern,
                recursive,
                case_sensitive,
                max_results,
            }),
        )
        .await
    }

    /// GET /v1/fs/find
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn find(
        &self,
        path: &MountPath,
        glob: &str,
        max_results: Option<u64>,
    ) -> Result<FindResponse, MemoClientError> {
        #[derive(Serialize)]
        struct Query<'a> {
            path: &'a MountPath,
            glob: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_results: Option<u64>,
        }

        self.get_json(
            "/v1/fs/find",
            Some(&Query {
                path,
                glob,
                max_results,
            }),
        )
        .await
    }

    /// GET /v1/meta/mounts
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn list_mounts(&self) -> Result<Vec<Mount>, MemoClientError> {
        let response: MountListResponse = self.get_json::<(), _>("/v1/meta/mounts", None).await?;
        Ok(response.mounts)
    }

    /// POST /v1/meta/mounts
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn create_mount(
        &self,
        request: &CreateMountRequest,
    ) -> Result<Mount, MemoClientError> {
        self.post_json("/v1/meta/mounts", request).await
    }

    /// GET /v1/meta/mounts/:name
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn get_mount(&self, name: &MountName) -> Result<Mount, MemoClientError> {
        let endpoint = format!("/v1/meta/mounts/{name}");
        self.get_json::<(), _>(&endpoint, None).await
    }

    /// PATCH /v1/meta/mounts/:name
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn update_mount(
        &self,
        name: &MountName,
        request: &UpdateMountRequest,
    ) -> Result<Mount, MemoClientError> {
        let endpoint = format!("/v1/meta/mounts/{name}");
        let response = self
            .request(Method::PATCH, &endpoint)
            .json(request)
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// DELETE /v1/meta/mounts/:name
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn remove_mount(
        &self,
        name: &MountName,
    ) -> Result<RemoveMountResponse, MemoClientError> {
        let endpoint = format!("/v1/meta/mounts/{name}");
        let response = self
            .request(Method::DELETE, &endpoint)
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// GET /v1/meta/tokens
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn list_tokens(&self) -> Result<Vec<TokenView>, MemoClientError> {
        let response: TokenListResponse = self.get_json::<(), _>("/v1/meta/tokens", None).await?;
        Ok(response.tokens)
    }

    /// POST /v1/meta/tokens
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn create_token(
        &self,
        request: &CreateTokenRequest,
    ) -> Result<CreatedToken, MemoClientError> {
        self.post_json("/v1/meta/tokens", request).await
    }

    /// DELETE /v1/meta/tokens/:id
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn revoke_token(&self, id: TokenId) -> Result<RevokeTokenResponse, MemoClientError> {
        let endpoint = format!("/v1/meta/tokens/{}", id.into_uuid());
        let response = self
            .request(Method::DELETE, &endpoint)
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    /// GET /v1/meta/audit
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn query_audit(&self, query: &AuditQuery) -> Result<AuditResponse, MemoClientError> {
        self.get_json("/v1/meta/audit", Some(query)).await
    }

    /// GET /health (no auth required)
    ///
    /// # Errors
    /// Returns [`MemoClientError::Request`] on transport failure,
    /// [`MemoClientError::Decode`] on invalid JSON, or [`MemoClientError::Api`]
    /// when the daemon returns an error response.
    pub async fn health(&self) -> Result<HealthResponse, MemoClientError> {
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    async fn get_json<Q, R>(&self, endpoint: &str, query: Option<&Q>) -> Result<R, MemoClientError>
    where
        Q: Serialize,
        R: DeserializeOwned,
    {
        let mut request = self.request(Method::GET, endpoint);
        if let Some(query) = query {
            request = request.query(query);
        }

        let response = request.send().await.map_err(MemoClientError::Request)?;
        self.decode_json(response).await
    }

    async fn post_json<B, R>(&self, endpoint: &str, body: &B) -> Result<R, MemoClientError>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let response = self
            .request(Method::POST, endpoint)
            .json(body)
            .send()
            .await
            .map_err(MemoClientError::Request)?;

        self.decode_json(response).await
    }

    fn request(&self, method: Method, endpoint: &str) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}{}", self.base_url, endpoint));

        if let Some(token) = &self.token {
            builder.bearer_auth(token)
        } else {
            builder
        }
    }

    async fn decode_json<T>(&self, response: reqwest::Response) -> Result<T, MemoClientError>
    where
        T: DeserializeOwned,
    {
        let response = self.ensure_success(response).await?;
        let body = response.bytes().await.map_err(MemoClientError::Request)?;
        serde_json::from_slice::<T>(&body).map_err(MemoClientError::Decode)
    }

    async fn ensure_success(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, MemoClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let body = response.text().await.map_err(MemoClientError::Request)?;
        if let Ok(parsed) = serde_json::from_str::<ErrorResponse>(&body) {
            return Err(MemoClientError::Api(map_api_error(parsed.error)));
        }

        Err(MemoClientError::Api(map_status_fallback(status, &body)))
    }
}

fn sanitize_base_url(base_url: &str) -> Result<String, MemoClientError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|_| MemoClientError::InvalidBaseUrl(base_url.to_owned()))?;

    Ok(parsed.as_str().trim_end_matches('/').to_owned())
}

fn map_status_fallback(status: StatusCode, message: &str) -> ApiError {
    let message = if message.trim().is_empty() {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_owned()
    } else {
        message.to_owned()
    };

    match status {
        StatusCode::BAD_REQUEST => ApiError::InvalidPath(message),
        StatusCode::UNAUTHORIZED => ApiError::AuthRequired,
        StatusCode::FORBIDDEN => ApiError::PermissionDenied,
        StatusCode::NOT_FOUND => ApiError::NotFound(message),
        StatusCode::CONFLICT => ApiError::Conflict(message),
        StatusCode::PAYLOAD_TOO_LARGE => ApiError::TooLarge {
            limit: 0,
            actual: 0,
        },
        _ => ApiError::Internal(message),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashMap;

    use axum::{
        body::Body as AxumBody,
        extract::{Path, Query},
        http::StatusCode,
        response::IntoResponse,
        routing::{delete, get, post, put},
        Json, Router,
    };
    use memo_core::{ApiError, MountName, MountPath, Scope, ScopeSet};
    use serde_json::{json, Value};
    use tokio::net::TcpListener;

    use super::{
        map_status_fallback, AuditQuery, CreateTokenRequest, MemoClient, MemoClientConfig,
        MemoClientError, PatchValue, UpdateMountRequest,
    };

    fn fixture_ts() -> Value {
        serde_json::to_value(
            time::OffsetDateTime::from_unix_timestamp(1_705_314_600)
                .expect("timestamp should be valid"),
        )
        .expect("timestamp should serialize")
    }

    fn mount_json() -> Value {
        json!({
            "name": "VaultKB",
            "root_path": "/tmp",
            "mode": "read_write",
            "audience": "shared",
            "description": "shared knowledge base",
            "policy": {
                "hide_globs": [],
                "deny_read_globs": [],
                "deny_write_globs": [],
                "max_read_bytes": null,
                "max_write_bytes": null
            },
            "created_at": fixture_ts(),
            "updated_at": fixture_ts()
        })
    }

    async fn fs_ls() -> Json<Value> {
        Json(json!({
            "path": "VaultKB:/notes",
            "entries": [
                { "name": "git.md", "kind": "file", "size": 32, "modified_at": fixture_ts() }
            ]
        }))
    }

    async fn fs_tree() -> Json<Value> {
        Json(json!({
            "path": "VaultKB:/",
            "depth": 2,
            "truncated": false,
            "tree": {
                "name": "",
                "kind": "dir",
                "children": [
                    {
                        "name": "notes",
                        "kind": "dir",
                        "children": [
                            { "name": "git.md", "kind": "file", "size": 32, "modified_at": fixture_ts() }
                        ]
                    }
                ]
            }
        }))
    }

    async fn fs_stat(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
        let path = query.get("path").map_or("", String::as_str);
        if path.contains("missing") {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": {
                        "code": "not_found",
                        "message": "missing path",
                        "mount": "VaultKB",
                        "path": "/notes/missing.md"
                    }
                })),
            )
                .into_response();
        }

        (
            StatusCode::OK,
            Json(json!({
                "path": "VaultKB:/notes/git.md",
                "kind": "file",
                "size": 32,
                "modified_at": fixture_ts(),
                "created_at": fixture_ts(),
                "memo_summary": "sample"
            })),
        )
            .into_response()
    }

    async fn fs_read() -> impl IntoResponse {
        (StatusCode::OK, AxumBody::from("hello world"))
    }

    async fn fs_write() -> Json<Value> {
        Json(json!({ "path": "VaultKB:/notes/git.md", "written_bytes": 11 }))
    }

    async fn fs_mkdir() -> Json<Value> {
        Json(json!({ "path": "VaultKB:/notes/new-dir", "created": true }))
    }

    async fn fs_mv() -> Json<Value> {
        Json(json!({ "src": "VaultKB:/drafts/x.md", "dst": "VaultKB:/notes/x.md" }))
    }

    async fn fs_rm() -> Json<Value> {
        Json(json!({ "path": "VaultKB:/drafts/old.md", "deleted": true }))
    }

    async fn fs_cp() -> Json<Value> {
        Json(json!({ "src": "VaultKB:/notes/a.md", "dst": "VaultKB:/archive/a.md" }))
    }

    async fn fs_grep() -> Json<Value> {
        Json(json!({
            "pattern": "kubernetes",
            "matches": [
                {
                    "path": "VaultKB:/notes/k8s.md",
                    "line": 42,
                    "content": "Kubernetes uses declarative configuration."
                }
            ]
        }))
    }

    async fn fs_find() -> Json<Value> {
        Json(json!({
            "glob": "*.md",
            "results": [
                { "path": "VaultKB:/notes/git.md", "size": 32, "modified_at": fixture_ts() }
            ]
        }))
    }

    async fn meta_list_mounts() -> Json<Value> {
        Json(json!({ "mounts": [mount_json()] }))
    }

    async fn meta_create_mount() -> Json<Value> {
        Json(mount_json())
    }

    async fn meta_get_mount(Path(_name): Path<String>) -> Json<Value> {
        Json(mount_json())
    }

    async fn meta_update_mount(Path(_name): Path<String>) -> Json<Value> {
        Json(mount_json())
    }

    async fn meta_remove_mount(Path(name): Path<String>) -> Json<Value> {
        Json(json!({ "name": name, "removed": true }))
    }

    async fn meta_list_tokens() -> Json<Value> {
        Json(json!({
            "tokens": [
                {
                    "id": "550e8400-e29b-41d4-a716-446655440000",
                    "name": "agent",
                    "scopes": ["fs:VaultKB:read"],
                    "created_at": fixture_ts(),
                    "expires_at": "Never",
                    "last_used_at": fixture_ts()
                }
            ]
        }))
    }

    async fn meta_create_token() -> Json<Value> {
        Json(json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "agent",
            "token": "memo_abc",
            "scopes": ["fs:VaultKB:read"],
            "created_at": fixture_ts(),
            "expires_at": "Never"
        }))
    }

    async fn meta_revoke_token(Path(id): Path<String>) -> Json<Value> {
        Json(json!({ "id": id, "revoked": true }))
    }

    async fn meta_audit() -> Json<Value> {
        Json(json!({
            "entries": [
                {
                    "id": 1,
                    "timestamp": fixture_ts(),
                    "token_id": "550e8400-e29b-41d4-a716-446655440000",
                    "operation": "read",
                    "mount": "VaultKB",
                    "path": "/notes/git.md",
                    "result": "ok",
                    "error_code": null
                }
            ]
        }))
    }

    async fn health() -> Json<Value> {
        Json(json!({ "status": "ok", "version": "0.1.0" }))
    }

    async fn spawn_server() -> String {
        let app = Router::new()
            .route("/v1/fs/ls", get(fs_ls))
            .route("/v1/fs/tree", get(fs_tree))
            .route("/v1/fs/stat", get(fs_stat))
            .route("/v1/fs/read", get(fs_read))
            .route("/v1/fs/write", put(fs_write))
            .route("/v1/fs/mkdir", post(fs_mkdir))
            .route("/v1/fs/mv", post(fs_mv))
            .route("/v1/fs/rm", delete(fs_rm))
            .route("/v1/fs/cp", post(fs_cp))
            .route("/v1/fs/grep", get(fs_grep))
            .route("/v1/fs/find", get(fs_find))
            .route(
                "/v1/meta/mounts",
                get(meta_list_mounts).post(meta_create_mount),
            )
            .route(
                "/v1/meta/mounts/{name}",
                get(meta_get_mount)
                    .patch(meta_update_mount)
                    .delete(meta_remove_mount),
            )
            .route(
                "/v1/meta/tokens",
                get(meta_list_tokens).post(meta_create_token),
            )
            .route("/v1/meta/tokens/{id}", delete(meta_revoke_token))
            .route("/v1/meta/audit", get(meta_audit))
            .route("/health", get(health));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("local addr should exist");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        format!("http://{addr}")
    }

    #[test]
    fn base_url_is_normalized() {
        let client = MemoClient::new(MemoClientConfig {
            base_url: "http://127.0.0.1:18301/".to_owned(),
            ..MemoClientConfig::default()
        })
        .expect("client should construct");

        assert_eq!(client.base_url(), "http://127.0.0.1:18301");
    }

    #[test]
    fn invalid_base_url_is_rejected() {
        let result = MemoClient::for_base_url("not a url");
        assert!(matches!(result, Err(MemoClientError::InvalidBaseUrl(_))));
    }

    #[test]
    fn fallback_maps_404_to_not_found() {
        let error = map_status_fallback(reqwest::StatusCode::NOT_FOUND, "missing");
        assert_eq!(error.code(), "not_found");
    }

    #[test]
    fn audit_query_serializes_only_present_fields() {
        let query = AuditQuery::default();
        let json = serde_json::to_value(query).expect("query should serialize");

        assert!(json.get("mount").is_none());
        assert!(json.get("limit").is_none());
    }

    #[test]
    fn create_token_request_serializes_scopes() {
        let request = CreateTokenRequest {
            name: "agent".to_owned(),
            scopes: ScopeSet::new(["fs:VaultKB:read"
                .parse::<Scope>()
                .expect("scope should parse")]),
            expires_at: None,
        };

        let json = serde_json::to_string(&request).expect("request should serialize");
        assert!(json.contains("fs:VaultKB:read"));
    }

    #[test]
    fn update_mount_request_serializes_null_for_clears() {
        let request = UpdateMountRequest {
            description: Some(PatchValue::Null),
            max_read_bytes: Some(PatchValue::Null),
            ..UpdateMountRequest::default()
        };

        let json = serde_json::to_value(request).expect("request should serialize");
        assert!(json
            .get("description")
            .is_some_and(serde_json::Value::is_null));
        assert!(json
            .get("max_read_bytes")
            .is_some_and(serde_json::Value::is_null));
    }

    #[test]
    fn mount_path_query_serializes_to_protocol_string() {
        let path = MountPath::parse("VaultKB:/notes/git.md").expect("path should parse");
        let json = serde_json::to_string(&serde_json::json!({ "path": path }))
            .expect("path query should serialize");
        assert!(json.contains("VaultKB:/notes/git.md"));
    }

    #[test]
    fn mount_name_formats_for_endpoint_segments() {
        let name = MountName::new("VaultKB").expect("mount should parse");
        assert_eq!(name.to_string(), "VaultKB");
    }

    async fn build_client() -> (MemoClient, MountPath, MountPath, MountPath, MountName) {
        let base_url = spawn_server().await;
        let path = MountPath::parse("VaultKB:/notes/git.md").expect("path should parse");
        let src = MountPath::parse("VaultKB:/notes/a.md").expect("src should parse");
        let dst = MountPath::parse("VaultKB:/archive/a.md").expect("dst should parse");
        let mount_name = MountName::new("VaultKB").expect("mount name should parse");

        let client = MemoClient::for_base_url(base_url)
            .expect("client should construct")
            .with_token("memo_test")
            .with_timeout(std::time::Duration::from_secs(10))
            .expect("timeout should apply");

        (client, path, src, dst, mount_name)
    }

    #[tokio::test]
    async fn memo_client_exercises_filesystem_endpoint_wrappers() {
        let (client, path, src, dst, _) = build_client().await;

        let ls = client
            .ls(
                &MountPath::parse("VaultKB:/notes").expect("path should parse"),
                Some(true),
            )
            .await
            .expect("ls should succeed");
        assert_eq!(ls.entries.len(), 1);

        let tree = client
            .tree(
                &MountPath::parse("VaultKB:/").expect("root path should parse"),
                Some(2),
            )
            .await
            .expect("tree should succeed");
        assert_eq!(tree.depth, 2);

        let stat = client.stat(&path).await.expect("stat should succeed");
        assert_eq!(stat.memo_summary.as_deref(), Some("sample"));

        let read = client.read(&path).await.expect("read should succeed");
        assert_eq!(read, b"hello world");

        let read_response = client
            .read_response(&path)
            .await
            .expect("read response should succeed");
        let read_response_bytes = read_response
            .bytes()
            .await
            .expect("read response bytes should load");
        assert_eq!(&read_response_bytes[..], b"hello world");

        let write_result = client
            .write_bytes(&path, b"hello world".to_vec())
            .await
            .expect("write bytes should succeed");
        assert_eq!(write_result.written_bytes, 11);

        let write_body_result = client
            .write_body(&path, reqwest::Body::from("hello world"))
            .await
            .expect("write body should succeed");
        assert_eq!(write_body_result.written_bytes, 11);

        let mkdir = client
            .mkdir(&MountPath::parse("VaultKB:/notes/new-dir").expect("mkdir path should parse"))
            .await
            .expect("mkdir should succeed");
        assert!(mkdir.created);

        let mv = client
            .mv(
                &MountPath::parse("VaultKB:/drafts/x.md").expect("src path should parse"),
                &path,
            )
            .await
            .expect("mv should succeed");
        assert_eq!(mv.dst.to_string(), "VaultKB:/notes/x.md");

        let rm = client
            .rm(
                &MountPath::parse("VaultKB:/drafts/old.md").expect("rm path should parse"),
                Some(true),
            )
            .await
            .expect("rm should succeed");
        assert!(rm.deleted);

        let cp = client.cp(&src, &dst).await.expect("cp should succeed");
        assert_eq!(cp.dst.to_string(), "VaultKB:/archive/a.md");

        let grep = client
            .grep(
                &MountPath::parse("VaultKB:/notes").expect("grep path should parse"),
                "kubernetes",
                Some(true),
                Some(true),
                Some(100),
            )
            .await
            .expect("grep should succeed");
        assert_eq!(grep.matches.len(), 1);

        let find = client
            .find(
                &MountPath::parse("VaultKB:/notes").expect("find path should parse"),
                "*.md",
                Some(100),
            )
            .await
            .expect("find should succeed");
        assert_eq!(find.results.len(), 1);
    }

    #[tokio::test]
    async fn memo_client_exercises_meta_endpoint_wrappers() {
        let (client, _, _, _, mount_name) = build_client().await;

        let mounts = client
            .list_mounts()
            .await
            .expect("list mounts should succeed");
        assert_eq!(mounts.len(), 1);

        let created_mount = client
            .create_mount(&super::CreateMountRequest {
                name: mount_name.clone(),
                root_path: "/tmp".into(),
                mode: memo_core::MountMode::ReadWrite,
                audience: memo_core::Audience::Shared,
                description: Some("shared knowledge base".to_owned()),
                hide_globs: Vec::new(),
                deny_read_globs: Vec::new(),
                deny_write_globs: Vec::new(),
                max_read_bytes: None,
                max_write_bytes: None,
            })
            .await
            .expect("create mount should succeed");
        assert_eq!(created_mount.name.to_string(), "VaultKB");

        let fetched_mount = client
            .get_mount(&mount_name)
            .await
            .expect("get mount should succeed");
        assert_eq!(fetched_mount.name.to_string(), "VaultKB");

        let updated_mount = client
            .update_mount(
                &mount_name,
                &super::UpdateMountRequest {
                    description: Some(PatchValue::Value("updated".to_owned())),
                    ..super::UpdateMountRequest::default()
                },
            )
            .await
            .expect("update mount should succeed");
        assert_eq!(updated_mount.name.to_string(), "VaultKB");

        let removed_mount = client
            .remove_mount(&mount_name)
            .await
            .expect("remove mount should succeed");
        assert!(removed_mount.removed);

        let tokens = client
            .list_tokens()
            .await
            .expect("list tokens should succeed");
        assert_eq!(tokens.len(), 1);

        let created_token = client
            .create_token(&CreateTokenRequest {
                name: "agent".to_owned(),
                scopes: ScopeSet::new(["fs:VaultKB:read"
                    .parse::<Scope>()
                    .expect("scope should parse")]),
                expires_at: None,
            })
            .await
            .expect("create token should succeed");
        assert_eq!(created_token.name, "agent");

        let revoked_token = client
            .revoke_token(created_token.id)
            .await
            .expect("revoke token should succeed");
        assert!(revoked_token.revoked);

        let audit = client
            .query_audit(&AuditQuery::default())
            .await
            .expect("query audit should succeed");
        assert_eq!(audit.entries.len(), 1);

        let health = client.health().await.expect("health should succeed");
        assert_eq!(health.status, "ok");
    }

    #[tokio::test]
    async fn memo_client_maps_api_errors_from_error_response() {
        let base_url = spawn_server().await;
        let client = MemoClient::for_base_url(base_url).expect("client should construct");
        let missing_path =
            MountPath::parse("VaultKB:/notes/missing.md").expect("path should parse");

        let error = client
            .stat(&missing_path)
            .await
            .expect_err("stat should fail for missing path");

        match error {
            MemoClientError::Api(ApiError::NotFound(message)) => {
                assert!(message.contains("missing path"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
