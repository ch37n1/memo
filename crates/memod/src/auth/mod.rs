pub mod middleware;
pub mod repository;

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::str::FromStr;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use memo_core::repositories::TokenRepository;
use memo_core::{AdminScope, DbError, Expiry, MetaScope, Scope, ScopeSet, Token, TokenId};
use rand::distr::Alphanumeric;
use rand::Rng;
use time::OffsetDateTime;

use crate::auth::repository::SqliteTokenRepository;

#[derive(Debug, serde::Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub scopes: ScopeSet,
    #[serde(default)]
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, serde::Serialize)]
pub struct CreatedTokenResponse {
    pub id: TokenId,
    pub name: String,
    pub token: String,
    pub scopes: ScopeSet,
    pub created_at: OffsetDateTime,
    pub expires_at: Expiry,
}

#[derive(Debug, serde::Serialize)]
pub struct TokenListResponse {
    pub tokens: Vec<memo_core::TokenView>,
}

#[derive(Debug, serde::Serialize)]
pub struct RevokeTokenResponse {
    pub id: TokenId,
    pub revoked: bool,
}

pub fn generate_token_value() -> String {
    let mut rng = rand::rng();
    let suffix: String = (&mut rng)
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    format!("memo_{suffix}")
}

fn argon2id() -> Result<Argon2<'static>, DbError> {
    let params =
        Params::new(19_456, 2, 1, None).map_err(|error| DbError::Query(error.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hashes a raw token value using Argon2id.
///
/// # Errors
///
/// Returns [`DbError::Query`] when Argon2 parameters or hashing fail.
pub fn hash_token(raw_token: &str) -> Result<String, DbError> {
    let mut salt_bytes = [0_u8; 16];
    rand::rng().fill(&mut salt_bytes);
    let salt =
        SaltString::encode_b64(&salt_bytes).map_err(|error| DbError::Query(error.to_string()))?;
    let argon2 = argon2id()?;

    argon2
        .hash_password(raw_token.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| DbError::Query(error.to_string()))
}

pub fn verify_token_hash(raw_token: &str, hashed_token: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hashed_token) else {
        return false;
    };
    let Ok(argon2) = argon2id() else {
        return false;
    };

    argon2
        .verify_password(raw_token.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn token_has_any_scope(token: &Token, required: &[Scope]) -> bool {
    required.iter().any(|scope| token.has_scope(scope))
}

/// Creates and persists a new token and returns the raw token once.
///
/// # Errors
///
/// Returns [`DbError`] when hashing, validation, or token persistence fails.
pub async fn create_token(
    repository: &SqliteTokenRepository,
    request: CreateTokenRequest,
) -> Result<CreatedTokenResponse, DbError> {
    let raw_token = generate_token_value();
    let hash = hash_token(&raw_token)?;
    let now = OffsetDateTime::now_utc();
    let expires_at = request.expires_at.map_or(Expiry::Never, Expiry::At);

    let token = Token::new(
        TokenId::new(),
        request.name,
        hash,
        request.scopes,
        now,
        expires_at.clone(),
    )
    .map_err(|error| DbError::Query(error.to_string()))?;

    repository.save(&token).await?;

    Ok(CreatedTokenResponse {
        id: token.id,
        name: token.name,
        token: raw_token,
        scopes: token.scopes,
        created_at: token.created_at,
        expires_at,
    })
}

/// Creates a bootstrap admin token and writes the raw token to a file when the
/// token store is empty.
///
/// # Errors
///
/// Returns [`DbError`] if counting or creating tokens fails, or if writing the
/// bootstrap token file fails.
pub async fn bootstrap_admin_token_if_needed(
    repository: &SqliteTokenRepository,
    bootstrap_token_path: &Path,
) -> Result<Option<String>, DbError> {
    if repository.count().await? > 0 {
        return Ok(None);
    }

    let mut scopes = ScopeSet::default();
    scopes.insert(Scope::from_str("admin:*:*").map_err(|error| DbError::Query(error.to_string()))?);
    scopes.insert(
        Scope::from_str("admin:*:mounts").map_err(|error| DbError::Query(error.to_string()))?,
    );
    scopes.insert(
        Scope::from_str("admin:*:tokens").map_err(|error| DbError::Query(error.to_string()))?,
    );
    scopes
        .insert(Scope::from_str("meta:*:read").map_err(|error| DbError::Query(error.to_string()))?);
    scopes.insert(Scope::from_str("fs:*:read").map_err(|error| DbError::Query(error.to_string()))?);
    scopes
        .insert(Scope::from_str("fs:*:write").map_err(|error| DbError::Query(error.to_string()))?);

    let created = create_token(
        repository,
        CreateTokenRequest {
            name: "bootstrap-admin".to_owned(),
            scopes,
            expires_at: None,
        },
    )
    .await?;

    if let Some(parent) = bootstrap_token_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| DbError::Connection(error.to_string()))?;
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(bootstrap_token_path)
        .map_err(|error| DbError::Connection(error.to_string()))?;
    writeln!(file, "{}", created.token).map_err(|error| DbError::Connection(error.to_string()))?;

    Ok(Some(created.token))
}

pub fn require_token_admin_scope(token: &Token) -> bool {
    token_has_any_scope(
        token,
        &[
            Scope::Admin(AdminScope::Tokens),
            Scope::Admin(AdminScope::All),
        ],
    )
}

pub fn require_token_list_scope(token: &Token) -> bool {
    token_has_any_scope(
        token,
        &[
            Scope::Admin(AdminScope::Tokens),
            Scope::Admin(AdminScope::All),
            Scope::Meta(MetaScope::Read),
        ],
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use memo_core::{Expiry, Scope};
    use tempfile::tempdir;
    use time::{Duration, OffsetDateTime};

    use crate::auth::repository::SqliteTokenRepository;
    use crate::auth::{
        bootstrap_admin_token_if_needed, create_token, hash_token, require_token_list_scope,
        verify_token_hash, CreateTokenRequest,
    };
    use crate::db::{init_pool, DbConfig};

    #[test]
    fn hash_verify_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let raw = "memo_test_token";
        let hash = hash_token(raw)?;
        assert!(verify_token_hash(raw, &hash));
        assert!(!verify_token_hash("memo_wrong_token", &hash));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_expired_token() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteTokenRepository::new(pool);

        let expired_at = OffsetDateTime::now_utc() - Duration::minutes(5);
        let created = create_token(
            &repository,
            CreateTokenRequest {
                name: "agent".to_owned(),
                scopes: memo_core::ScopeSet::new([Scope::from_str("fs:*:read")?]),
                expires_at: Some(expired_at),
            },
        )
        .await?;

        let verify = <SqliteTokenRepository as memo_core::TokenRepository>::verify(
            &repository,
            &created.token,
        )
        .await;

        assert_eq!(verify, Err(memo_core::AuthError::TokenExpired));
        Ok(())
    }

    #[tokio::test]
    async fn scope_check_allows_meta_read() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteTokenRepository::new(pool);

        let created = create_token(
            &repository,
            CreateTokenRequest {
                name: "reader".to_owned(),
                scopes: memo_core::ScopeSet::new([Scope::from_str("meta:*:read")?]),
                expires_at: None,
            },
        )
        .await?;

        let token = <SqliteTokenRepository as memo_core::TokenRepository>::verify(
            &repository,
            &created.token,
        )
        .await?;

        assert_eq!(token.expires_at, Expiry::Never);
        assert!(require_token_list_scope(&token));
        Ok(())
    }

    #[tokio::test]
    async fn bootstrap_creates_token_file() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteTokenRepository::new(pool);
        let token_path = tempdir.path().join("bootstrap.token");

        let token = bootstrap_admin_token_if_needed(&repository, &token_path).await?;
        assert!(token.is_some());
        assert!(token_path.exists());
        Ok(())
    }
}
