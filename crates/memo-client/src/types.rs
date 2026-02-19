use std::path::PathBuf;

use memo_core::{
    Audience, Expiry, Mount, MountMode, MountName, MountPath, ScopeSet, TokenId, TokenView,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LsResponse {
    pub path: MountPath,
    pub entries: Vec<FsEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub kind: FsEntryKind,
    #[serde(default)]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsEntryKind {
    File,
    Dir,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeResponse {
    pub path: MountPath,
    pub depth: u8,
    pub truncated: bool,
    pub tree: TreeNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeNode {
    pub name: String,
    pub kind: FsEntryKind,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub modified_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatResponse {
    pub path: MountPath,
    pub kind: FsEntryKind,
    #[serde(default)]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
    #[serde(default)]
    pub created_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub memo_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteResponse {
    pub path: MountPath,
    pub written_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkdirResponse {
    pub path: MountPath,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveResponse {
    pub src: MountPath,
    pub dst: MountPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveResponse {
    pub path: MountPath,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyResponse {
    pub src: MountPath,
    pub dst: MountPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrepResponse {
    pub pattern: String,
    pub matches: Vec<GrepMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: MountPath,
    pub line: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindResponse {
    pub glob: String,
    pub results: Vec<FindResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindResult {
    pub path: MountPath,
    #[serde(default)]
    pub size: Option<u64>,
    pub modified_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountListResponse {
    pub mounts: Vec<Mount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateMountRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<MountMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<Audience>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<PatchValue<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_globs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_read_globs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deny_write_globs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_read_bytes: Option<PatchValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_write_bytes: Option<PatchValue<u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatchValue<T> {
    Value(T),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveMountResponse {
    pub name: MountName,
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub scopes: ScopeSet,
    #[serde(default)]
    pub expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedToken {
    pub id: TokenId,
    pub name: String,
    pub token: String,
    pub scopes: ScopeSet,
    pub created_at: OffsetDateTime,
    pub expires_at: Expiry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeTokenResponse {
    pub id: TokenId,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuditQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount: Option<MountName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_id: Option<TokenId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<AuditResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditResponse {
    pub entries: Vec<AuditEntry>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}
