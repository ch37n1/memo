// memod: daemon process. Owns all filesystem I/O.
// Serves an axum/tokio HTTP API on 127.0.0.1:18301.

pub mod auth;
pub mod db;
pub mod fs;
pub mod mount_registry;

use std::future::pending;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use auth::middleware::{auth_middleware, AuthState, VerifiedToken};
use auth::repository::SqliteTokenRepository;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use memo_core::repositories::MountRepository;
use memo_core::repositories::TokenRepository;
use memo_core::{ApiError, AuthError, DbError, MountPath, TokenId};
use mount_registry::repository::{PolicyCache, SqliteMountRepository};
use serde::Serialize;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct AppConfig {
    bind_addr: String,
    database_url: String,
    bootstrap_token_path: PathBuf,
}

impl AppConfig {
    fn from_env() -> Self {
        let bind_addr =
            std::env::var("MEMOD_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:18301".to_owned());
        let database_url =
            std::env::var("MEMOD_DATABASE_URL").unwrap_or_else(|_| "sqlite://memo.db".to_owned());
        let bootstrap_token_path = std::env::var("MEMOD_BOOTSTRAP_TOKEN_PATH")
            .map_or_else(|_| default_bootstrap_token_path(), PathBuf::from);

        Self {
            bind_addr,
            database_url,
            bootstrap_token_path,
        }
    }
}

#[derive(Clone)]
struct AppState {
    token_repository: Arc<SqliteTokenRepository>,
    mount_repository: Arc<SqliteMountRepository>,
    file_system_service: Arc<fs::ops::FileSystemService>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ErrorEnvelope,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct HttpError(pub ApiError);

impl From<ApiError> for HttpError {
    fn from(value: ApiError) -> Self {
        Self(value)
    }
}

impl From<AuthError> for HttpError {
    fn from(value: AuthError) -> Self {
        Self(ApiError::from(value))
    }
}

impl From<DbError> for HttpError {
    fn from(value: DbError) -> Self {
        Self(ApiError::from(value))
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            ApiError::AuthRequired | ApiError::TokenInvalid | ApiError::TokenExpired => {
                StatusCode::UNAUTHORIZED
            }
            ApiError::PermissionDenied
            | ApiError::PolicyViolated(_)
            | ApiError::OutOfBounds
            | ApiError::SymlinkDenied => StatusCode::FORBIDDEN,
            ApiError::InvalidPath(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) | ApiError::MountNotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let code = self.0.code().to_owned();
        let message = self.0.to_string();
        let (limit, actual) = match &self.0 {
            ApiError::TooLarge { limit, actual } => (Some(*limit), Some(*actual)),
            _ => (None, None),
        };

        (
            status,
            Json(ErrorResponse {
                error: ErrorEnvelope {
                    code,
                    message,
                    mount: None,
                    path: None,
                    limit,
                    actual,
                },
            }),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(AppConfig::from_env()).await {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "{error}");
        std::process::exit(1);
    }
}

async fn run(config: AppConfig) -> Result<(), ApiError> {
    let pool = db::init_pool(&db::DbConfig::new(config.database_url)).await?;
    let token_repository = Arc::new(SqliteTokenRepository::new(pool.clone()));
    let mount_repository = Arc::new(SqliteMountRepository::new(pool, PolicyCache::new()));

    if auth::bootstrap_admin_token_if_needed(&token_repository, &config.bootstrap_token_path)
        .await?
        .is_some()
    {
        let mut stderr = std::io::stderr();
        let _ = writeln!(
            stderr,
            "bootstrap token written to {}",
            config.bootstrap_token_path.display()
        );
    }

    let state = AppState {
        file_system_service: Arc::new(fs::ops::FileSystemService::new(Arc::clone(
            &mount_repository,
        ))),
        token_repository,
        mount_repository,
    };

    let router = app_router(&state);
    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    serve(listener, router, pending()).await
}

async fn serve(
    listener: tokio::net::TcpListener,
    router: Router,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), ApiError> {
    let server = axum::serve(listener, router).with_graceful_shutdown(shutdown);
    server
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))
}

