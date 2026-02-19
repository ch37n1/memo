use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use memo_core::{DenialReason, DomainEvent, MountName, TokenId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    pub timestamp: OffsetDateTime,
    #[serde(default)]
    pub token_id: Option<TokenId>,
    pub operation: String,
    #[serde(default)]
    pub mount: Option<MountName>,
    #[serde(default)]
    pub path: Option<String>,
    pub result: AuditResult,
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditResponse {
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuditQuery {
    #[serde(default)]
    pub mount: Option<MountName>,
    #[serde(default)]
    pub token_id: Option<TokenId>,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub result: Option<AuditResult>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub before: Option<OffsetDateTime>,
    #[serde(default)]
    pub after: Option<OffsetDateTime>,
    #[serde(default)]
    pub after_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub token_id: Option<TokenId>,
    pub operation: String,
    pub mount: Option<MountName>,
    pub path: Option<String>,
    pub result: AuditResult,
    pub error_code: Option<String>,
}

impl AuditRecord {
    #[must_use]
    pub fn ok(
        operation: impl Into<String>,
        token_id: Option<TokenId>,
        mount: Option<MountName>,
        path: Option<String>,
    ) -> Self {
        Self {
            token_id,
            operation: operation.into(),
            mount,
            path,
            result: AuditResult::Ok,
            error_code: None,
        }
    }

    #[must_use]
    pub fn error(
        operation: impl Into<String>,
        token_id: Option<TokenId>,
        mount: Option<MountName>,
        path: Option<String>,
        error_code: impl Into<String>,
    ) -> Self {
        Self {
            token_id,
            operation: operation.into(),
            mount,
            path,
            result: AuditResult::Error,
            error_code: Some(error_code.into()),
        }
    }
}

#[derive(Debug)]
pub struct AuditService {
    path: PathBuf,
    next_id: AtomicU64,
}

impl AuditService {
    /// Creates an audit service backed by a JSONL file.
    ///
    /// # Errors
    ///
    /// Returns an IO error when parent directory creation or file scanning fails.
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let next_id = scan_next_id(&path).unwrap_or(1);
        Ok(Self {
            path,
            next_id: AtomicU64::new(next_id),
        })
    }

    /// Appends one audit record to the JSONL log.
    ///
    /// # Errors
    ///
    /// Returns an IO error when serialization or file append fails.
    pub async fn append_record(&self, record: AuditRecord) -> std::io::Result<AuditEntry> {
        let entry = AuditEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp: OffsetDateTime::now_utc(),
            token_id: record.token_id,
            operation: record.operation,
            mount: record.mount,
            path: record.path,
            result: record.result,
            error_code: record.error_code,
        };

        self.append_entry(&entry).await?;
        Ok(entry)
    }

    /// Converts a domain event into an audit record and appends it.
    ///
    /// # Errors
    ///
    /// Returns an IO error when append fails.
    pub async fn append_domain_event(&self, event: DomainEvent) -> std::io::Result<AuditEntry> {
        let record = match event {
            DomainEvent::FileRead {
                token_id,
                mount,
                path,
                ..
            } => AuditRecord::ok(
                "file_read",
                Some(token_id),
                Some(mount),
                Some(path.as_str().to_owned()),
            ),
            DomainEvent::FileWritten {
                token_id,
                mount,
                path,
                ..
            } => AuditRecord::ok(
                "file_written",
                Some(token_id),
                Some(mount),
                Some(path.as_str().to_owned()),
            ),
            DomainEvent::DirListed {
                token_id,
                mount,
                path,
            } => AuditRecord::ok(
                "dir_listed",
                Some(token_id),
                Some(mount),
                Some(path.as_str().to_owned()),
            ),
            DomainEvent::MountRegistered { name, .. } => {
                AuditRecord::ok("mount_registered", None, Some(name), None)
            }
            DomainEvent::MountUpdated { name } => {
                AuditRecord::ok("mount_updated", None, Some(name), None)
            }
            DomainEvent::MountRemoved { name } => {
                AuditRecord::ok("mount_removed", None, Some(name), None)
            }
            DomainEvent::TokenCreated { id, .. } => {
                AuditRecord::ok("token_created", Some(id), None, None)
            }
            DomainEvent::TokenRevoked { id } => {
                AuditRecord::ok("token_revoked", Some(id), None, None)
            }
            DomainEvent::AccessDenied {
                token_id,
                reason,
                mount,
            } => AuditRecord::error(
                "access_denied",
                token_id,
                mount,
                None,
                denial_reason_code(&reason),
            ),
        };

        self.append_record(record).await
    }

    /// Loads and filters audit entries.
    ///
    /// # Errors
    ///
    /// Returns an IO error when reading the audit file fails.
    pub async fn query(&self, filter: &AuditQuery) -> std::io::Result<Vec<AuditEntry>> {
        let content = read_if_exists(&self.path).await?;
        let mut entries = content
            .lines()
            .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
            .filter(|entry| match_filter(entry, filter))
            .collect::<Vec<_>>();

        entries.sort_by_key(|entry| entry.id);
        if let Some(limit) = filter.limit.and_then(|value| usize::try_from(value).ok()) {
            if entries.len() > limit {
                let keep_from = entries.len().saturating_sub(limit);
                entries = entries.split_off(keep_from);
            }
        }

        Ok(entries)
    }

    /// Rotates the audit log to `<name>.1` when row count is above `max_rows`.
    ///
    /// # Errors
    ///
    /// Returns an IO error when checking, deleting, or renaming files fails.
    pub async fn prune_if_exceeds(&self, max_rows: usize) -> std::io::Result<()> {
        let content = read_if_exists(&self.path).await?;
        if content.lines().count() <= max_rows {
            return Ok(());
        }

        let rotated = rotate_path(&self.path);
        if tokio::fs::try_exists(&rotated).await.unwrap_or(false) {
            tokio::fs::remove_file(&rotated).await?;
        }

        tokio::fs::rename(&self.path, &rotated).await?;
        self.next_id.store(1, Ordering::Relaxed);
        Ok(())
    }

    async fn append_entry(&self, entry: &AuditEntry) -> std::io::Result<()> {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;

        let mut line = serde_json::to_string(entry)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        line.push('\n');
        file.write_all(line.as_bytes()).await
    }
}

