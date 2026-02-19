use std::path::{Path, PathBuf};
use std::sync::Arc;

use memo_core::repositories::MountRepository;
use memo_core::{
    ApiError, FsAction, Mount, MountName, MountPath, RelativePath, Scope, ScopeMount, Token,
};
use serde::Serialize;
use time::OffsetDateTime;

use crate::fs::atomic::atomic_write_bytes;
use crate::fs::find;
use crate::fs::grep;
use crate::mount_registry::policy::PolicyEngine;
use crate::mount_registry::repository::SqliteMountRepository;

const TREE_MAX_DEPTH: u8 = 10;
const TREE_ENTRY_CAP: usize = 10_000;

#[derive(Debug, Clone)]
pub struct FileSystemService {
    mount_repository: Arc<SqliteMountRepository>,
    policy_engine: PolicyEngine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FsEntryKind {
    File,
    Dir,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsEntry {
    pub name: String,
    pub kind: FsEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LsResponse {
    pub path: MountPath,
    pub entries: Vec<FsEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeNode {
    pub name: String,
    pub kind: FsEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeResponse {
    pub path: MountPath,
    pub depth: u8,
    pub truncated: bool,
    pub tree: TreeNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatResponse {
    pub path: MountPath,
    pub kind: FsEntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<OffsetDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WriteResponse {
    pub path: MountPath,
    pub written_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MkdirResponse {
    pub path: MountPath,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoveResponse {
    pub src: MountPath,
    pub dst: MountPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoveResponse {
    pub path: MountPath,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CopyResponse {
    pub src: MountPath,
    pub dst: MountPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrepMatch {
    pub path: MountPath,
    pub line: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrepResponse {
    pub pattern: String,
    pub matches: Vec<GrepMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindResult {
    pub path: MountPath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindResponse {
    pub glob: String,
    pub results: Vec<FindResult>,
}

#[allow(clippy::missing_errors_doc)]
impl FileSystemService {
    #[must_use]
    pub fn new(mount_repository: Arc<SqliteMountRepository>) -> Self {
        let policy_engine = PolicyEngine::new(mount_repository.policy_cache().clone());
        Self {
            mount_repository,
            policy_engine,
        }
    }

    pub async fn ls(&self, path: &MountPath, _info: bool) -> Result<LsResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let root = canonical_mount_root(&mount)?;
        let resolved = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        if !metadata.is_dir() {
            return Err(ApiError::InvalidPath("ls requires a directory".to_owned()));
        }

        let mut entries = Vec::new();
        let mut reader = tokio::fs::read_dir(&resolved)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        while let Some(entry) = reader
            .next_entry()
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
        {
            let entry_path = entry.path();
            let Some(relative) = to_relative_path(&root, &entry_path) else {
                continue;
            };

            let Ok(entry_meta) = tokio::fs::metadata(&entry_path).await else {
                continue;
            };

            let file_size = if entry_meta.is_file() {
                Some(entry_meta.len())
            } else {
                None
            };

            if self
                .policy_engine
                .resolve_read_path(&mount, &relative, file_size)
                .is_err()
            {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            entries.push(FsEntry {
                name,
                kind: kind_from_metadata(&entry_meta),
                size: file_size,
                modified_at: modified_at(&entry_meta),
            });
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(LsResponse {
            path: path.clone(),
            entries,
        })
    }

    pub async fn tree(
        &self,
        path: &MountPath,
        depth: Option<u8>,
    ) -> Result<TreeResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let resolved = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        if !metadata.is_dir() {
            return Err(ApiError::InvalidPath(
                "tree requires a directory".to_owned(),
            ));
        }

        let effective_depth = depth.unwrap_or(3).min(TREE_MAX_DEPTH);
        let mut seen = 0_usize;
        let mut truncated = false;

        let root_name = if path.relative().is_root() {
            String::new()
        } else {
            Path::new(path.relative().as_str())
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("")
                .to_owned()
        };

        let root = canonical_mount_root(&mount)?;
        let tree = self.build_tree(
            &mount,
            &root,
            &resolved,
            root_name,
            effective_depth,
            &mut seen,
            &mut truncated,
        )?;

        Ok(TreeResponse {
            path: path.clone(),
            depth: effective_depth,
            truncated,
            tree,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_tree(
        &self,
        mount: &Mount,
        root: &Path,
        absolute: &Path,
        name: String,
        depth: u8,
        seen: &mut usize,
        truncated: &mut bool,
    ) -> Result<TreeNode, ApiError> {
        *seen += 1;
        if *seen > TREE_ENTRY_CAP {
            *truncated = true;
            return Ok(TreeNode {
                name,
                kind: FsEntryKind::Dir,
                size: None,
                modified_at: None,
                children: Vec::new(),
            });
        }

        let metadata = std::fs::metadata(absolute)
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        let is_dir = metadata.is_dir();
        let mut node = TreeNode {
            name,
            kind: kind_from_metadata_std(&metadata),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            modified_at: Some(modified_at_std(&metadata)),
            children: Vec::new(),
        };

        if !is_dir || depth == 0 || *truncated {
            return Ok(node);
        }

        let entries =
            std::fs::read_dir(absolute).map_err(|error| ApiError::Internal(error.to_string()))?;
        for entry in entries.flatten() {
            if *truncated {
                break;
            }

            let child_abs = entry.path();
            let Some(child_rel) = to_relative_path(root, &child_abs) else {
                continue;
            };

            let Ok(child_meta) = std::fs::metadata(&child_abs) else {
                continue;
            };
            let file_size = if child_meta.is_file() {
                Some(child_meta.len())
            } else {
                None
            };

            if self
                .policy_engine
                .resolve_read_path(mount, &child_rel, file_size)
                .is_err()
            {
                continue;
            }

            let child_name = entry.file_name().to_string_lossy().into_owned();
            let child = self.build_tree(
                mount,
                root,
                &child_abs,
                child_name,
                depth.saturating_sub(1),
                seen,
                truncated,
            )?;
            node.children.push(child);
        }

        node.children.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(node)
    }

    pub async fn stat(&self, path: &MountPath) -> Result<StatResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let first_pass = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let metadata = tokio::fs::metadata(&first_pass)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;
        let file_size = if metadata.is_file() {
            Some(metadata.len())
        } else {
            None
        };

        let resolved = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), file_size)
            .map_err(ApiError::from)?;
        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        let memo_summary = if metadata.is_dir() {
            parse_summary_from_index(&resolved.join("index.md"))
        } else if resolved
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name == "index.md")
        {
            parse_summary_from_index(&resolved)
        } else {
            None
        };

        Ok(StatResponse {
            path: path.clone(),
            kind: kind_from_metadata(&metadata),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            modified_at: modified_at(&metadata),
            created_at: created_at(&metadata),
            memo_summary,
        })
    }

    pub async fn read_file_meta(
        &self,
        path: &MountPath,
    ) -> Result<(PathBuf, u64, String), ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let first = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let metadata = tokio::fs::metadata(&first)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        if !metadata.is_file() {
            return Err(ApiError::InvalidPath("read requires a file".to_owned()));
        }

        let resolved = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), Some(metadata.len()))
            .map_err(ApiError::from)?;
        let mime = mime_from_path(&resolved);

        Ok((resolved, metadata.len(), mime))
    }

    pub async fn write_file(
        &self,
        path: &MountPath,
        bytes: &[u8],
    ) -> Result<WriteResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let write_size = u64::try_from(bytes.len())
            .map_err(|_| ApiError::Internal("payload too large".to_owned()))?;

        let resolved = self
            .policy_engine
            .resolve_write_path(&mount, path.relative(), write_size)
            .map_err(ApiError::from)?;
        atomic_write_bytes(&resolved, bytes)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        Ok(WriteResponse {
            path: path.clone(),
            written_bytes: write_size,
        })
    }

    pub async fn mkdir(&self, path: &MountPath) -> Result<MkdirResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let resolved = self
            .policy_engine
            .resolve_write_path(&mount, path.relative(), 0)
            .map_err(ApiError::from)?;
        tokio::fs::create_dir_all(&resolved)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        Ok(MkdirResponse {
            path: path.clone(),
            created: true,
        })
    }

    pub async fn mv(&self, src: &MountPath, dst: &MountPath) -> Result<MoveResponse, ApiError> {
        if src.mount() != dst.mount() {
            return Err(ApiError::InvalidPath(
                "mv requires source and destination in the same mount".to_owned(),
            ));
        }

        let mount = self.load_mount(src.mount()).await?;
        let src_resolved = self
            .policy_engine
            .resolve_read_path(&mount, src.relative(), None)
            .map_err(ApiError::from)?;
        let dst_resolved = self
            .policy_engine
            .resolve_write_path(&mount, dst.relative(), 0)
            .map_err(ApiError::from)?;

        if let Some(parent) = dst_resolved.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }
        tokio::fs::rename(src_resolved, dst_resolved)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        Ok(MoveResponse {
            src: src.clone(),
            dst: dst.clone(),
        })
    }

    pub async fn rm(&self, path: &MountPath, recursive: bool) -> Result<RemoveResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let resolved = self
            .policy_engine
            .resolve_write_path(&mount, path.relative(), 0)
            .map_err(ApiError::from)?;

        let metadata = tokio::fs::metadata(&resolved)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        if metadata.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&resolved)
                    .await
                    .map_err(|error| ApiError::Internal(error.to_string()))?;
            } else {
                match tokio::fs::remove_dir(&resolved).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                        return Err(ApiError::Conflict(
                            "directory not empty; use recursive=true".to_owned(),
                        ));
                    }
                    Err(error) => return Err(ApiError::Internal(error.to_string())),
                }
            }
        } else {
            tokio::fs::remove_file(&resolved)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
        }

        Ok(RemoveResponse {
            path: path.clone(),
            deleted: true,
        })
    }

    pub async fn cp(&self, src: &MountPath, dst: &MountPath) -> Result<CopyResponse, ApiError> {
        let src_mount = self.load_mount(src.mount()).await?;
        let dst_mount = self.load_mount(dst.mount()).await?;

        let src_resolved = self
            .policy_engine
            .resolve_read_path(&src_mount, src.relative(), None)
            .map_err(ApiError::from)?;
        let src_meta = tokio::fs::metadata(&src_resolved)
            .await
            .map_err(|_| ApiError::NotFound("path not found".to_owned()))?;

        if !src_meta.is_file() {
            return Err(ApiError::InvalidPath(
                "cp requires a file source".to_owned(),
            ));
        }

        let src_checked = self
            .policy_engine
            .resolve_read_path(&src_mount, src.relative(), Some(src_meta.len()))
            .map_err(ApiError::from)?;
        let bytes = tokio::fs::read(src_checked)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        let write_size = u64::try_from(bytes.len())
            .map_err(|_| ApiError::Internal("payload too large".to_owned()))?;
        let dst_resolved = self
            .policy_engine
            .resolve_write_path(&dst_mount, dst.relative(), write_size)
            .map_err(ApiError::from)?;
        atomic_write_bytes(&dst_resolved, &bytes)
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        Ok(CopyResponse {
            src: src.clone(),
            dst: dst.clone(),
        })
    }

    pub async fn grep(
        &self,
        path: &MountPath,
        pattern: &str,
        recursive: bool,
        case_sensitive: bool,
        max_results: u64,
    ) -> Result<GrepResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let start = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let root = canonical_mount_root(&mount)?;

        let hits = grep::grep(
            &root,
            &start,
            pattern,
            recursive,
            case_sensitive,
            usize::try_from(max_results).unwrap_or(usize::MAX),
        )
        .map_err(|error| ApiError::InvalidPath(error.to_string()))?;

        let mut matches = Vec::new();
        for hit in hits {
            let relative = RelativePath::new(hit.relative_path)
                .map_err(|_| ApiError::InvalidPath("invalid path".to_owned()))?;
            if self
                .policy_engine
                .resolve_read_path(&mount, &relative, None)
                .is_err()
            {
                continue;
            }

            matches.push(GrepMatch {
                path: MountPath::new(mount.name.clone(), relative),
                line: hit.line,
                content: hit.content,
            });
        }

        Ok(GrepResponse {
            pattern: pattern.to_owned(),
            matches,
        })
    }

    pub async fn find(
        &self,
        path: &MountPath,
        glob: &str,
        max_results: u64,
    ) -> Result<FindResponse, ApiError> {
        let mount = self.load_mount(path.mount()).await?;
        let start = self
            .policy_engine
            .resolve_read_path(&mount, path.relative(), None)
            .map_err(ApiError::from)?;
        let root = canonical_mount_root(&mount)?;

        let hits = find::find(
            &root,
            &start,
            glob,
            usize::try_from(max_results).unwrap_or(usize::MAX),
        )
        .map_err(|error| ApiError::InvalidPath(error.to_string()))?;

        let mut results = Vec::new();
        for hit in hits {
            let relative = RelativePath::new(hit.relative_path)
                .map_err(|_| ApiError::InvalidPath("invalid path".to_owned()))?;
            let Ok(resolved) = self
                .policy_engine
                .resolve_read_path(&mount, &relative, None)
            else {
                continue;
            };

            let metadata = tokio::fs::metadata(&resolved)
                .await
                .map_err(|error| ApiError::Internal(error.to_string()))?;
            results.push(FindResult {
                path: MountPath::new(mount.name.clone(), relative),
                size: if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
                modified_at: modified_at(&metadata),
            });
        }

        Ok(FindResponse {
            glob: glob.to_owned(),
            results,
        })
    }

    pub async fn load_mount(&self, name: &MountName) -> Result<Mount, ApiError> {
        self.mount_repository
            .find(name)
            .await
            .map_err(|error| match error {
                memo_core::DbError::NotFound => ApiError::MountNotFound(name.to_string()),
                other => ApiError::from(other),
            })
    }
}

pub fn require_fs_read_scope(token: &Token, mount: &MountName) -> bool {
    token.has_scope(&Scope::Fs {
        mount: ScopeMount::Named(mount.clone()),
        action: FsAction::Read,
    }) || token.has_scope(&Scope::Fs {
        mount: ScopeMount::Any,
        action: FsAction::Read,
    })
}

pub fn require_fs_write_scope(token: &Token, mount: &MountName) -> bool {
    token.has_scope(&Scope::Fs {
        mount: ScopeMount::Named(mount.clone()),
        action: FsAction::Write,
    }) || token.has_scope(&Scope::Fs {
        mount: ScopeMount::Any,
        action: FsAction::Write,
    })
}

fn to_relative_path(root: &Path, absolute: &Path) -> Option<RelativePath> {
    let stripped = absolute.strip_prefix(root).ok()?;
    let value = stripped.to_string_lossy().replace('\\', "/");
    RelativePath::new(value).ok()
}

fn canonical_mount_root(mount: &Mount) -> Result<PathBuf, ApiError> {
    mount
        .root_path
        .canonicalize()
        .map_err(|_| ApiError::InvalidPath("invalid mount root".to_owned()))
}

fn kind_from_metadata(metadata: &std::fs::Metadata) -> FsEntryKind {
    if metadata.is_dir() {
        FsEntryKind::Dir
    } else {
        FsEntryKind::File
    }
}

fn kind_from_metadata_std(metadata: &std::fs::Metadata) -> FsEntryKind {
    if metadata.is_dir() {
        FsEntryKind::Dir
    } else {
        FsEntryKind::File
    }
}

fn modified_at(metadata: &std::fs::Metadata) -> OffsetDateTime {
    metadata
        .modified()
        .map_or_else(|_| OffsetDateTime::now_utc(), OffsetDateTime::from)
}

fn modified_at_std(metadata: &std::fs::Metadata) -> OffsetDateTime {
    metadata
        .modified()
        .map_or_else(|_| OffsetDateTime::now_utc(), OffsetDateTime::from)
}

fn created_at(metadata: &std::fs::Metadata) -> Option<OffsetDateTime> {
    metadata.created().ok().map(OffsetDateTime::from)
}

fn parse_summary_from_index(index_path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(index_path).ok()?;
    parse_frontmatter_summary(&text)
}

fn parse_frontmatter_summary(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next()? != "---" {
        return None;
    }

    for line in lines {
        if line == "---" {
            break;
        }

        if let Some(value) = line.strip_prefix("summary:") {
            return Some(value.trim().trim_matches('"').to_owned());
        }
    }

    None
}

fn mime_from_path(path: &Path) -> String {
    match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("md") => "text/markdown; charset=utf-8".to_owned(),
        Some("txt") => "text/plain; charset=utf-8".to_owned(),
        Some("json") => "application/json".to_owned(),
        Some("html") => "text/html; charset=utf-8".to_owned(),
        Some("csv") => "text/csv; charset=utf-8".to_owned(),
        _ => "application/octet-stream".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::Arc;

    use memo_core::{
        ApiError, Audience, Expiry, Mount, MountMode, MountName, MountPath, MountPolicy, Scope,
        ScopeSet, Token, TokenId,
    };
    use tempfile::tempdir;
    use time::OffsetDateTime;

    use crate::db::{init_pool, DbConfig};
    use crate::fs::ops::FileSystemService;
    use crate::mount_registry::repository::{PolicyCache, SqliteMountRepository};

    fn sample_token(scopes: &[&str]) -> Result<Token, Box<dyn std::error::Error>> {
        let parsed = scopes
            .iter()
            .map(|scope| Scope::from_str(scope))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Token::new(
            TokenId::new(),
            "agent",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new(parsed),
            OffsetDateTime::now_utc(),
            Expiry::Never,
        )?)
    }

