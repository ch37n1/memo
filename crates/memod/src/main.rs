// memod: daemon process. Owns all filesystem I/O.
// Serves an axum/tokio HTTP API on 127.0.0.1:18301.

pub mod audit;
pub mod auth;
pub mod db;
pub mod fs;
pub mod mount_registry;

use std::io::Write;
use std::path::{Path as StdPath, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use audit::{AuditQuery, AuditRecord, AuditResponse, AuditService};
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
use memo_core::{
    ApiError, AuthError, DbError, DenialReason, DomainEvent, MetaScope, MountPath, Scope, TokenId,
};
use mount_registry::repository::{PolicyCache, SqliteMountRepository};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct AppConfig {
    bind_addr: String,
    database_url: String,
    log_level: String,
    log_path: PathBuf,
    write_fsync: bool,
    write_dir_sync: bool,
    pid_file_path: PathBuf,
    audit_log_path: PathBuf,
    max_audit_log_rows: usize,
    bootstrap_token_path: PathBuf,
}

impl AppConfig {
    fn load() -> Result<Self, ApiError> {
        ensure_xdg_dirs().map_err(|error| ApiError::Internal(error.to_string()))?;
        let from_file =
            MemodFileConfig::load().map_err(|error| ApiError::Internal(error.to_string()))?;
        let defaults = Defaults::detect();

        let bind_addr = std::env::var("MEMOD_BIND_ADDR")
            .ok()
            .or_else(|| {
                from_file
                    .daemon
                    .as_ref()
                    .and_then(|daemon| daemon.bind_addr.clone())
            })
            .unwrap_or_else(|| "127.0.0.1:18301".to_owned());
        let db_path = from_file.db_path.unwrap_or_else(|| defaults.db.clone());
        let database_url = std::env::var("MEMOD_DATABASE_URL")
            .unwrap_or_else(|_| format!("sqlite://{}", db_path.display()));
        let log_path = from_file.log_path.unwrap_or_else(|| defaults.log.clone());
        let bootstrap_token_path = std::env::var("MEMOD_BOOTSTRAP_TOKEN_PATH")
            .ok()
            .map_or_else(|| defaults.bootstrap_token.clone(), PathBuf::from);
        let log_level = from_file
            .daemon
            .as_ref()
            .and_then(|daemon| daemon.log_level.clone())
            .unwrap_or_else(|| "info".to_owned());
        let max_audit_log_rows = from_file
            .daemon
            .as_ref()
            .and_then(|daemon| daemon.limits.as_ref())
            .and_then(|limits| limits.max_audit_log_rows)
            .unwrap_or(100_000);
        let write_fsync = from_file
            .daemon
            .as_ref()
            .and_then(|daemon| daemon.write.as_ref())
            .and_then(|write| write.fsync)
            .unwrap_or(true);
        let write_dir_sync = from_file
            .daemon
            .as_ref()
            .and_then(|daemon| daemon.write.as_ref())
            .and_then(|write| write.dir_sync)
            .unwrap_or(true);

        Self {
            bind_addr,
            database_url,
            log_level,
            log_path,
            write_fsync,
            write_dir_sync,
            pid_file_path: defaults.pid_file,
            audit_log_path: defaults.audit_log,
            max_audit_log_rows,
            bootstrap_token_path,
        }
        .validate()
    }

    fn validate(self) -> Result<Self, ApiError> {
        if self.max_audit_log_rows == 0 {
            return Err(ApiError::InvalidPath(
                "daemon.limits.max_audit_log_rows must be greater than zero".to_owned(),
            ));
        }

        Ok(self)
    }
}

#[derive(Clone)]
struct AppState {
    token_repository: Arc<SqliteTokenRepository>,
    mount_repository: Arc<SqliteMountRepository>,
    file_system_service: Arc<fs::ops::FileSystemService>,
    audit_service: Arc<AuditService>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct MemodFileConfig {
    #[serde(default)]
    daemon: Option<DaemonConfig>,
    #[serde(default)]
    db_path: Option<PathBuf>,
    #[serde(default)]
    log_path: Option<PathBuf>,
}

impl MemodFileConfig {
    fn load() -> Result<Self, std::io::Error> {
        let path = default_config_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let body = std::fs::read_to_string(path)?;
        Ok(parse_memod_config(&body))
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DaemonConfig {
    #[serde(default)]
    bind_addr: Option<String>,
    #[serde(default)]
    log_level: Option<String>,
    #[serde(default)]
    write: Option<DaemonWriteConfig>,
    #[serde(default)]
    limits: Option<DaemonLimits>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DaemonWriteConfig {
    #[serde(default)]
    fsync: Option<bool>,
    #[serde(default)]
    dir_sync: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct DaemonLimits {
    #[serde(default)]
    max_audit_log_rows: Option<usize>,
}

#[derive(Debug, Clone)]
struct Defaults {
    db: PathBuf,
    log: PathBuf,
    audit_log: PathBuf,
    bootstrap_token: PathBuf,
    pid_file: PathBuf,
}

impl Defaults {
    fn detect() -> Self {
        Self {
            db: xdg_data_home().join("memo/memo.db"),
            log: xdg_state_home().join("memo/memod.log"),
            audit_log: xdg_state_home().join("memo/audit.log"),
            bootstrap_token: default_bootstrap_token_path(),
            pid_file: default_pid_file_path(),
        }
    }
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
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            let mut stderr = std::io::stderr();
            let _ = writeln!(stderr, "{error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = run(config).await {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "{error}");
        std::process::exit(1);
    }
}

async fn run(config: AppConfig) -> Result<(), ApiError> {
    init_logging(&config)?;
    let _pid_file = PidFileGuard::create(&config.pid_file_path)
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let pool = db::init_pool(&db::DbConfig::new(config.database_url)).await?;
    let token_repository = Arc::new(SqliteTokenRepository::new(pool.clone()));
    let mount_repository = Arc::new(SqliteMountRepository::new(pool, PolicyCache::new()));
    let audit_service = Arc::new(
        AuditService::new(config.audit_log_path.clone())
            .map_err(|error| ApiError::Internal(error.to_string()))?,
    );

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

    if let Err(error) = audit_service
        .prune_if_exceeds(config.max_audit_log_rows)
        .await
    {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "failed to prune audit log at startup: {error}");
    }

    let state = AppState {
        file_system_service: Arc::new(fs::ops::FileSystemService::new(
            Arc::clone(&mount_repository),
            fs::atomic::AtomicWriteOptions {
                fsync: config.write_fsync,
                dir_sync: config.write_dir_sync,
            },
        )),
        token_repository,
        mount_repository,
        audit_service,
    };

    let router = app_router(&state);
    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "memod listening on {}", config.bind_addr);

    serve(listener, router, shutdown_signal()).await?;
    state.token_repository.pool().close().await;
    Ok(())
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

fn init_logging(config: &AppConfig) -> Result<(), ApiError> {
    if let Some(parent) = config.log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ApiError::Internal(error.to_string()))?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_path)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    let _ = writeln!(
        file,
        "{{\"level\":\"{}\",\"message\":\"memod startup\",\"ts\":\"{}\"}}",
        config.log_level,
        time::OffsetDateTime::now_utc()
    );
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let Ok(mut sigterm) = signal(SignalKind::terminate()) else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[derive(Debug)]
struct PidFileGuard {
    path: PathBuf,
}

impl PidFileGuard {
    fn create(path: &StdPath) -> Result<Self, std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{}\n", std::process::id()))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn default_config_path() -> PathBuf {
    xdg_config_home().join("memo/config.toml")
}

fn default_pid_file_path() -> PathBuf {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").map_or_else(|_| std::env::temp_dir(), PathBuf::from);
    runtime_dir.join("memo/memod.pid")
}

fn default_bootstrap_token_path() -> PathBuf {
    xdg_config_home().join("memo/bootstrap.token")
}

fn xdg_config_home() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME").map_or_else(|_| home_dir().join(".config"), PathBuf::from)
}

fn xdg_data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME").map_or_else(|_| home_dir().join(".local/share"), PathBuf::from)
}

fn xdg_state_home() -> PathBuf {
    std::env::var("XDG_STATE_HOME").map_or_else(|_| home_dir().join(".local/state"), PathBuf::from)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from)
}

fn ensure_xdg_dirs() -> Result<(), std::io::Error> {
    std::fs::create_dir_all(xdg_config_home().join("memo"))?;
    std::fs::create_dir_all(xdg_data_home().join("memo"))?;
    std::fs::create_dir_all(xdg_state_home().join("memo"))?;
    Ok(())
}

fn parse_memod_config(body: &str) -> MemodFileConfig {
    let mut cfg = MemodFileConfig::default();
    let mut daemon = DaemonConfig::default();
    let mut write = DaemonWriteConfig::default();
    let mut limits = DaemonLimits::default();
    let mut section = String::new();

    for raw in body.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            line[1..line.len() - 1].trim().clone_into(&mut section);
            continue;
        }

        let Some((key, value_raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value_raw.trim().trim_matches('"').to_owned();

        match (section.as_str(), key) {
            ("", "db_path") => cfg.db_path = Some(PathBuf::from(value)),
            ("", "log_path") => cfg.log_path = Some(PathBuf::from(value)),
            ("daemon", "bind_addr") => daemon.bind_addr = Some(value),
            ("daemon", "log_level") => daemon.log_level = Some(value),
            ("daemon.write", "fsync") => {
                if let Ok(parsed) = value.parse::<bool>() {
                    write.fsync = Some(parsed);
                }
            }
            ("daemon.write", "dir_sync") => {
                if let Ok(parsed) = value.parse::<bool>() {
                    write.dir_sync = Some(parsed);
                }
            }
            ("daemon.limits", "max_audit_log_rows") => {
                if let Ok(parsed) = value.parse::<usize>() {
                    limits.max_audit_log_rows = Some(parsed);
                }
            }
            _ => {}
        }
    }

    daemon.write = Some(write);
    daemon.limits = Some(limits);
    cfg.daemon = Some(daemon);
    cfg
}

fn app_router(state: &AppState) -> Router {
    let auth_state = AuthState {
        token_repository: Arc::clone(&state.token_repository),
        audit_service: Arc::clone(&state.audit_service),
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
                audit_service: Arc::clone(&state.audit_service),
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
                audit_service: Arc::clone(&state.audit_service),
            },
            auth_middleware,
        ))
        .with_state(state.clone());

    let audit_routes = Router::new()
        .route("/v1/meta/audit", get(query_audit))
        .layer(middleware::from_fn_with_state(
            AuthState {
                token_repository: Arc::clone(&state.token_repository),
                audit_service: Arc::clone(&state.audit_service),
            },
            auth_middleware,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/health", get(health))
        .merge(token_routes)
        .merge(mount_routes)
        .merge(audit_routes)
        .merge(fs_routes)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn query_audit(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<AuditResponse>, HttpError> {
    if !require_audit_read_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "audit_query").await);
    }

    let entries = state
        .audit_service
        .query(&query)
        .await
        .map_err(|error| HttpError(ApiError::Internal(error.to_string())))?;
    Ok(Json(AuditResponse { entries }))
}

