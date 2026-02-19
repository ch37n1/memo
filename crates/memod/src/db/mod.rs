use std::str::FromStr;
use std::time::Duration;

use memo_core::mount::{Audience, MountMode};
use memo_core::DbError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Executor, SqlitePool};

const CREATE_SCHEMA_MIGRATIONS_TABLE: &str = r"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
)
";

const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("migrations/0001_init.sql"))];

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
}

impl DbConfig {
    #[must_use]
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        }
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self::new("sqlite://memo.db")
    }
}

/// Initializes the `SQLite` connection pool with WAL and foreign keys enabled,
/// then applies embedded schema migrations.
///
/// # Errors
///
/// Returns [`DbError::Connection`] when the URL is not `SQLite`, pool bounds are
/// invalid, connection options fail, or pool creation fails.
/// Returns [`DbError::Query`] when migration application fails.
pub async fn init_pool(config: &DbConfig) -> Result<SqlitePool, DbError> {
    if !config.database_url.starts_with("sqlite:") {
        return Err(DbError::Connection(
            "database_url must start with sqlite:".to_owned(),
        ));
    }

    if config.min_connections > config.max_connections {
        return Err(DbError::Connection(
            "min_connections must be <= max_connections".to_owned(),
        ));
    }

    let connect_options = SqliteConnectOptions::from_str(&config.database_url)
        .map_err(|error| DbError::Connection(error.to_string()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout)
        .connect_with(connect_options)
        .await
        .map_err(|error| DbError::Connection(error.to_string()))?;

    apply_migrations(&pool).await?;

    Ok(pool)
}

/// Applies embedded schema migrations in order.
///
/// # Errors
///
/// Returns [`DbError::Query`] when starting a transaction, checking migration state,
/// executing migration SQL, recording migration versions, or committing fails.
pub async fn apply_migrations(pool: &SqlitePool) -> Result<(), DbError> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

    tx.execute(CREATE_SCHEMA_MIGRATIONS_TABLE)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

    for (version, migration_sql) in MIGRATIONS {
        let applied = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM schema_migrations WHERE version = ?1",
        )
        .bind(version)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

        if applied == 0 {
            sqlx::raw_sql(migration_sql)
                .execute(&mut *tx)
                .await
                .map_err(|error| DbError::Query(error.to_string()))?;

            sqlx::query("INSERT INTO schema_migrations (version) VALUES (?1)")
                .bind(version)
                .execute(&mut *tx)
                .await
                .map_err(|error| DbError::Query(error.to_string()))?;
        }
    }

    tx.commit()
        .await
        .map_err(|error| DbError::Query(error.to_string()))?;

    Ok(())
}

#[must_use]
pub fn mount_mode_to_db(mode: &MountMode) -> &'static str {
    match mode {
        MountMode::ReadOnly => "ro",
        MountMode::ReadWrite => "rw",
    }
}

/// Parses a persisted mount mode value from `SQLite` storage.
///
/// # Errors
///
/// Returns [`DbError::Query`] when `value` is not one of `ro` or `rw`.
pub fn mount_mode_from_db(value: &str) -> Result<MountMode, DbError> {
    match value {
        "ro" => Ok(MountMode::ReadOnly),
        "rw" => Ok(MountMode::ReadWrite),
        _ => Err(DbError::Query(format!("invalid mount mode: {value}"))),
    }
}

#[must_use]
pub fn audience_to_db(audience: &Audience) -> &'static str {
    match audience {
        Audience::Shared => "shared",
        Audience::AgentOnly => "agent-only",
        Audience::HumanOnly => "human-only",
    }
}

/// Parses a persisted audience value from `SQLite` storage.
///
/// # Errors
///
/// Returns [`DbError::Query`] when `value` is not one of `shared`, `agent-only`,
/// or `human-only`.
pub fn audience_from_db(value: &str) -> Result<Audience, DbError> {
    match value {
        "shared" => Ok(Audience::Shared),
        "agent-only" => Ok(Audience::AgentOnly),
        "human-only" => Ok(Audience::HumanOnly),
        _ => Err(DbError::Query(format!("invalid audience: {value}"))),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use memo_core::mount::{Audience, MountMode};

    use super::{
        apply_migrations, audience_from_db, audience_to_db, init_pool, mount_mode_from_db,
        mount_mode_to_db, DbConfig, MIGRATIONS,
    };

    #[tokio::test]
    async fn applies_migrations_idempotently() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_path = tempdir.path().join("memo.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let config = DbConfig::new(database_url);

        let pool = init_pool(&config).await?;

        apply_migrations(&pool).await?;

        let migration_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM schema_migrations")
                .fetch_one(&pool)
                .await?;

        let expected_count = i64::try_from(MIGRATIONS.len())?;
        assert_eq!(migration_count, expected_count);

        let tables = sqlx::query_scalar::<_, String>(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind("mounts")
        .fetch_one(&pool)
        .await?;

        assert_eq!(tables, "mounts");
        Ok(())
    }

    #[tokio::test]
    async fn enables_wal_mode_and_foreign_keys() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_path = tempdir.path().join("memo.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let config = DbConfig::new(database_url);

        let pool = init_pool(&config).await?;

        let journal_mode = sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await?;
        let foreign_keys = sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?;

        assert_eq!(journal_mode.to_lowercase(), "wal");
        assert_eq!(foreign_keys, 1);
        Ok(())
    }

    #[test]
    fn mount_mode_db_mapping_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(mount_mode_to_db(&MountMode::ReadOnly), "ro");
        assert_eq!(mount_mode_to_db(&MountMode::ReadWrite), "rw");
        assert_eq!(mount_mode_from_db("ro")?, MountMode::ReadOnly);
        assert_eq!(mount_mode_from_db("rw")?, MountMode::ReadWrite);
        assert!(mount_mode_from_db("read_only").is_err());
        Ok(())
    }

    #[test]
    fn audience_db_mapping_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(audience_to_db(&Audience::Shared), "shared");
        assert_eq!(audience_to_db(&Audience::AgentOnly), "agent-only");
        assert_eq!(audience_to_db(&Audience::HumanOnly), "human-only");
        assert_eq!(audience_from_db("shared")?, Audience::Shared);
        assert_eq!(audience_from_db("agent-only")?, Audience::AgentOnly);
        assert_eq!(audience_from_db("human-only")?, Audience::HumanOnly);
        assert!(audience_from_db("agent_only").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_invalid_database_url() -> Result<(), Box<dyn std::error::Error>> {
        let config = DbConfig::new("not-a-sqlite-url");
        let result = init_pool(&config).await;
        assert!(matches!(result, Err(memo_core::DbError::Connection(_))));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_invalid_pool_bounds() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_path = tempdir.path().join("memo.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let mut config = DbConfig::new(database_url);
        config.min_connections = 2;
        config.max_connections = 1;

        let result = init_pool(&config).await;
        assert!(matches!(result, Err(memo_core::DbError::Connection(_))));
        Ok(())
    }
}