fn app_router(state: &AppState) -> Router {
    let auth_state = AuthState {
        token_repository: Arc::clone(&state.token_repository),
    };

    let token_routes = Router::new()
        .route("/v1/meta/tokens", get(list_tokens).post(create_token))
        .route("/v1/meta/tokens/{id}", delete(revoke_token))
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
        .with_state(state.clone());

    let mount_routes = Router::new()
        .route("/v1/meta/mounts", get(list_mounts).post(create_mount))
        .route(
            "/v1/meta/mounts/{name}",
            get(get_mount).patch(update_mount).delete(remove_mount),
        )
        .layer(middleware::from_fn_with_state(
            AuthState {
                token_repository: Arc::clone(&state.token_repository),
            },
            auth_middleware,
        ))
        .with_state(state.clone());

    let fs_routes = Router::new()
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
        .layer(middleware::from_fn_with_state(
            AuthState {
                token_repository: Arc::clone(&state.token_repository),
            },
            auth_middleware,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/health", get(health))
        .merge(token_routes)
        .merge(mount_routes)
        .merge(fs_routes)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn list_tokens(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
) -> Result<Json<auth::TokenListResponse>, HttpError> {
    if !auth::require_token_list_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let tokens = state.token_repository.list().await?;
    Ok(Json(auth::TokenListResponse { tokens }))
}

async fn create_token(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Json(payload): Json<auth::CreateTokenRequest>,
) -> Result<Json<auth::CreatedTokenResponse>, HttpError> {
    if !auth::require_token_admin_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let created = auth::create_token(&state.token_repository, payload).await?;
    Ok(Json(created))
}

async fn revoke_token(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(id): Path<String>,
) -> Result<Json<auth::RevokeTokenResponse>, HttpError> {
    if !auth::require_token_admin_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let parsed = Uuid::from_str(&id)
        .map_err(|_| HttpError(ApiError::InvalidPath("invalid token id".to_owned())))?;
    let token_id = TokenId::from_uuid(parsed);

    state.token_repository.delete(&token_id).await?;

    Ok(Json(auth::RevokeTokenResponse {
        id: token_id,
        revoked: true,
    }))
}

fn default_bootstrap_token_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home)
        .join(".config")
        .join("memo")
        .join("bootstrap.token")
}

async fn list_mounts(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
) -> Result<Json<mount_registry::MountListResponse>, HttpError> {
    if !mount_registry::require_mount_read_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let mounts = state.mount_repository.list().await?;
    Ok(Json(mount_registry::MountListResponse { mounts }))
}

async fn create_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Json(payload): Json<mount_registry::CreateMountRequest>,
) -> Result<Json<memo_core::Mount>, HttpError> {
    if !mount_registry::require_mount_admin_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let created = mount_registry::create_mount(&state.mount_repository, payload)
        .await
        .map_err(|error| match error {
            DbError::Conflict => HttpError(ApiError::Conflict("mount already exists".to_owned())),
            other => HttpError(other.into()),
        })?;

    Ok(Json(created))
}

async fn get_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(name): Path<String>,
) -> Result<Json<memo_core::Mount>, HttpError> {
    if !mount_registry::require_mount_read_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let mount_name = mount_registry::parse_mount_name(&name).map_err(HttpError)?;
    let mount = state
        .mount_repository
        .find(&mount_name)
        .await
        .map_err(|error| map_mount_not_found(&mount_name, error))?;
    Ok(Json(mount))
}

async fn update_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(name): Path<String>,
    Json(payload): Json<mount_registry::UpdateMountRequest>,
) -> Result<Json<memo_core::Mount>, HttpError> {
    if !mount_registry::require_mount_admin_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let mount_name = mount_registry::parse_mount_name(&name).map_err(HttpError)?;
    let updated = mount_registry::update_mount(&state.mount_repository, &mount_name, payload)
        .await
        .map_err(|error| map_mount_not_found(&mount_name, error))?;

    Ok(Json(updated))
}

async fn remove_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(name): Path<String>,
) -> Result<Json<mount_registry::RemoveMountResponse>, HttpError> {
    if !mount_registry::require_mount_admin_scope(&token) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let mount_name = mount_registry::parse_mount_name(&name).map_err(HttpError)?;
    state
        .mount_repository
        .delete(&mount_name)
        .await
        .map_err(|error| map_mount_not_found(&mount_name, error))?;

    Ok(Json(mount_registry::RemoveMountResponse {
        name: mount_name,
        removed: true,
    }))
}

fn map_mount_not_found(name: &memo_core::MountName, error: DbError) -> HttpError {
    match error {
        DbError::NotFound => HttpError(ApiError::MountNotFound(name.to_string())),
        other => HttpError(other.into()),
    }
}

#[derive(Debug, serde::Deserialize)]
struct FsPathQuery {
    path: MountPath,
}

#[derive(Debug, serde::Deserialize)]
struct FsLsQuery {
    path: MountPath,
    #[serde(default)]
    info: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct FsTreeQuery {
    path: MountPath,
    #[serde(default)]
    depth: Option<u8>,
}

#[derive(Debug, serde::Deserialize)]
struct FsRmQuery {
    path: MountPath,
    #[serde(default)]
    recursive: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct FsMvQuery {
    src: MountPath,
    dst: MountPath,
}

#[derive(Debug, serde::Deserialize)]
struct FsCpQuery {
    src: MountPath,
    dst: MountPath,
}

#[derive(Debug, serde::Deserialize)]
struct FsGrepQuery {
    path: MountPath,
    pattern: String,
    #[serde(default)]
    recursive: Option<bool>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    max_results: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct FsFindQuery {
    path: MountPath,
    glob: String,
    #[serde(default)]
    max_results: Option<u64>,
}

async fn fs_ls(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsLsQuery>,
) -> Result<Json<fs::ops::LsResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .ls(&query.path, query.info.unwrap_or(false))
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_tree(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsTreeQuery>,
) -> Result<Json<fs::ops::TreeResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .tree(&query.path, query.depth)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_stat(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<fs::ops::StatResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .stat(&query.path)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_read(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
) -> Result<Response, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let (resolved, size, mime) = state
        .file_system_service
        .read_file_meta(&query.path)
        .await
        .map_err(HttpError)?;
    let content_type = axum::http::HeaderValue::from_str(&mime)
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("application/octet-stream"));