fn require_audit_read_scope(token: &memo_core::Token) -> bool {
    token.has_scope(&Scope::Meta(MetaScope::Read))
        || token.has_scope(&Scope::Admin(memo_core::AdminScope::All))
}

async fn deny_scope(
    state: &AppState,
    token_id: Option<TokenId>,
    mount: Option<memo_core::MountName>,
    operation: &str,
) -> HttpError {
    let _ = state
        .audit_service
        .append_record(AuditRecord::error(
            operation,
            token_id,
            mount.clone(),
            None,
            "missing_scope",
        ))
        .await;
    let _ = state
        .audit_service
        .append_domain_event(DomainEvent::AccessDenied {
            token_id,
            reason: DenialReason::MissingScope,
            mount,
        })
        .await;
    HttpError(ApiError::PermissionDenied)
}

async fn record_domain_event(state: &AppState, event: DomainEvent) {
    if let Err(error) = state.audit_service.append_domain_event(event).await {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "failed to persist domain event: {error}");
    }
}

async fn record_operation_ok(
    state: &AppState,
    operation: &str,
    token_id: Option<TokenId>,
    mount: Option<memo_core::MountName>,
    path: Option<String>,
) {
    if let Err(error) = state
        .audit_service
        .append_record(AuditRecord::ok(operation, token_id, mount, path))
        .await
    {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "failed to persist audit operation: {error}");
    }
}

