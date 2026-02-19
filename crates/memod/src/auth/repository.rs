use std::str::FromStr;

use memo_core::repositories::TokenRepository;
use memo_core::{
    AdminScope, AuthError, DbError, Expiry, FsAction, MetaScope, Scope, ScopeMount, ScopeSet,
    Token, TokenId, TokenView,
};
use sqlx::SqlitePool;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqliteTokenRepository {
    pool: SqlitePool,
}

impl SqliteTokenRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Returns current token row count.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Query`] when the count query fails.
    pub async fn count(&self) -> Result<i64, DbError> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM tokens")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| DbError::Query(error.to_string()))
    }

    fn parse_scopes(value: &str) -> Result<ScopeSet, DbError> {
        let raw_scopes: Vec<String> =
            serde_json::from_str(value).map_err(|error| DbError::Query(error.to_string()))?;

        let mut scopes = ScopeSet::default();
        for raw in raw_scopes {
            let scope = Scope::from_str(&raw).map_err(|error| DbError::Query(error.to_string()))?;
            scopes.insert(scope);
        }

        Ok(scopes)
    }

    fn serialize_scopes(scopes: &ScopeSet) -> Result<String, DbError> {
        let raw_scopes: Vec<String> = scopes.iter().map(scope_to_db_string).collect();
        serde_json::to_string(&raw_scopes).map_err(|error| DbError::Query(error.to_string()))
    }

    fn parse_datetime(value: &str) -> Result<OffsetDateTime, DbError> {
        if let Ok(parsed) = OffsetDateTime::parse(value, &Rfc3339) {
            return Ok(parsed);
        }

        let with_utc = format!("{}Z", value.replace(' ', "T"));
        OffsetDateTime::parse(&with_utc, &Rfc3339)
            .map_err(|error| DbError::Query(format!("invalid datetime {value}: {error}")))
    }

    fn serialize_datetime(value: OffsetDateTime) -> Result<String, DbError> {
        value
            .format(&Rfc3339)
            .map_err(|error| DbError::Query(error.to_string()))
    }

    fn map_row_to_token(row: TokenRow) -> Result<Token, DbError> {
        let id = Uuid::parse_str(&row.id).map_err(|error| DbError::Query(error.to_string()))?;
        let created_at = Self::parse_datetime(&row.created_at)?;
        let expires_at = row
            .expires_at
            .map(|value| Self::parse_datetime(&value))
            .transpose()?
            .map_or(Expiry::Never, Expiry::At);
        let last_used_at = row
            .last_used_at
            .map(|value| Self::parse_datetime(&value))
            .transpose()?;

        let scopes = Self::parse_scopes(&row.scopes)?;

        Ok(Token {
            id: TokenId::from_uuid(id),
            name: row.name,
            hash: row.hash,
            scopes,
            created_at,
            expires_at,
            last_used_at,
        })
    }
}

fn scope_to_db_string(scope: &Scope) -> String {
    match scope {
        Scope::Fs { mount, action } => format!(
            "fs:{}:{}",
            scope_mount_to_str(mount),
            fs_action_to_str(action)
        ),
        Scope::Meta(scope) => format!("meta:*:{}", meta_scope_to_str(scope)),
        Scope::Admin(scope) => format!("admin:*:{}", admin_scope_to_str(scope)),
    }
}

fn scope_mount_to_str(scope: &ScopeMount) -> String {
    match scope {
        ScopeMount::Any => "*".to_owned(),
        ScopeMount::Named(name) => name.to_string(),
    }
}

fn fs_action_to_str(action: &FsAction) -> &'static str {
    match action {
        FsAction::Read => "read",
        FsAction::Write => "write",
    }
}

fn meta_scope_to_str(scope: &MetaScope) -> &'static str {
    match scope {
        MetaScope::Read => "read",
    }
}

fn admin_scope_to_str(scope: &AdminScope) -> &'static str {
    match scope {
        AdminScope::Mounts => "mounts",
        AdminScope::Tokens => "tokens",
        AdminScope::All => "*",
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    id: String,
    name: String,
    hash: String,
    scopes: String,
    created_at: String,
    expires_at: Option<String>,
    last_used_at: Option<String>,
}

#[async_trait::async_trait]
impl TokenRepository for SqliteTokenRepository {
    async fn find(&self, id: &TokenId) -> Result<Token, DbError> {
        let row = sqlx::query_as::<_, TokenRow>(
            "SELECT id, name, hash, scopes, created_at, expires_at, last_used_at FROM tokens WHERE id = ?1",
        )
        .bind(id.into_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?
        .ok_or(DbError::NotFound)?;

        Self::map_row_to_token(row)
    }

    async fn verify(&self, raw_token: &str) -> Result<Token, AuthError> {
        let rows = sqlx::query_as::<_, TokenRow>(
            "SELECT id, name, hash, scopes, created_at, expires_at, last_used_at FROM tokens",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|_| AuthError::TokenInvalid)?;

        for row in rows {
            let token = Self::map_row_to_token(row).map_err(|_| AuthError::TokenInvalid)?;

            let raw = raw_token.to_owned();
            let hash = token.hash.clone();
            let verified =
                tokio::task::spawn_blocking(move || super::verify_token_hash(&raw, &hash))
                    .await
                    .map_err(|_| AuthError::TokenInvalid)?;

            if !verified {
                continue;
            }

            if token.is_expired_at(OffsetDateTime::now_utc()) {
                return Err(AuthError::TokenExpired);
            }

            let _ = self.touch_last_used(&token.id).await;
            return Ok(token);
        }

        Err(AuthError::TokenInvalid)
    }

    async fn list(&self) -> Result<Vec<TokenView>, DbError> {
        let rows = sqlx::query_as::<_, TokenRow>(
            "SELECT id, name, hash, scopes, created_at, expires_at, last_used_at FROM tokens ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        rows.into_iter()
            .map(|row| Self::map_row_to_token(row).map(|token| TokenView::from(&token)))
            .collect()
    }

    async fn save(&self, token: &Token) -> Result<(), DbError> {
        let scopes = Self::serialize_scopes(&token.scopes)?;
        let created_at = Self::serialize_datetime(token.created_at)?;
        let expires_at = match token.expires_at {
            Expiry::Never => None,
            Expiry::At(value) => Some(Self::serialize_datetime(value)?),
        };
        let last_used_at = token
            .last_used_at
            .map(Self::serialize_datetime)
            .transpose()?;

        sqlx::query(
            "INSERT INTO tokens (id, name, hash, scopes, created_at, expires_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(token.id.into_uuid().to_string())
        .bind(&token.name)
        .bind(&token.hash)
        .bind(scopes)
        .bind(created_at)
        .bind(expires_at)
        .bind(last_used_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: &TokenId) -> Result<(), DbError> {
        let result = sqlx::query("DELETE FROM tokens WHERE id = ?1")
            .bind(id.into_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|error| DbError::Query(error.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }

        Ok(())
    }

    async fn touch_last_used(&self, id: &TokenId) -> Result<(), DbError> {
        let now = Self::serialize_datetime(OffsetDateTime::now_utc())?;

        sqlx::query("UPDATE tokens SET last_used_at = ?1 WHERE id = ?2")
            .bind(now)
            .bind(id.into_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|error| DbError::Query(error.to_string()))?;

        Ok(())
    }
}
