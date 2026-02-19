use std::fmt;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

use crate::errors::PolicyError;
use crate::path::RelativePath;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct MountName(String);

impl MountName {
    /// Creates a validated mount name.
    ///
    /// # Errors
    /// Returns [`MountNameError`] when the name is empty, longer than 64 chars,
    /// or contains characters outside `[A-Za-z0-9_-]`.
    pub fn new(input: impl AsRef<str>) -> Result<Self, MountNameError> {
        let input = input.as_ref();
        if input.is_empty() || input.len() > 64 {
            return Err(MountNameError::InvalidLength);
        }

        if !input
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(MountNameError::InvalidCharacters);
        }

        Ok(Self(input.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MountName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::new(input).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountMode {
    ReadOnly,
    ReadWrite,
}

impl MountMode {
    #[must_use]
    pub fn allows_write(&self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Shared,
    AgentOnly,
    HumanOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MountPolicy {
    pub hide_globs: Vec<String>,
    pub deny_read_globs: Vec<String>,
    pub deny_write_globs: Vec<String>,
    pub max_read_bytes: Option<u64>,
    pub max_write_bytes: Option<u64>,
}

impl MountPolicy {
    /// Checks read policy for a path and optional file size.
    ///
    /// # Errors
    /// Returns [`PolicyError`] when denied by glob rules, policy globs are invalid,
    /// or size exceeds `max_read_bytes`.
    pub fn check_read(
        &self,
        path: &RelativePath,
        file_size: Option<u64>,
    ) -> Result<(), PolicyError> {
        if Self::matches_any(&self.deny_read_globs, path)? {
            return Err(PolicyError::PermissionDenied);
        }

        if let (Some(limit), Some(size)) = (self.max_read_bytes, file_size) {
            if size > limit {
                return Err(PolicyError::TooLarge {
                    limit,
                    actual: size,
                });
            }
        }

        Ok(())
    }

    /// Checks write policy for a path and write payload size.
    ///
    /// # Errors
    /// Returns [`PolicyError`] when denied by glob rules, policy globs are invalid,
    /// or size exceeds `max_write_bytes`.
    pub fn check_write(&self, path: &RelativePath, write_size: u64) -> Result<(), PolicyError> {
        if Self::matches_any(&self.deny_write_globs, path)? {
            return Err(PolicyError::PermissionDenied);
        }

        if let Some(limit) = self.max_write_bytes {
            if write_size > limit {
                return Err(PolicyError::TooLarge {
                    limit,
                    actual: write_size,
                });
            }
        }

        Ok(())
    }

    /// Returns whether the path is hidden by hide globs.
    ///
    /// # Errors
    /// Returns [`PolicyError::InvalidPolicy`] if any configured glob is invalid.
    pub fn is_hidden(&self, path: &RelativePath) -> Result<bool, PolicyError> {
        Self::matches_any(&self.hide_globs, path)
    }

    fn matches_any(globs: &[String], path: &RelativePath) -> Result<bool, PolicyError> {
        if globs.is_empty() {
            return Ok(false);
        }

        let mut builder = GlobSetBuilder::new();
        for glob in globs {
            let parsed = Glob::new(glob).map_err(|_| PolicyError::InvalidPolicy(glob.clone()))?;
            builder.add(parsed);
        }

        let matcher = builder
            .build()
            .map_err(|error| PolicyError::InvalidPolicy(error.to_string()))?;
        Ok(matcher.is_match(path.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mount {
    pub name: MountName,
    pub root_path: PathBuf,
    pub mode: MountMode,
    pub audience: Audience,
    pub description: Option<String>,
    pub policy: MountPolicy,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Mount {
    /// Creates a mount aggregate.
    ///
    /// # Errors
    /// Returns [`PolicyError::InvalidPath`] when `root_path` is not absolute.
    pub fn new(
        name: MountName,
        root_path: PathBuf,
        mode: MountMode,
        audience: Audience,
        description: Option<String>,
        policy: MountPolicy,
        now: OffsetDateTime,
    ) -> Result<Self, PolicyError> {
        if !root_path.is_absolute() {
            return Err(PolicyError::InvalidPath);
        }

        Ok(Self {
            name,
            root_path,
            mode,
            audience,
            description,
            policy,
            created_at: now,
            updated_at: now,
        })
    }

    #[must_use]
    pub fn resolve(&self, relative: &RelativePath) -> PathBuf {
        if relative.is_root() {
            self.root_path.clone()
        } else {
            self.root_path.join(Path::new(relative.as_str()))
        }
    }

    /// Applies read checks for hidden and deny-read policies.
    ///
    /// # Errors
    /// Returns [`PolicyError`] when the path is hidden, denied by policy,
    /// or exceeds read limits.
    pub fn check_read(
        &self,
        path: &RelativePath,
        file_size: Option<u64>,
    ) -> Result<(), PolicyError> {
        if self.policy.is_hidden(path)? {
            return Err(PolicyError::NotFound);
        }

        self.policy.check_read(path, file_size)
    }

    /// Applies write checks for mount mode and deny-write policy.
    ///
    /// # Errors
    /// Returns [`PolicyError`] when writes are disabled, path is hidden,
    /// denied by policy, or exceeds write limits.
    pub fn check_write(&self, path: &RelativePath, write_size: u64) -> Result<(), PolicyError> {
        if !self.mode.allows_write() {
            return Err(PolicyError::PermissionDenied);
        }

        if self.policy.is_hidden(path)? {
            return Err(PolicyError::PermissionDenied);
        }

        self.policy.check_write(path, write_size)
    }

    #[must_use]
    pub fn with_policy(mut self, policy: MountPolicy, now: OffsetDateTime) -> Self {
        self.policy = policy;
        self.updated_at = now;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MountNameError {
    #[error("mount name must be 1..=64 characters")]
    InvalidLength,
    #[error("mount name may only contain ASCII alphanumeric, '-' and '_'")]
    InvalidCharacters,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;
    use time::OffsetDateTime;

    use crate::errors::PolicyError;
    use crate::mount::{Audience, Mount, MountMode, MountName, MountPolicy};
    use crate::path::RelativePath;

    #[test]
    fn validates_mount_name() {
        assert!(MountName::new("VaultKB").is_ok());
        assert!(MountName::new("").is_err());
        assert!(MountName::new("with space").is_err());
    }

    #[test]
    fn mount_policy_matches_globs() {
        let policy = MountPolicy {
            hide_globs: vec!["**/*.secret".to_owned()],
            ..MountPolicy::default()
        };

        let hidden = policy
            .is_hidden(&RelativePath::new("notes/a.secret").expect("valid path"))
            .expect("glob should compile");
        assert!(hidden);
    }

    #[test]
    fn read_only_mount_rejects_write() {
        let now = OffsetDateTime::now_utc();
        let mount = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("/tmp/vault"),
            MountMode::ReadOnly,
            Audience::Shared,
            None,
            MountPolicy::default(),
            now,
        )
        .expect("valid mount");

        let result = mount.check_write(&RelativePath::new("note.md").expect("valid path"), 10);
        assert_eq!(result, Err(PolicyError::PermissionDenied));
    }

    #[test]
    fn write_size_limit_enforced() {
        let now = OffsetDateTime::now_utc();
        let mount = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("/tmp/vault"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy {
                max_write_bytes: Some(8),
                ..MountPolicy::default()
            },
            now,
        )
        .expect("valid mount");

        let result = mount.check_write(&RelativePath::new("note.md").expect("valid path"), 9);
        assert_eq!(
            result,
            Err(PolicyError::TooLarge {
                limit: 8,
                actual: 9
            })
        );
    }

    #[test]
    fn rejects_non_absolute_root() {
        let now = OffsetDateTime::now_utc();
        let result = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("relative/path"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            now,
        );

        assert_eq!(result, Err(PolicyError::InvalidPath));
    }

    #[test]
    fn resolve_handles_root_and_nested() {
        let now = OffsetDateTime::now_utc();
        let mount = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("/tmp/vault"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            now,
        )
        .expect("valid mount");

        assert_eq!(
            mount.resolve(&RelativePath::root()),
            PathBuf::from("/tmp/vault")
        );
        assert_eq!(
            mount.resolve(&RelativePath::new("notes/a.md").expect("valid path")),
            PathBuf::from("/tmp/vault/notes/a.md")
        );
    }

    #[test]
    fn check_read_respects_hidden_and_size() {
        let now = OffsetDateTime::now_utc();
        let mount = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("/tmp/vault"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy {
                hide_globs: vec!["**/*.hidden".to_owned()],
                max_read_bytes: Some(4),
                ..MountPolicy::default()
            },
            now,
        )
        .expect("valid mount");

        let hidden = mount.check_read(&RelativePath::new("a.hidden").expect("valid path"), Some(1));
        assert_eq!(hidden, Err(PolicyError::NotFound));

        let too_large = mount.check_read(&RelativePath::new("a.md").expect("valid path"), Some(8));
        assert_eq!(
            too_large,
            Err(PolicyError::TooLarge {
                limit: 4,
                actual: 8
            })
        );
    }

    #[test]
    fn policy_reports_invalid_glob() {
        let policy = MountPolicy {
            deny_read_globs: vec!["[".to_owned()],
            ..MountPolicy::default()
        };
        let result = policy.check_read(&RelativePath::new("a.md").expect("valid path"), Some(1));
        assert_eq!(result, Err(PolicyError::InvalidPolicy("[".to_owned())));
    }

    #[test]
    fn with_policy_updates_timestamp() {
        let now = OffsetDateTime::now_utc();
        let later = now + time::Duration::minutes(1);
        let mount = Mount::new(
            MountName::new("VaultKB").expect("valid mount name"),
            PathBuf::from("/tmp/vault"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            now,
        )
        .expect("valid mount");

        let updated = mount.with_policy(
            MountPolicy {
                deny_write_globs: vec!["**/*.tmp".to_owned()],
                ..MountPolicy::default()
            },
            later,
        );
        assert_eq!(updated.updated_at, later);
        assert_eq!(updated.policy.deny_write_globs, vec!["**/*.tmp".to_owned()]);
    }

    #[test]
    fn mount_name_deserialize_enforces_validation() {
        let valid: MountName = serde_json::from_value(json!("VaultKB")).expect("valid mount name");
        assert_eq!(valid.as_str(), "VaultKB");

        let invalid = serde_json::from_value::<MountName>(json!("with space"));
        assert!(invalid.is_err());
    }
}
