use std::path::{Component, Path, PathBuf};

use memo_core::{Mount, PolicyError, RelativePath};

use crate::mount_registry::repository::PolicyCache;

#[derive(Debug, Clone)]
pub struct PolicyEngine {
    policy_cache: PolicyCache,
}

impl PolicyEngine {
    #[must_use]
    pub fn new(policy_cache: PolicyCache) -> Self {
        Self { policy_cache }
    }

    /// Resolves and validates a read path against mount boundaries and policy.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when path resolution, boundary checks, symlink
    /// checks, or policy checks fail.
    pub fn resolve_read_path(
        &self,
        mount: &Mount,
        relative: &RelativePath,
        file_size: Option<u64>,
    ) -> Result<PathBuf, PolicyError> {
        let compiled = self
            .policy_cache
            .get_or_compile(mount)
            .map_err(|error| PolicyError::InvalidPolicy(error.to_string()))?;
        let lexical_path = mount.resolve(relative);
        let canonical_path = lexical_path
            .canonicalize()
            .map_err(|_| PolicyError::NotFound)?;
        let root_canonical = &compiled.root_path_canonical;
        let normalized = normalize_joined_path(root_canonical, relative);

        if !canonical_path.starts_with(root_canonical) {
            return Err(PolicyError::OutOfBounds);
        }

        if normalized != canonical_path {
            return Err(PolicyError::SymlinkDenied);
        }

        let relative_str = relative.as_str();
        if compiled.hide_globs.is_match(relative_str) {
            return Err(PolicyError::NotFound);
        }
        if compiled.deny_read_globs.is_match(relative_str) {
            return Err(PolicyError::PermissionDenied);
        }
        if let (Some(limit), Some(size)) = (compiled.max_read_bytes, file_size) {
            if size > limit {
                return Err(PolicyError::TooLarge {
                    limit,
                    actual: size,
                });
            }
        }

        Ok(canonical_path)
    }

    /// Resolves and validates a write path against mount boundaries and policy.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when path resolution, boundary checks, symlink
    /// checks, or policy checks fail.
    pub fn resolve_write_path(
        &self,
        mount: &Mount,
        relative: &RelativePath,
        write_size: u64,
    ) -> Result<PathBuf, PolicyError> {
        let compiled = self
            .policy_cache
            .get_or_compile(mount)
            .map_err(|error| PolicyError::InvalidPolicy(error.to_string()))?;
        if relative.is_root() {
            return Err(PolicyError::InvalidPath);
        }

        let lexical_path = mount.resolve(relative);
        let parent = lexical_path.parent().ok_or(PolicyError::InvalidPath)?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|_| PolicyError::InvalidPath)?;
        let root_canonical = &compiled.root_path_canonical;
        let relative_parent = Path::new(relative.as_str())
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let normalized_parent = normalize_parent_path(root_canonical, relative_parent);

        if !canonical_parent.starts_with(root_canonical) {
            return Err(PolicyError::OutOfBounds);
        }

        if normalized_parent != canonical_parent {
            return Err(PolicyError::SymlinkDenied);
        }

        let filename = Path::new(relative.as_str())
            .file_name()
            .ok_or(PolicyError::InvalidPath)?;
        let resolved = canonical_parent.join(filename);

        let relative_str = relative.as_str();
        if compiled.hide_globs.is_match(relative_str)
            || compiled.deny_write_globs.is_match(relative_str)
        {
            return Err(PolicyError::PermissionDenied);
        }
        if let Some(limit) = compiled.max_write_bytes {
            if write_size > limit {
                return Err(PolicyError::TooLarge {
                    limit,
                    actual: write_size,
                });
            }
        }

        Ok(resolved)
    }
}

fn normalize_joined_path(root_canonical: &Path, relative: &RelativePath) -> PathBuf {
    let relative_path = Path::new(relative.as_str());
    normalize_path(&root_canonical.join(relative_path))
}