async fn list_tokens(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
) -> Result<Json<auth::TokenListResponse>, HttpError> {
    if !auth::require_token_list_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "token_list").await);
    }

    let tokens = state.token_repository.list().await?;
    record_operation_ok(&state, "token_list", Some(token.id), None, None).await;
    Ok(Json(auth::TokenListResponse { tokens }))
}

async fn create_token(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Json(payload): Json<auth::CreateTokenRequest>,
) -> Result<Json<auth::CreatedTokenResponse>, HttpError> {
    if !auth::require_token_admin_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "token_create").await);
    }

    let created = auth::create_token(&state.token_repository, payload).await?;
    record_domain_event(
        &state,
        DomainEvent::TokenCreated {
            id: created.id,
            name: created.name.clone(),
        },
    )
    .await;
    Ok(Json(created))
}

async fn revoke_token(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(id): Path<String>,
) -> Result<Json<auth::RevokeTokenResponse>, HttpError> {
    if !auth::require_token_admin_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "token_revoke").await);
    }

    let parsed = Uuid::from_str(&id)
        .map_err(|_| HttpError(ApiError::InvalidPath("invalid token id".to_owned())))?;
    let token_id = TokenId::from_uuid(parsed);

    state.token_repository.delete(&token_id).await?;
    record_domain_event(&state, DomainEvent::TokenRevoked { id: token_id }).await;

    Ok(Json(auth::RevokeTokenResponse {
        id: token_id,
        revoked: true,
    }))
}

