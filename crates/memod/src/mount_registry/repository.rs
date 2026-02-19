use std::sync::Arc;
use std::{collections::HashMap, sync::RwLock};

use memo_core::repositories::MountRepository;
use memo_core::{DbError, Mount, MountName, MountPolicy};
use sqlx::SqlitePool;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::db::{audience_from_db, audience_to_db, mount_mode_from_db, mount_mode_to_db};

#[derive(Debug, Clone)]
pub struct CompiledMount {
    pub policy: MountPolicy,
}

impl CompiledMount {
    #[must_use]
    pub fn from_policy(policy: &MountPolicy) -> Self {
        Self {
            policy: policy.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PolicyCache {
    inner: Arc<RwLock<HashMap<String, Arc<CompiledMount>>>>,
}

impl PolicyCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get_or_compile(&self, mount: &Mount) -> Arc<CompiledMount> {
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(compiled) = guard.get(mount.name.as_str()) {
            return Arc::clone(compiled);
        }
        drop(guard);

        let compiled = Arc::new(CompiledMount::from_policy(&mount.policy));
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(mount.name.as_str().to_owned(), Arc::clone(&compiled));
        compiled
    }

    pub fn invalidate_mount(&self, name: &MountName) {
        if let Ok(mut guard) = self.inner.write() {
            let _ = guard.remove(name.as_str());
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqliteMountRepository {
    pool: SqlitePool,
    policy_cache: PolicyCache,
}

impl SqliteMountRepository {
    #[must_use]
    pub fn new(pool: SqlitePool, policy_cache: PolicyCache) -> Self {
        Self { pool, policy_cache }
    }

    /// Inserts a mount and fails on duplicate name.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Conflict`] for duplicate names and [`DbError::Query`] for
    /// other database failures.
    pub async fn create(&self, mount: &Mount) -> Result<(), DbError> {
        let hide_globs = serialize_json_array(&mount.policy.hide_globs)?;
        let deny_read_globs = serialize_json_array(&mount.policy.deny_read_globs)?;
        let deny_write_globs = serialize_json_array(&mount.policy.deny_write_globs)?;
        let created_at = serialize_datetime(mount.created_at)?;
        let updated_at = serialize_datetime(mount.updated_at)?;

        let result = sqlx::query(
            "INSERT INTO mounts (
                name, root_path, mode, audience, description, hide_globs,
                deny_read_globs, deny_write_globs, max_read_bytes, max_write_bytes,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .bind(mount.name.to_string())
        .bind(mount.root_path.to_string_lossy().into_owned())
        .bind(mount_mode_to_db(&mount.mode))
        .bind(audience_to_db(&mount.audience))
        .bind(mount.description.clone())
        .bind(hide_globs)
        .bind(deny_read_globs)
        .bind(deny_write_globs)
        .bind(
            mount
                .policy
                .max_read_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(
            mount
                .policy
                .max_write_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(created_at)
        .bind(updated_at)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                self.policy_cache.invalidate_mount(&mount.name);
                Ok(())
            }
            Err(error) if is_unique_violation(&error) => Err(DbError::Conflict),
            Err(error) => Err(DbError::Query(error.to_string())),
        }
    }

    /// Updates an existing mount.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::NotFound`] when mount does not exist.
    pub async fn update(&self, mount: &Mount) -> Result<(), DbError> {
        let hide_globs = serialize_json_array(&mount.policy.hide_globs)?;
        let deny_read_globs = serialize_json_array(&mount.policy.deny_read_globs)?;
        let deny_write_globs = serialize_json_array(&mount.policy.deny_write_globs)?;
        let updated_at = serialize_datetime(mount.updated_at)?;

        let result = sqlx::query(
            "UPDATE mounts SET
                mode = ?1,
                audience = ?2,
                description = ?3,
                hide_globs = ?4,
                deny_read_globs = ?5,
                deny_write_globs = ?6,
                max_read_bytes = ?7,
                max_write_bytes = ?8,
                updated_at = ?9
             WHERE name = ?10",
        )
        .bind(mount_mode_to_db(&mount.mode))
        .bind(audience_to_db(&mount.audience))
        .bind(mount.description.clone())
        .bind(hide_globs)
        .bind(deny_read_globs)
        .bind(deny_write_globs)
        .bind(
            mount
                .policy
                .max_read_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(
            mount
                .policy
                .max_write_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(updated_at)
        .bind(mount.name.to_string())
        .execute(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }

        self.policy_cache.invalidate_mount(&mount.name);
        Ok(())
    }

    #[must_use]
    pub fn policy_cache(&self) -> &PolicyCache {
        &self.policy_cache
    }
}

#[derive(Debug, sqlx::FromRow)]
struct MountRow {
    name: String,
    root_path: String,
    mode: String,
    audience: String,
    description: Option<String>,
    hide_globs: String,
    deny_read_globs: String,
    deny_write_globs: String,
    max_read_bytes: Option<i64>,
    max_write_bytes: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[async_trait::async_trait]
impl MountRepository for SqliteMountRepository {
    async fn find(&self, name: &MountName) -> Result<Mount, DbError> {
        let row = sqlx::query_as::<_, MountRow>(
            "SELECT
                name, root_path, mode, audience, description, hide_globs,
                deny_read_globs, deny_write_globs, max_read_bytes, max_write_bytes,
                created_at, updated_at
             FROM mounts WHERE name = ?1",
        )
        .bind(name.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?
        .ok_or(DbError::NotFound)?;

        row_to_mount(row)
    }

    async fn list(&self) -> Result<Vec<Mount>, DbError> {
        let rows = sqlx::query_as::<_, MountRow>(
            "SELECT
                name, root_path, mode, audience, description, hide_globs,
                deny_read_globs, deny_write_globs, max_read_bytes, max_write_bytes,
                created_at, updated_at
             FROM mounts ORDER BY name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        rows.into_iter().map(row_to_mount).collect()
    }

    async fn save(&self, mount: &Mount) -> Result<(), DbError> {
        let hide_globs = serialize_json_array(&mount.policy.hide_globs)?;
        let deny_read_globs = serialize_json_array(&mount.policy.deny_read_globs)?;
        let deny_write_globs = serialize_json_array(&mount.policy.deny_write_globs)?;
        let created_at = serialize_datetime(mount.created_at)?;
        let updated_at = serialize_datetime(mount.updated_at)?;

        sqlx::query(
            "INSERT INTO mounts (
                name, root_path, mode, audience, description, hide_globs,
                deny_read_globs, deny_write_globs, max_read_bytes, max_write_bytes,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(name) DO UPDATE SET
                mode = excluded.mode,
                audience = excluded.audience,
                description = excluded.description,
                hide_globs = excluded.hide_globs,
                deny_read_globs = excluded.deny_read_globs,
                deny_write_globs = excluded.deny_write_globs,
                max_read_bytes = excluded.max_read_bytes,
                max_write_bytes = excluded.max_write_bytes,
                updated_at = excluded.updated_at",
        )
        .bind(mount.name.to_string())
        .bind(mount.root_path.to_string_lossy().into_owned())
        .bind(mount_mode_to_db(&mount.mode))
        .bind(audience_to_db(&mount.audience))
        .bind(mount.description.clone())
        .bind(hide_globs)
        .bind(deny_read_globs)
        .bind(deny_write_globs)
        .bind(
            mount
                .policy
                .max_read_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(
            mount
                .policy
                .max_write_bytes
                .and_then(|v| i64::try_from(v).ok()),
        )
        .bind(created_at)
        .bind(updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        self.policy_cache.invalidate_mount(&mount.name);
        Ok(())
    }

    async fn delete(&self, name: &MountName) -> Result<(), DbError> {
        let result = sqlx::query("DELETE FROM mounts WHERE name = ?1")
            .bind(name.to_string())
            .execute(&self.pool)
            .await
            .map_err(|error| DbError::Query(error.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound);
        }

        self.policy_cache.invalidate_mount(name);
        Ok(())
    }
}

fn row_to_mount(row: MountRow) -> Result<Mount, DbError> {
    let hide_globs = parse_json_array(&row.hide_globs)?;
    let deny_read_globs = parse_json_array(&row.deny_read_globs)?;
    let deny_write_globs = parse_json_array(&row.deny_write_globs)?;

    Ok(Mount {
        name: MountName::new(&row.name).map_err(|error| DbError::Query(error.to_string()))?,
        root_path: row.root_path.into(),
        mode: mount_mode_from_db(&row.mode)?,
        audience: audience_from_db(&row.audience)?,
        description: row.description,
        policy: MountPolicy {
            hide_globs,
            deny_read_globs,
            deny_write_globs,
            max_read_bytes: row.max_read_bytes.and_then(|v| u64::try_from(v).ok()),
            max_write_bytes: row.max_write_bytes.and_then(|v| u64::try_from(v).ok()),
        },
        created_at: parse_datetime(&row.created_at)?,
        updated_at: parse_datetime(&row.updated_at)?,
    })
}

fn parse_json_array(value: &str) -> Result<Vec<String>, DbError> {
    serde_json::from_str(value).map_err(|error| DbError::Query(error.to_string()))
}

fn serialize_json_array(values: &[String]) -> Result<String, DbError> {
    serde_json::to_string(values).map_err(|error| DbError::Query(error.to_string()))
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

fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(db_error) => db_error.is_unique_violation(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use memo_core::repositories::MountRepository;
    use memo_core::{Audience, Mount, MountMode, MountName, MountPolicy, RelativePath};
    use tempfile::tempdir;
    use time::OffsetDateTime;

    use crate::db::{init_pool, DbConfig};

    use super::{PolicyCache, SqliteMountRepository};

    #[tokio::test]
    async fn mount_crud_round_trip() -> Result<(), Box<dyn std::error::Error>> {
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
            Some("knowledge base".to_owned()),
            MountPolicy {
                hide_globs: vec!["**/.obsidian/**".to_owned()],
                ..MountPolicy::default()
            },
            now,
        )?;

        repository.create(&mount).await?;

        let fetched = repository.find(&mount.name).await?;
        assert_eq!(fetched.name, mount.name);

        let list = repository.list().await?;
        assert_eq!(list.len(), 1);

        repository.delete(&mount.name).await?;
        assert!(matches!(
            repository.find(&mount.name).await,
            Err(memo_core::DbError::NotFound)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn create_returns_conflict_for_duplicate_name() -> Result<(), Box<dyn std::error::Error>>
    {
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
            None,
            MountPolicy::default(),
            now,
        )?;

        repository.create(&mount).await?;
        let duplicate = repository.create(&mount).await;
        assert!(matches!(duplicate, Err(memo_core::DbError::Conflict)));
        Ok(())
    }

    #[tokio::test]
    async fn update_returns_not_found_for_missing_mount() -> Result<(), Box<dyn std::error::Error>>
    {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = SqliteMountRepository::new(pool, PolicyCache::new());

        let now = OffsetDateTime::now_utc();
        let missing = Mount::new(
            MountName::new("Missing")?,
            PathBuf::from("/tmp/missing"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            now,
        )?;

        let result = repository.update(&missing).await;
        assert!(matches!(result, Err(memo_core::DbError::NotFound)));
        Ok(())
    }

    #[tokio::test]
    async fn cache_is_invalidated_after_update() -> Result<(), Box<dyn std::error::Error>> {
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
            None,
            MountPolicy {
                hide_globs: vec!["a/*".to_owned()],
                ..MountPolicy::default()
            },
            now,
        )?;
        repository.create(&mount).await?;

        let fetched = repository.find(&mount.name).await?;
        let compiled_before = repository.policy_cache().get_or_compile(&fetched);
        assert!(compiled_before
            .policy
            .is_hidden(&RelativePath::new("a/1.md")?)?);
        assert!(!compiled_before
            .policy
            .is_hidden(&RelativePath::new("b/1.md")?)?);

        let updated = Mount {
            policy: MountPolicy {
                hide_globs: vec!["b/*".to_owned()],
                ..fetched.policy.clone()
            },
            updated_at: OffsetDateTime::now_utc(),
            ..fetched.clone()
        };
        repository.update(&updated).await?;

        let fetched_after = repository.find(&mount.name).await?;
        let compiled_after = repository.policy_cache().get_or_compile(&fetched_after);
        assert!(!compiled_after
            .policy
            .is_hidden(&RelativePath::new("a/1.md")?)?);
        assert!(compiled_after
            .policy
            .is_hidden(&RelativePath::new("b/1.md")?)?);
        Ok(())
    }
}
