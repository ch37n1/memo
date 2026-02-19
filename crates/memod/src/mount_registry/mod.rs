pub mod policy;
pub mod repository;

use memo_core::repositories::MountRepository;
use memo_core::{
    AdminScope, ApiError, Audience, DbError, MetaScope, Mount, MountMode, MountName, MountPolicy,
    Scope, Token,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use time::OffsetDateTime;

use crate::mount_registry::repository::SqliteMountRepository;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateMountRequest {
    pub name: MountName,
    pub root_path: PathBuf,
    pub mode: MountMode,
    pub audience: Audience,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hide_globs: Vec<String>,
    #[serde(default)]
    pub deny_read_globs: Vec<String>,
    #[serde(default)]
    pub deny_write_globs: Vec<String>,
    #[serde(default)]
    pub max_read_bytes: Option<u64>,
    #[serde(default)]
    pub max_write_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateMountRequest {
    #[serde(default)]
    pub mode: Option<MountMode>,
    #[serde(default)]
    pub audience: Option<Audience>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hide_globs: Option<Vec<String>>,
    #[serde(default)]
    pub deny_read_globs: Option<Vec<String>>,
    #[serde(default)]
    pub deny_write_globs: Option<Vec<String>>,
    #[serde(default)]
    pub max_read_bytes: Option<u64>,
    #[serde(default)]
    pub max_write_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct MountListResponse {
    pub mounts: Vec<Mount>,
}

#[derive(Debug, Serialize)]
pub struct RemoveMountResponse {
    pub name: MountName,
    pub removed: bool,
}

pub fn require_mount_admin_scope(token: &Token) -> bool {
    token.has_scope(&Scope::Admin(AdminScope::Mounts))
        || token.has_scope(&Scope::Admin(AdminScope::All))
}

pub fn require_mount_read_scope(token: &Token) -> bool {
    require_mount_admin_scope(token) || token.has_scope(&Scope::Meta(MetaScope::Read))
}

/// Creates a new mount.
///
/// # Errors
///
/// Returns [`DbError`] when validation or persistence fails.
pub async fn create_mount(
    repository: &SqliteMountRepository,
    request: CreateMountRequest,
) -> Result<Mount, DbError> {
    let now = OffsetDateTime::now_utc();
    let mount = Mount::new(
        request.name,
        request.root_path,
        request.mode,
        request.audience,
        request.description,
        MountPolicy {
            hide_globs: request.hide_globs,
            deny_read_globs: request.deny_read_globs,
            deny_write_globs: request.deny_write_globs,
            max_read_bytes: request.max_read_bytes,
            max_write_bytes: request.max_write_bytes,
        },
        now,
    )
    .map_err(|error| DbError::Query(error.to_string()))?;

    repository.create(&mount).await?;
    Ok(mount)
}

/// Updates an existing mount by patching provided fields.
///
/// # Errors
///
/// Returns [`DbError`] when lookup fails or persistence fails.
pub async fn update_mount(
    repository: &SqliteMountRepository,
    name: &MountName,
    request: UpdateMountRequest,
) -> Result<Mount, DbError> {
    let current = repository.find(name).await?;
    let now = OffsetDateTime::now_utc();
    let updated = Mount {
        name: current.name.clone(),
        root_path: current.root_path.clone(),
        mode: request.mode.unwrap_or(current.mode),
        audience: request.audience.unwrap_or(current.audience),
        description: request.description.or(current.description),
        policy: MountPolicy {
            hide_globs: request.hide_globs.unwrap_or(current.policy.hide_globs),
            deny_read_globs: request
                .deny_read_globs
                .unwrap_or(current.policy.deny_read_globs),
            deny_write_globs: request
                .deny_write_globs
                .unwrap_or(current.policy.deny_write_globs),
            max_read_bytes: request.max_read_bytes.or(current.policy.max_read_bytes),
            max_write_bytes: request.max_write_bytes.or(current.policy.max_write_bytes),
        },
        created_at: current.created_at,
        updated_at: now,
    };

    repository.update(&updated).await?;
    Ok(updated)
}

/// Parses and validates mount name from route segment.
///
/// # Errors
///
/// Returns [`ApiError::InvalidPath`] when name is invalid.
pub fn parse_mount_name(input: &str) -> Result<MountName, ApiError> {
    MountName::new(input).map_err(|_| ApiError::InvalidPath("invalid mount name".to_owned()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;

    use memo_core::{
        AdminScope, Audience, Mount, MountMode, MountName, MountPolicy, Scope, ScopeSet, Token,
        TokenId,
    };
    use tempfile::tempdir;
    use time::OffsetDateTime;

    use crate::db::{init_pool, DbConfig};
    use crate::mount_registry::repository::{PolicyCache, SqliteMountRepository};

    #[test]
    fn parse_mount_name_rejects_invalid_chars() {
        let parsed = super::parse_mount_name("bad name");
        assert!(matches!(parsed, Err(memo_core::ApiError::InvalidPath(_))));
    }

    #[test]
    fn mount_scope_helpers_match_expected_scopes() -> Result<(), Box<dyn std::error::Error>> {
        let now = OffsetDateTime::now_utc();
        let admin_token = Token::new(
            TokenId::new(),
            "admin",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new([Scope::from_str("admin:*:mounts")?]),
            now,
            memo_core::Expiry::Never,
        )?;
        let meta_reader = Token::new(
            TokenId::new(),
            "reader",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new([Scope::from_str("meta:*:read")?]),
            now,
            memo_core::Expiry::Never,
        )?;
        let token_admin_all = Token::new(
            TokenId::new(),
            "super-admin",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new([Scope::Admin(AdminScope::All)]),
            now,
            memo_core::Expiry::Never,
        )?;

        assert!(super::require_mount_admin_scope(&admin_token));
        assert!(super::require_mount_read_scope(&admin_token));
        assert!(!super::require_mount_admin_scope(&meta_reader));
        assert!(super::require_mount_read_scope(&meta_reader));
        assert!(super::require_mount_admin_scope(&token_admin_all));
        assert!(super::require_mount_read_scope(&token_admin_all));
        Ok(())
    }

    #[tokio::test]
    async fn update_mount_preserves_unspecified_fields() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteMountRepository::new(pool, PolicyCache::new());

        let now = OffsetDateTime::now_utc();
        let mount = Mount::new(
            MountName::new("VaultKB")?,
            PathBuf::from("/tmp/vault"),
            MountMode::ReadWrite,
            Audience::Shared,
            Some("original".to_owned()),
            MountPolicy {
                hide_globs: vec!["hidden/*".to_owned()],
                deny_read_globs: vec!["private/*".to_owned()],
                deny_write_globs: vec!["blocked/*".to_owned()],
                max_read_bytes: Some(100),
                max_write_bytes: Some(50),
            },
            now,
        )?;
        repository.create(&mount).await?;

        let updated = super::update_mount(
            &repository,
            &mount.name,
            super::UpdateMountRequest {
                mode: Some(MountMode::ReadOnly),
                description: Some("updated".to_owned()),
                ..super::UpdateMountRequest::default()
            },
        )
        .await?;

        assert_eq!(updated.mode, MountMode::ReadOnly);
        assert_eq!(updated.description.as_deref(), Some("updated"));
        assert_eq!(updated.audience, Audience::Shared);
        assert_eq!(updated.policy.hide_globs, vec!["hidden/*".to_owned()]);
        assert_eq!(updated.policy.deny_read_globs, vec!["private/*".to_owned()]);
        assert_eq!(
            updated.policy.deny_write_globs,
            vec!["blocked/*".to_owned()]
        );
        assert_eq!(updated.policy.max_read_bytes, Some(100));
        assert_eq!(updated.policy.max_write_bytes, Some(50));
        Ok(())
    }

    #[tokio::test]
    async fn update_mount_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteMountRepository::new(pool, PolicyCache::new());

        let result = super::update_mount(
            &repository,
            &MountName::new("Missing")?,
            super::UpdateMountRequest {
                mode: Some(MountMode::ReadOnly),
                ..super::UpdateMountRequest::default()
            },
        )
        .await;

        assert!(matches!(result, Err(memo_core::DbError::NotFound)));
        Ok(())
    }
}
