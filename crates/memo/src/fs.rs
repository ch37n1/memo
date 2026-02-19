use std::fs;
use std::io::Write;
use std::path::Path;

use base64::Engine;
use memo_client::{
    CopyResponse, FindResponse, FsEntryKind, GrepResponse, LsResponse, MemoClient, StatResponse,
    TreeNode, TreeResponse,
};
use memo_core::{MountName, MountPath, RelativePath};
use serde::Serialize;
use serde_json::json;

use crate::cli::{
    FsCatArgs, FsCpArgs, FsFindArgs, FsGrepArgs, FsInfoArgs, FsLsArgs, FsMkdirArgs, FsMvArgs,
    FsRmArgs, FsTreeArgs, FsWriteArgs,
};
use crate::config::RuntimeConfig;
use crate::error::CliError;

pub async fn run_ls(args: &FsLsArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .ls(&path, args.info.then_some(true))
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_ls(&response)
    }
}

pub async fn run_tree(args: &FsTreeArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .tree(&path, args.depth)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_tree(&response)
    }
}

pub async fn run_cat(args: &FsCatArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let bytes = client
        .read(&path)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&json!({
            "path": path,
            "content_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }))
    } else {
        write_stdout_bytes(&bytes)
    }
}

pub async fn run_write(args: &FsWriteArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let bytes = read_write_input(args.file.as_deref())?;
    let response = client
        .write_bytes(&path, bytes)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_plain(&format!(
            "wrote {} bytes to {}",
            response.written_bytes, response.path
        ))
    }
}

pub async fn run_mkdir(args: &FsMkdirArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .mkdir(&path)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_plain(&format!("created {}", response.path))
    }
}

pub async fn run_mv(args: &FsMvArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let src = parse_mount_path(&args.src, runtime.default_mount.as_deref())?;
    let dst = parse_mount_path(&args.dst, runtime.default_mount.as_deref())?;
    let response = client
        .mv(&src, &dst)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_plain(&format!("moved {} -> {}", response.src, response.dst))
    }
}

pub async fn run_rm(args: &FsRmArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .rm(&path, args.recursive.then_some(true))
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_plain(&format!("removed {}", response.path))
    }
}

pub async fn run_cp(args: &FsCpArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let dst = parse_mount_path(&args.dst, runtime.default_mount.as_deref())?;

    if let Ok(src_mount) = parse_mount_path(&args.src, runtime.default_mount.as_deref()) {
        let response = client
            .cp(&src_mount, &dst)
            .await
            .map_err(CliError::from_client_error)?;

        return emit_cp(runtime, &response);
    }

    let src_local = Path::new(&args.src);
    if !src_local.exists() {
        return Err(CliError::Command(format!(
            "source '{}' is neither a valid mount path nor an existing local file",
            args.src
        )));
    }
    if src_local.is_dir() {
        return Err(CliError::Command(format!(
            "local source '{}' is a directory; only file -> mount copy is supported",
            args.src
        )));
    }

    let bytes = fs::read(src_local)?;
    let write_response = client
        .write_bytes(&dst, bytes)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&json!({
            "src": args.src,
            "dst": write_response.path,
            "written_bytes": write_response.written_bytes
        }))
    } else {
        emit_plain(&format!(
            "copied local file {} -> {} ({} bytes)",
            args.src, write_response.path, write_response.written_bytes
        ))
    }
}

pub async fn run_grep(args: &FsGrepArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let recursive = (!args.no_recursive).then_some(true);
    let case_sensitive = args.case_insensitive.then_some(false);

    let response = client
        .grep(
            &path,
            &args.pattern,
            recursive,
            case_sensitive,
            args.max_results,
        )
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_grep(&response)
    }
}

pub async fn run_find(args: &FsFindArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .find(&path, &args.glob, args.max_results)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_find(&response)
    }
}

pub async fn run_info(args: &FsInfoArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let path = parse_mount_path(&args.path, runtime.default_mount.as_deref())?;
    let response = client
        .stat(&path)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_info(&response)
    }
}