    let mut response = if size > 4 * 1024 * 1024 {
        let file = tokio::fs::File::open(resolved)
            .await
            .map_err(|error| HttpError(ApiError::Internal(error.to_string())))?;
        let stream = ReaderStream::new(file);
        Body::from_stream(stream).into_response()
    } else {
        let bytes = tokio::fs::read(resolved)
            .await
            .map_err(|error| HttpError(ApiError::Internal(error.to_string())))?;
        bytes.into_response()
    };

    response
        .headers_mut()
        .insert(axum::http::header::CONTENT_TYPE, content_type);
    Ok(response)
}

async fn fs_write(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
    body: Body,
) -> Result<Json<fs::ops::WriteResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| HttpError(ApiError::Internal(error.to_string())))?;

    state
        .file_system_service
        .write_file(&query.path, &bytes)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_mkdir(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<fs::ops::MkdirResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .mkdir(&query.path)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_mv(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsMvQuery>,
) -> Result<Json<fs::ops::MoveResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.src.mount())
        || !fs::ops::require_fs_write_scope(&token, query.dst.mount())
    {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .mv(&query.src, &query.dst)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_rm(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsRmQuery>,
) -> Result<Json<fs::ops::RemoveResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .rm(&query.path, query.recursive.unwrap_or(false))
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_cp(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsCpQuery>,
) -> Result<Json<fs::ops::CopyResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.src.mount())
        || !fs::ops::require_fs_write_scope(&token, query.dst.mount())
    {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .cp(&query.src, &query.dst)
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_grep(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsGrepQuery>,
) -> Result<Json<fs::ops::GrepResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .grep(
            &query.path,
            &query.pattern,
            query.recursive.unwrap_or(true),
            query.case_sensitive.unwrap_or(true),
            query.max_results.unwrap_or(100),
        )
        .await
        .map(Json)
        .map_err(HttpError)
}

async fn fs_find(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsFindQuery>,
) -> Result<Json<fs::ops::FindResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(HttpError(ApiError::PermissionDenied));
    }

    state
        .file_system_service
        .find(&query.path, &query.glob, query.max_results.unwrap_or(100))
        .await
        .map(Json)
        .map_err(HttpError)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::Write;

    use memo_client::{MemoClient, MemoClientConfig};
    use tempfile::tempdir;

    use super::app_router;
    use crate::auth::repository::SqliteTokenRepository;
    use crate::db::{init_pool, DbConfig};
    use crate::mount_registry::repository::{PolicyCache, SqliteMountRepository};

    #[tokio::test]
    async fn bootstrap_flow() -> Result<(), Box<dyn Error>> {
        let tempdir = tempdir()?;
        let db_path = tempdir.path().join("memo.db");
        let db_url = format!("sqlite://{}", db_path.display());
        let bootstrap_path = tempdir.path().join("bootstrap.token");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let pool = init_pool(&DbConfig::new(db_url.clone())).await?;
        let token_repository = std::sync::Arc::new(SqliteTokenRepository::new(pool.clone()));
        let mount_repository =
            std::sync::Arc::new(SqliteMountRepository::new(pool, PolicyCache::new()));
        let state = super::AppState {
            file_system_service: std::sync::Arc::new(crate::fs::ops::FileSystemService::new(
                std::sync::Arc::clone(&mount_repository),
            )),
            token_repository,
            mount_repository,
        };

        if crate::auth::bootstrap_admin_token_if_needed(&state.token_repository, &bootstrap_path)
            .await?
            .is_some()
        {
            let mut stderr = std::io::stderr();
            let _ = writeln!(
                stderr,
                "bootstrap token written to {}",
                bootstrap_path.display()
            );
        }

        let app = app_router(&state);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(super::serve(listener, app, async {
            let _ = shutdown_rx.await;
        }));

        let token = tokio::fs::read_to_string(&bootstrap_path)
            .await?
            .trim()
            .to_owned();

        let client = MemoClient::new(MemoClientConfig {
            base_url: format!("http://{addr}"),
            token: Some(token),
            ..MemoClientConfig::default()
        })?;

        let tokens = client.list_tokens().await?;
        assert!(!tokens.is_empty());

        let _ = shutdown_tx.send(());
        handle.await??;

        Ok(())
    }
}