async fn list_mounts(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
) -> Result<Json<mount_registry::MountListResponse>, HttpError> {
    if !mount_registry::require_mount_read_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "mount_list").await);
    }

    let mounts = state.mount_repository.list().await?;
    record_operation_ok(&state, "mount_list", Some(token.id), None, None).await;
    Ok(Json(mount_registry::MountListResponse { mounts }))
}

async fn create_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Json(payload): Json<mount_registry::CreateMountRequest>,
) -> Result<Json<memo_core::Mount>, HttpError> {
    if !mount_registry::require_mount_admin_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "mount_create").await);
    }

    let created = mount_registry::create_mount(&state.mount_repository, payload)
        .await
        .map_err(|error| match error {
            DbError::Conflict => HttpError(ApiError::Conflict("mount already exists".to_owned())),
            other => HttpError(other.into()),
        })?;
    record_domain_event(
        &state,
        DomainEvent::MountRegistered {
            name: created.name.clone(),
            mode: created.mode.clone(),
        },
    )
    .await;

    Ok(Json(created))
}

async fn get_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(name): Path<String>,
) -> Result<Json<memo_core::Mount>, HttpError> {
    if !mount_registry::require_mount_read_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "mount_show").await);
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
        return Err(deny_scope(&state, Some(token.id), None, "mount_update").await);
    }

    let mount_name = mount_registry::parse_mount_name(&name).map_err(HttpError)?;
    let updated = mount_registry::update_mount(&state.mount_repository, &mount_name, payload)
        .await
        .map_err(|error| map_mount_not_found(&mount_name, error))?;
    record_domain_event(
        &state,
        DomainEvent::MountUpdated {
            name: updated.name.clone(),
        },
    )
    .await;

    Ok(Json(updated))
}