fn normalize_parent_path(root_canonical: &Path, relative_parent: &Path) -> PathBuf {
    normalize_path(&root_canonical.join(relative_parent))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use memo_core::{
        Audience, Mount, MountMode, MountName, MountPolicy, PolicyError, RelativePath,
    };
    use tempfile::tempdir;
    use time::OffsetDateTime;

    use crate::mount_registry::policy::PolicyEngine;
    use crate::mount_registry::repository::PolicyCache;

    #[test]
    fn read_path_enforces_size_limit() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let root = tempdir.path().join("vault");
        std::fs::create_dir_all(&root)?;
        let file = root.join("note.md");
        std::fs::write(&file, b"hello")?;

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy {
                max_read_bytes: Some(4),
                ..MountPolicy::default()
            },
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let result = engine.resolve_read_path(&mount, &RelativePath::new("note.md")?, Some(5));
        assert_eq!(
            result,
            Err(PolicyError::TooLarge {
                limit: 4,
                actual: 5
            })
        );
        Ok(())
    }

    #[test]
    fn write_path_rejects_root_target() -> Result<(), Box<dyn std::error::Error>> {
        let mount = Mount::new(
            MountName::new("VaultKB")?,
            PathBuf::from("/tmp"),
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let result = engine.resolve_write_path(&mount, &RelativePath::root(), 1);
        assert_eq!(result, Err(PolicyError::InvalidPath));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn read_path_rejects_symlink_escape() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let tempdir = tempdir()?;
        let root = tempdir.path().join("vault");
        let outside = tempdir.path().join("outside");
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(&outside)?;
        std::fs::write(outside.join("secret.txt"), b"secret")?;

        symlink(&outside, root.join("linked"))?;

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let result =
            engine.resolve_read_path(&mount, &RelativePath::new("linked/secret.txt")?, None);
        assert_eq!(result, Err(PolicyError::OutOfBounds));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn write_path_rejects_symlink_parent() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let tempdir = tempdir()?;
        let root = tempdir.path().join("vault");
        let outside = tempdir.path().join("outside");
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(&outside)?;
        symlink(&outside, root.join("linked"))?;

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let result = engine.resolve_write_path(&mount, &RelativePath::new("linked/new.md")?, 1);
        assert_eq!(result, Err(PolicyError::OutOfBounds));
        Ok(())
    }

    #[test]
    fn read_hidden_path_maps_to_not_found() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let root = tempdir.path().join("vault");
        std::fs::create_dir_all(root.join("notes"))?;
        std::fs::write(root.join("notes/private.md"), b"x")?;

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy {
                hide_globs: vec!["notes/private.md".to_owned()],
                ..MountPolicy::default()
            },
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let result =
            engine.resolve_read_path(&mount, &RelativePath::new("notes/private.md")?, Some(1));
        assert_eq!(result, Err(PolicyError::NotFound));
        Ok(())
    }

    #[test]
    fn traversal_corpus_structural_rejections() {
        let corpus = ["../secret", "..", "/etc/passwd", "notes/\0secret"];

        for candidate in corpus {
            assert!(
                RelativePath::new(candidate).is_err(),
                "expected path to fail: {candidate:?}"
            );
        }
    }

    #[test]
    #[ignore = "security"]
    fn traversal_corpus_decoded_forms_rejected() {
        let decoded = ["../secret", "a/../../secret", "/tmp/secret", "..//secret"];
        for candidate in decoded {
            assert!(
                RelativePath::new(candidate).is_err(),
                "decoded traversal should fail: {candidate}"
            );
        }
    }

    #[test]
    fn unicode_normalization_consistent_result() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let root = tempdir.path().join("vault");
        std::fs::create_dir_all(&root)?;

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )?;
        let engine = PolicyEngine::new(PolicyCache::new());

        let nfc = RelativePath::new("notes/caf\u{00E9}.md")?;
        let nfd = RelativePath::new("notes/cafe\u{0301}.md")?;

        let composed_result = engine.resolve_read_path(&mount, &nfc, None);
        let decomposed_result = engine.resolve_read_path(&mount, &nfd, None);

        assert_eq!(composed_result, Err(PolicyError::NotFound));
        assert_eq!(decomposed_result, Err(PolicyError::NotFound));
        Ok(())
    }
}