fn scan_next_id(path: &Path) -> Option<u64> {
    let content = std::fs::read_to_string(path).ok()?;
    let max = content
        .lines()
        .filter_map(|line| serde_json::from_str::<AuditEntry>(line).ok())
        .map(|entry| entry.id)
        .max()
        .unwrap_or(0);
    Some(max.saturating_add(1))
}

async fn read_if_exists(path: &Path) -> std::io::Result<String> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

fn rotate_path(path: &Path) -> PathBuf {
    let file_name = path.file_name().map_or_else(
        || "audit.log".to_owned(),
        |value| value.to_string_lossy().into_owned(),
    );
    path.with_file_name(format!("{file_name}.1"))
}

fn match_filter(entry: &AuditEntry, filter: &AuditQuery) -> bool {
    if let Some(mount) = &filter.mount {
        if entry.mount.as_ref() != Some(mount) {
            return false;
        }
    }

    if let Some(token_id) = filter.token_id {
        if entry.token_id != Some(token_id) {
            return false;
        }
    }

    if let Some(operation) = &filter.operation {
        if &entry.operation != operation {
            return false;
        }
    }

    if let Some(result) = &filter.result {
        if &entry.result != result {
            return false;
        }
    }

    if let Some(before) = filter.before {
        if entry.timestamp >= before {
            return false;
        }
    }

    if let Some(after) = filter.after {
        if entry.timestamp <= after {
            return false;
        }
    }

    if let Some(after_id) = filter.after_id {
        if entry.id <= after_id {
            return false;
        }
    }

    true
}

#[must_use]
pub fn denial_reason_code(reason: &DenialReason) -> &'static str {
    match reason {
        DenialReason::AuthRequired => "auth_required",
        DenialReason::TokenInvalid => "token_invalid",
        DenialReason::TokenExpired => "token_expired",
        DenialReason::MissingScope => "missing_scope",
        DenialReason::PolicyViolation => "policy_violated",
        DenialReason::InvalidPath => "invalid_path",
        DenialReason::OutOfBounds => "out_of_bounds",
        DenialReason::SymlinkDenied => "symlink_denied",
        DenialReason::MountNotFound => "mount_not_found",
        DenialReason::NotFound => "not_found",
        DenialReason::Other => "internal",
    }
}

#[cfg(test)]
mod tests {
    use memo_core::{DenialReason, DomainEvent, MountMode, MountName, RelativePath, TokenId};
    use tempfile::tempdir;

    use super::{denial_reason_code, AuditQuery, AuditRecord, AuditResult, AuditService};

    #[tokio::test]
    async fn append_and_query_filters() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let service = AuditService::new(tempdir.path().join("audit.log"))?;

        let _ = service
            .append_record(AuditRecord::ok(
                "file_read",
                None,
                Some(MountName::new("VaultKB")?),
                Some("notes/a.md".to_owned()),
            ))
            .await?;
        let _ = service
            .append_record(AuditRecord::error(
                "file_read",
                None,
                Some(MountName::new("VaultKB")?),
                Some("notes/b.md".to_owned()),
                "not_found",
            ))
            .await?;