async fn remove_mount(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Path(name): Path<String>,
) -> Result<Json<mount_registry::RemoveMountResponse>, HttpError> {
    if !mount_registry::require_mount_admin_scope(&token) {
        return Err(deny_scope(&state, Some(token.id), None, "mount_remove").await);
    }

    let mount_name = mount_registry::parse_mount_name(&name).map_err(HttpError)?;
    state
        .mount_repository
        .delete(&mount_name)
        .await
        .map_err(|error| map_mount_not_found(&mount_name, error))?;
    record_domain_event(
        &state,
        DomainEvent::MountRemoved {
            name: mount_name.clone(),
        },
    )
    .await;

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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_ls",
        )
        .await);
    }

    let response = state
        .file_system_service
        .ls(&query.path, query.info.unwrap_or(false))
        .await
        .map_err(HttpError)?;
    record_domain_event(
        &state,
        DomainEvent::DirListed {
            token_id: token.id,
            mount: query.path.mount().clone(),
            path: query.path.relative().clone(),
        },
    )
    .await;
    Ok(Json(response))
}

async fn fs_tree(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsTreeQuery>,
) -> Result<Json<fs::ops::TreeResponse>, HttpError> {
    if !fs::ops::require_fs_read_scope(&token, query.path.mount()) {
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_tree",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_stat",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_read",
        )
        .await);
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
    record_domain_event(
        &state,
        DomainEvent::FileRead {
            token_id: token.id,
            mount: query.path.mount().clone(),
            path: query.path.relative().clone(),
            bytes: size,
        },
    )
    .await;
    Ok(response)
}

async fn fs_write(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
    body: Body,
) -> Result<Json<fs::ops::WriteResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.path.mount()) {
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_write",
        )
        .await);
    }

    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| HttpError(ApiError::Internal(error.to_string())))?;

    let response = state
        .file_system_service
        .write_file(&query.path, &bytes)
        .await
        .map_err(HttpError)?;
    record_domain_event(
        &state,
        DomainEvent::FileWritten {
            token_id: token.id,
            mount: query.path.mount().clone(),
            path: query.path.relative().clone(),
            bytes: response.written_bytes,
        },
    )
    .await;
    Ok(Json(response))
}