    async fn service_with_mount(
        root: PathBuf,
    ) -> Result<FileSystemService, Box<dyn std::error::Error>> {
        let tempdir = tempdir()?;
        let db_url = format!("sqlite://{}", tempdir.path().join("memo.db").display());
        let pool = init_pool(&DbConfig::new(db_url)).await?;
        let repository = Arc::new(SqliteMountRepository::new(pool, PolicyCache::new()));

        let mount = Mount::new(
            MountName::new("VaultKB")?,
            root,
            MountMode::ReadWrite,
            Audience::Shared,
            None,
            MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )?;
        repository.create(&mount).await?;

        Ok(FileSystemService::new(repository))
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let root_dir = tempdir()?;
        let service = service_with_mount(root_dir.path().to_path_buf()).await?;
        let path = MountPath::parse("VaultKB:/notes/a.md")?;

        let write = service.write_file(&path, b"hello").await?;
        assert_eq!(write.written_bytes, 5);

        let (resolved, _, _) = service.read_file_meta(&path).await?;
        let bytes = tokio::fs::read(resolved).await?;
        assert_eq!(bytes, b"hello");

        Ok(())
    }

    #[tokio::test]
    async fn rm_non_empty_dir_without_recursive_conflicts() -> Result<(), Box<dyn std::error::Error>>
    {
        let root_dir = tempdir()?;
        let notes = root_dir.path().join("notes");
        std::fs::create_dir_all(&notes)?;
        std::fs::write(notes.join("a.md"), b"x")?;

        let service = service_with_mount(root_dir.path().to_path_buf()).await?;
        let path = MountPath::parse("VaultKB:/notes")?;

        let result = service.rm(&path, false).await;
        assert!(matches!(result, Err(ApiError::Conflict(_))));

        Ok(())
    }

    #[test]
    fn fs_scope_helpers_match_expected_scopes() -> Result<(), Box<dyn std::error::Error>> {
        let mount = MountName::new("VaultKB")?;
        let read_token = sample_token(&["fs:*:read"])?;
        let write_token = sample_token(&["fs:VaultKB:write"])?;

        assert!(super::require_fs_read_scope(&read_token, &mount));
        assert!(!super::require_fs_write_scope(&read_token, &mount));
        assert!(super::require_fs_write_scope(&write_token, &mount));

        Ok(())
    }

    #[test]
    fn parses_frontmatter_summary() {
        let text = "---\nsummary: test value\n---\nbody";
        assert_eq!(
            super::parse_frontmatter_summary(text).as_deref(),
            Some("test value")
        );
    }
}