fn parse_mount_path(input: &str, default_mount: Option<&str>) -> Result<MountPath, CliError> {
    if input.contains(":/") {
        return MountPath::parse(input).map_err(|error| CliError::Command(error.to_string()));
    }

    let default_mount = default_mount.ok_or_else(|| {
        CliError::Command(
            "mount path must use '<mount>:/path' or set global --mount for relative paths"
                .to_owned(),
        )
    })?;

    let mount =
        MountName::new(default_mount).map_err(|error| CliError::Command(error.to_string()))?;
    let relative_input = input.trim_start_matches('/');
    let relative =
        RelativePath::new(relative_input).map_err(|error| CliError::Command(error.to_string()))?;
    Ok(MountPath::new(mount, relative))
}

fn read_write_input(file: Option<&Path>) -> Result<Vec<u8>, CliError> {
    if let Some(path) = file {
        return fs::read(path).map_err(CliError::from);
    }

    let mut bytes = Vec::new();
    let mut stdin = std::io::stdin().lock();
    std::io::Read::read_to_end(&mut stdin, &mut bytes)?;
    Ok(bytes)
}

fn client(runtime: &RuntimeConfig) -> Result<MemoClient, CliError> {
    let base_client =
        MemoClient::for_base_url(runtime.base_url()).map_err(CliError::from_client_error)?;
    Ok(if let Some(token) = &runtime.token {
        base_client.with_token(token.clone())
    } else {
        base_client
    })
}

fn emit_ls(response: &LsResponse) -> Result<(), CliError> {
    for entry in &response.entries {
        let kind = match entry.kind {
            FsEntryKind::Dir => "d",
            FsEntryKind::File => "f",
        };
        let size = entry
            .size
            .map_or_else(|| "-".to_owned(), |value| value.to_string());
        emit_plain(&format!(
            "{}\t{}\t{}\t{}",
            kind, entry.name, size, entry.modified_at
        ))?;
    }
    Ok(())
}

fn emit_tree(response: &TreeResponse) -> Result<(), CliError> {
    emit_plain(&response.path.to_string())?;
    for child in &response.tree.children {
        emit_tree_node(child, "", true)?;
    }
    if response.truncated {
        emit_plain("(truncated)")?;
    }
    Ok(())
}

fn emit_tree_node(node: &TreeNode, prefix: &str, is_last: bool) -> Result<(), CliError> {
    let branch = if is_last { "└──" } else { "├──" };
    emit_plain(&format!("{prefix}{branch} {}", node.name))?;

    let next_prefix = if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    let last_index = node.children.len().saturating_sub(1);
    for (idx, child) in node.children.iter().enumerate() {
        emit_tree_node(child, &next_prefix, idx == last_index)?;
    }
    Ok(())
}

fn emit_grep(response: &GrepResponse) -> Result<(), CliError> {
    for item in &response.matches {
        emit_plain(&format!("{}:{}:{}", item.path, item.line, item.content))?;
    }
    Ok(())
}

fn emit_find(response: &FindResponse) -> Result<(), CliError> {
    for item in &response.results {
        let size = item
            .size
            .map_or_else(|| "-".to_owned(), |value| value.to_string());
        emit_plain(&format!(
            "{}\tsize={}\t{}",
            item.path, size, item.modified_at
        ))?;
    }
    Ok(())
}

fn emit_info(response: &StatResponse) -> Result<(), CliError> {
    let kind = match response.kind {
        FsEntryKind::Dir => "dir",
        FsEntryKind::File => "file",
    };
    let size = response
        .size
        .map_or_else(|| "-".to_owned(), |value| value.to_string());
    emit_plain(&format!(
        "{}\tkind={}\tsize={}\tmodified={}",
        response.path, kind, size, response.modified_at
    ))?;
    if let Some(summary) = &response.memo_summary {
        emit_plain(&format!("summary: {summary}"))?;
    }
    Ok(())
}