async fn fs_mkdir(
    State(state): State<AppState>,
    Extension(VerifiedToken(token)): Extension<VerifiedToken>,
    Query(query): Query<FsPathQuery>,
) -> Result<Json<fs::ops::MkdirResponse>, HttpError> {
    if !fs::ops::require_fs_write_scope(&token, query.path.mount()) {
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_mkdir",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.src.mount().clone()),
            "fs_mv",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_rm",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.src.mount().clone()),
            "fs_cp",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_grep",
        )
        .await);
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
        return Err(deny_scope(
            &state,
            Some(token.id),
            Some(query.path.mount().clone()),
            "fs_find",
        )
        .await);
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

    use memo_client::{CreateMountRequest, MemoClient, MemoClientConfig, MemoClientError};
    use memo_core::{ApiError, Audience, MountMode, MountName, MountPath};
    use tempfile::tempdir;

    use super::app_router;
    use crate::audit::AuditService;
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
        let audit_service =
            std::sync::Arc::new(AuditService::new(tempdir.path().join("audit.log"))?);
        let state = super::AppState {
            file_system_service: std::sync::Arc::new(crate::fs::ops::FileSystemService::new(
                std::sync::Arc::clone(&mount_repository),
                crate::fs::atomic::AtomicWriteOptions::default(),
            )),
            token_repository,
            mount_repository,
            audit_service,
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

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn filesystem_endpoints_round_trip() -> Result<(), Box<dyn Error>> {
        let tempdir = tempdir()?;
        let db_path = tempdir.path().join("memo.db");
        let db_url = format!("sqlite://{}", db_path.display());
        let bootstrap_path = tempdir.path().join("bootstrap.token");
        let mount_root = tempdir.path().join("vault");
        let archive_root = tempdir.path().join("archive");
        std::fs::create_dir_all(&mount_root)?;
        std::fs::create_dir_all(&archive_root)?;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        let pool = init_pool(&DbConfig::new(db_url.clone())).await?;
        let token_repository = std::sync::Arc::new(SqliteTokenRepository::new(pool.clone()));
        let mount_repository =
            std::sync::Arc::new(SqliteMountRepository::new(pool, PolicyCache::new()));
        let audit_service =
            std::sync::Arc::new(AuditService::new(tempdir.path().join("audit.log"))?);
        let state = super::AppState {
            file_system_service: std::sync::Arc::new(crate::fs::ops::FileSystemService::new(
                std::sync::Arc::clone(&mount_repository),
                crate::fs::atomic::AtomicWriteOptions::default(),
            )),
            token_repository,
            mount_repository,
            audit_service,
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

        let _ = client
            .create_mount(&CreateMountRequest {
                name: MountName::new("VaultKB")?,
                root_path: mount_root.clone(),
                mode: MountMode::ReadWrite,
                audience: Audience::Shared,
                description: Some("kb".to_owned()),
                hide_globs: vec![],
                deny_read_globs: vec![],
                deny_write_globs: vec![],
                max_read_bytes: None,
                max_write_bytes: None,
            })
            .await?;
        let _ = client
            .create_mount(&CreateMountRequest {
                name: MountName::new("Archive")?,
                root_path: archive_root.clone(),
                mode: MountMode::ReadWrite,
                audience: Audience::Shared,
                description: None,
                hide_globs: vec![],
                deny_read_globs: vec![],
                deny_write_globs: vec![],
                max_read_bytes: None,
                max_write_bytes: None,
            })
            .await?;

        let notes = MountPath::parse("VaultKB:/notes")?;
        let _ = client.mkdir(&notes).await?;
        let file = MountPath::parse("VaultKB:/notes/rust.md")?;
        let content = b"---\nsummary: rust note\n---\nrust async patterns\n";
        let _ = client.write_bytes(&file, content.to_vec()).await?;
        let index = MountPath::parse("VaultKB:/notes/index.md")?;
        let _ = client
            .write_bytes(&index, b"---\nsummary: rust note\n---\nindex\n".to_vec())
            .await?;

        let listed = client.ls(&notes, Some(true)).await?;
        assert!(listed.entries.iter().any(|entry| entry.name == "rust.md"));
        assert!(listed.entries.iter().any(|entry| entry.name == "index.md"));
        let tree = client
            .tree(&MountPath::parse("VaultKB:/")?, Some(4))
            .await?;
        assert!(!tree.tree.children.is_empty());

        let stat = client.stat(&file).await?;
        assert_eq!(stat.memo_summary, None);
        let index_stat = client.stat(&index).await?;
        assert_eq!(index_stat.memo_summary.as_deref(), Some("rust note"));

        let raw = client.read(&file).await?;
        assert_eq!(raw, content);

        let moved = MountPath::parse("VaultKB:/notes/rust-moved.md")?;
        let _ = client.mv(&file, &moved).await?;

        let copied = MountPath::parse("Archive:/copies/rust-copy.md")?;
        let _ = client.cp(&moved, &copied).await?;

        let grep = client
            .grep(
                &MountPath::parse("VaultKB:/")?,
                "rust",
                Some(true),
                Some(true),
                Some(10),
            )
            .await?;
        assert!(!grep.matches.is_empty());

        let found = client
            .find(&MountPath::parse("Archive:/")?, "*.md", Some(10))
            .await?;
        assert!(!found.results.is_empty());

        let non_empty = MountPath::parse("VaultKB:/dir")?;
        let child = MountPath::parse("VaultKB:/dir/a.txt")?;
        let _ = client.mkdir(&non_empty).await?;
        let _ = client.write_bytes(&child, b"x".to_vec()).await?;
        let conflict = client.rm(&non_empty, Some(false)).await;
        assert!(matches!(
            conflict,
            Err(MemoClientError::Api(ApiError::Conflict(_)))
        ));
        let _ = client.rm(&non_empty, Some(true)).await?;

        let big = vec![b'a'; 5 * 1024 * 1024];
        let big_path = MountPath::parse("VaultKB:/notes/big.bin")?;
        let _ = client.write_bytes(&big_path, big.clone()).await?;
        let stream_response = client.read_response(&big_path).await?;
        let streamed = stream_response.bytes().await?;
        assert_eq!(streamed.len(), big.len());

        let _ = client.rm(&moved, Some(false)).await?;
        let _ = client.rm(&copied, Some(false)).await?;
        let _ = client.rm(&notes, Some(true)).await?;

        let _ = shutdown_tx.send(());
        handle.await??;

        Ok(())
    }
}