        let filtered = service
            .query(&AuditQuery {
                mount: Some(MountName::new("VaultKB")?),
                operation: Some("file_read".to_owned()),
                result: Some(AuditResult::Error),
                ..AuditQuery::default()
            })
            .await?;

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].result, AuditResult::Error);
        Ok(())
    }

    #[tokio::test]
    async fn rotates_when_pruned() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let path = tempdir.path().join("audit.log");
        let service = AuditService::new(path.clone())?;

        for idx in 0..3 {
            let _ = service
                .append_record(AuditRecord::ok(
                    format!("op-{idx}"),
                    None,
                    Some(MountName::new("VaultKB")?),
                    None,
                ))
                .await?;
        }

        service.prune_if_exceeds(2).await?;

        assert!(path.with_file_name("audit.log.1").exists());
        assert!(!path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn domain_events_map_to_audit_operations() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let service = AuditService::new(tempdir.path().join("audit.log"))?;
        let token_id = TokenId::new();
        let mount = MountName::new("VaultKB")?;
        let path = RelativePath::new("notes/a.md")?;

        let events = vec![
            DomainEvent::FileRead {
                token_id,
                mount: mount.clone(),
                path: path.clone(),
                bytes: 3,
            },
            DomainEvent::FileWritten {
                token_id,
                mount: mount.clone(),
                path: path.clone(),
                bytes: 4,
            },
            DomainEvent::DirListed {
                token_id,
                mount: mount.clone(),
                path: RelativePath::root(),
            },
            DomainEvent::MountRegistered {
                name: mount.clone(),
                mode: MountMode::ReadWrite,
            },
            DomainEvent::MountUpdated {
                name: mount.clone(),
            },
            DomainEvent::MountRemoved {
                name: mount.clone(),
            },
            DomainEvent::TokenCreated {
                id: token_id,
                name: "agent".to_owned(),
            },
            DomainEvent::TokenRevoked { id: token_id },
            DomainEvent::AccessDenied {
                token_id: Some(token_id),
                reason: DenialReason::MissingScope,
                mount: Some(mount.clone()),
            },
        ];

        for event in events {
            let _ = service.append_domain_event(event).await?;
        }

        let all = service.query(&AuditQuery::default()).await?;
        let operations = all
            .iter()
            .map(|entry| entry.operation.as_str())
            .collect::<Vec<_>>();
        assert!(operations.contains(&"file_read"));
        assert!(operations.contains(&"file_written"));
        assert!(operations.contains(&"dir_listed"));
        assert!(operations.contains(&"mount_registered"));
        assert!(operations.contains(&"mount_updated"));
        assert!(operations.contains(&"mount_removed"));
        assert!(operations.contains(&"token_created"));
        assert!(operations.contains(&"token_revoked"));
        assert!(operations.contains(&"access_denied"));
        Ok(())
    }

    #[tokio::test]
    async fn query_supports_time_and_id_filters() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let service = AuditService::new(tempdir.path().join("audit.log"))?;
        let mount = MountName::new("VaultKB")?;

        let first = service
            .append_record(AuditRecord::ok(
                "first",
                None,
                Some(mount.clone()),
                Some("notes/1.md".to_owned()),
            ))
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let second = service
            .append_record(AuditRecord::ok(
                "second",
                None,
                Some(mount.clone()),
                Some("notes/2.md".to_owned()),
            ))
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let third = service
            .append_record(AuditRecord::ok(
                "third",
                None,
                Some(mount),
                Some("notes/3.md".to_owned()),
            ))
            .await?;

        let after_id = service
            .query(&AuditQuery {
                after_id: Some(first.id),
                ..AuditQuery::default()
            })
            .await?;
        assert_eq!(after_id.len(), 2);
        assert_eq!(after_id[0].id, second.id);

        let before = service
            .query(&AuditQuery {
                before: Some(third.timestamp),
                ..AuditQuery::default()
            })
            .await?;
        assert_eq!(before.len(), 2);
        assert_eq!(before[0].id, first.id);
        assert_eq!(before[1].id, second.id);

        let after = service
            .query(&AuditQuery {
                after: Some(first.timestamp),
                ..AuditQuery::default()
            })
            .await?;
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].id, second.id);
        assert_eq!(after[1].id, third.id);

        let limited = service
            .query(&AuditQuery {
                limit: Some(2),
                ..AuditQuery::default()
            })
            .await?;
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].id, second.id);
        assert_eq!(limited[1].id, third.id);
        Ok(())
    }

    #[test]
    fn denial_reason_codes_cover_all_variants() {
        assert_eq!(
            denial_reason_code(&DenialReason::AuthRequired),
            "auth_required"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::TokenInvalid),
            "token_invalid"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::TokenExpired),
            "token_expired"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::MissingScope),
            "missing_scope"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::PolicyViolation),
            "policy_violated"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::InvalidPath),
            "invalid_path"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::OutOfBounds),
            "out_of_bounds"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::SymlinkDenied),
            "symlink_denied"
        );
        assert_eq!(
            denial_reason_code(&DenialReason::MountNotFound),
            "mount_not_found"
        );
        assert_eq!(denial_reason_code(&DenialReason::NotFound), "not_found");
        assert_eq!(denial_reason_code(&DenialReason::Other), "internal");
    }
}