fn emit_cp(runtime: &RuntimeConfig, response: &CopyResponse) -> Result<(), CliError> {
    if runtime.json {
        emit_json(response)
    } else {
        emit_plain(&format!("copied {} -> {}", response.src, response.dst))
    }
}

fn emit_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let line =
        serde_json::to_string(value).map_err(|error| CliError::Command(error.to_string()))?;
    emit_plain(&line)
}

fn emit_plain(line: &str) -> Result<(), CliError> {
    writeln!(std::io::stdout(), "{line}").map_err(CliError::from)
}

fn write_stdout_bytes(bytes: &[u8]) -> Result<(), CliError> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(bytes)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::PathBuf;

    use memo_client::{
        CopyResponse, FindResponse, FindResult, FsEntry, FsEntryKind, GrepMatch, GrepResponse,
        LsResponse, StatResponse, TreeNode, TreeResponse,
    };
    use memo_core::MountPath;
    use time::OffsetDateTime;

    use crate::config::RuntimeConfig;

    use super::{
        emit_cp, emit_find, emit_grep, emit_info, emit_ls, emit_tree, parse_mount_path,
        read_write_input, run_cat, run_cp, run_find, run_grep, run_info, run_ls, run_mkdir, run_mv,
        run_rm, run_tree, run_write, write_stdout_bytes,
    };
    use crate::cli::{
        FsCatArgs, FsCpArgs, FsFindArgs, FsGrepArgs, FsInfoArgs, FsLsArgs, FsMkdirArgs, FsMvArgs,
        FsRmArgs, FsTreeArgs, FsWriteArgs,
    };

    fn runtime(json: bool) -> RuntimeConfig {
        RuntimeConfig {
            json,
            host: "127.0.0.1".to_owned(),
            port: 18_301,
            token: None,
            default_mount: None,
            log_path: PathBuf::from("/tmp/memod.log"),
        }
    }

    fn sample_ts() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("timestamp should be valid")
    }

    #[test]
    fn parse_mount_path_accepts_explicit_mount() {
        let path = parse_mount_path("VaultKB:/notes", None).expect("path should parse");
        assert_eq!(path.to_string(), "VaultKB:/notes");
    }

    #[test]
    fn parse_mount_path_uses_default_mount_for_relative() {
        let path = parse_mount_path("notes/git.md", Some("VaultKB")).expect("path should parse");
        assert_eq!(path.to_string(), "VaultKB:/notes/git.md");
    }

    #[test]
    fn parse_mount_path_accepts_leading_slash_with_default_mount() {
        let path = parse_mount_path("/notes/git.md", Some("VaultKB")).expect("path should parse");
        assert_eq!(
            path,
            MountPath::parse("VaultKB:/notes/git.md").expect("valid path")
        );
    }

    #[test]
    fn parse_mount_path_rejects_relative_without_default_mount() {
        let error = parse_mount_path("notes/git.md", None).expect_err("path should fail");
        assert!(error.to_string().contains("mount path"));
    }

    #[test]
    fn parse_mount_path_rejects_invalid_default_mount_name() {
        let error =
            parse_mount_path("notes/git.md", Some("bad mount")).expect_err("path should fail");
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn read_write_input_reads_local_file() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let path = dir.path().join("note.md");
        std::fs::write(&path, b"hello").expect("file should write");
        let bytes = read_write_input(Some(&path)).expect("input should read");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn emit_helpers_accept_valid_payloads() {
        let ls = LsResponse {
            path: MountPath::parse("VaultKB:/notes").expect("path should parse"),
            entries: vec![FsEntry {
                name: "git.md".to_owned(),
                kind: FsEntryKind::File,
                size: Some(4),
                modified_at: sample_ts(),
            }],
        };
        assert!(emit_ls(&ls).is_ok());

        let tree = TreeResponse {
            path: MountPath::parse("VaultKB:/").expect("path should parse"),
            depth: 2,
            truncated: false,
            tree: TreeNode {
                name: String::new(),
                kind: FsEntryKind::Dir,
                size: None,
                modified_at: None,
                children: vec![TreeNode {
                    name: "notes".to_owned(),
                    kind: FsEntryKind::Dir,
                    size: None,
                    modified_at: Some(sample_ts()),
                    children: vec![TreeNode {
                        name: "git.md".to_owned(),
                        kind: FsEntryKind::File,
                        size: Some(4),
                        modified_at: Some(sample_ts()),
                        children: Vec::new(),
                    }],
                }],
            },
        };
        assert!(emit_tree(&tree).is_ok());

        let grep = GrepResponse {
            pattern: "git".to_owned(),
            matches: vec![GrepMatch {
                path: MountPath::parse("VaultKB:/notes/git.md").expect("path should parse"),
                line: 1,
                content: "git".to_owned(),
            }],
        };
        assert!(emit_grep(&grep).is_ok());

        let find = FindResponse {
            glob: "**/*.md".to_owned(),
            results: vec![FindResult {
                path: MountPath::parse("VaultKB:/notes/git.md").expect("path should parse"),
                size: Some(4),
                modified_at: sample_ts(),
            }],
        };
        assert!(emit_find(&find).is_ok());

        let stat = StatResponse {
            path: MountPath::parse("VaultKB:/notes/git.md").expect("path should parse"),
            kind: FsEntryKind::File,
            size: Some(4),
            modified_at: sample_ts(),
            created_at: Some(sample_ts()),
            memo_summary: Some("summary".to_owned()),
        };
        assert!(emit_info(&stat).is_ok());
    }

    #[test]
    fn emit_cp_supports_plain_and_json_modes() {
        let response = CopyResponse {
            src: MountPath::parse("VaultKB:/a").expect("path should parse"),
            dst: MountPath::parse("VaultKB:/b").expect("path should parse"),
        };
        assert!(emit_cp(&runtime(false), &response).is_ok());
        assert!(emit_cp(&runtime(true), &response).is_ok());
    }

    #[test]
    fn write_stdout_bytes_accepts_binary_data() {
        assert!(write_stdout_bytes(&[0, 1, 2, 3]).is_ok());
    }

    #[tokio::test]
    async fn run_commands_fail_fast_on_missing_mount_for_relative_path() {
        let cfg = runtime(false);

        assert!(run_ls(
            &FsLsArgs {
                path: "notes".to_owned(),
                info: false
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_tree(
            &FsTreeArgs {
                path: "notes".to_owned(),
                depth: None
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_cat(
            &FsCatArgs {
                path: "notes".to_owned()
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_write(
            &FsWriteArgs {
                path: "notes".to_owned(),
                file: None
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_mkdir(
            &FsMkdirArgs {
                path: "notes".to_owned()
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_mv(
            &FsMvArgs {
                src: "src".to_owned(),
                dst: "dst".to_owned()
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_rm(
            &FsRmArgs {
                path: "notes".to_owned(),
                recursive: false
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_grep(
            &FsGrepArgs {
                path: "notes".to_owned(),
                pattern: "x".to_owned(),
                no_recursive: false,
                case_insensitive: false,
                max_results: None
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_find(
            &FsFindArgs {
                path: "notes".to_owned(),
                glob: "*.md".to_owned(),
                max_results: None
            },
            &cfg
        )
        .await
        .is_err());
        assert!(run_info(
            &FsInfoArgs {
                path: "notes".to_owned()
            },
            &cfg
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn run_cp_rejects_invalid_local_sources_before_network_call() {
        let cfg = runtime(false);

        let missing = run_cp(
            &FsCpArgs {
                src: "/path/that/does/not/exist.md".to_owned(),
                dst: "VaultKB:/notes/out.md".to_owned(),
            },
            &cfg,
        )
        .await;
        assert!(missing.is_err());

        let dir = tempfile::tempdir().expect("temp dir should be created");
        let local_dir = dir.path().to_string_lossy().to_string();
        let dir_result = run_cp(
            &FsCpArgs {
                src: local_dir,
                dst: "VaultKB:/notes/out.md".to_owned(),
            },
            &cfg,
        )
        .await;
        assert!(dir_result.is_err());
    }
}
