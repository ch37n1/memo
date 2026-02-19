use std::io::Write;
use std::str::FromStr;

use memo_client::{
    AuditQuery, AuditResponse, AuditResult, CreateMountRequest, CreateTokenRequest, MemoClient,
    PatchValue, UpdateMountRequest,
};
use memo_core::{Audience, Mount, MountMode, MountName, Scope, ScopeSet, TokenId, TokenView};
use serde::Serialize;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::cli::{
    AuditArgs, MountAddArgs, MountCommand, MountRemoveArgs, MountShowArgs, MountUpdateArgs,
    TokenCommand, TokenCreateArgs, TokenRevokeArgs,
};
use crate::config::RuntimeConfig;
use crate::error::CliError;

pub async fn run_mount(command: &MountCommand, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;

    match command {
        MountCommand::List => {
            let mounts = client
                .list_mounts()
                .await
                .map_err(CliError::from_client_error)?;

            if runtime.json {
                emit_json(&json!({ "mounts": mounts }))
            } else {
                emit_mount_list(&mounts)
            }
        }
        MountCommand::Add(args) => add_mount(&client, args, runtime).await,
        MountCommand::Remove(args) => remove_mount(&client, args, runtime).await,
        MountCommand::Show(args) => show_mount(&client, args, runtime).await,
        MountCommand::Update(args) => update_mount(&client, args, runtime).await,
    }
}

pub async fn run_token(command: &TokenCommand, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;

    match command {
        TokenCommand::List => {
            let tokens = client
                .list_tokens()
                .await
                .map_err(CliError::from_client_error)?;

            if runtime.json {
                emit_json(&json!({ "tokens": tokens }))
            } else {
                emit_token_list(&tokens)
            }
        }
        TokenCommand::Create(args) => create_token(&client, args, runtime).await,
        TokenCommand::Revoke(args) => revoke_token(&client, args, runtime).await,
    }
}

pub async fn run_audit(args: &AuditArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let client = client(runtime)?;
    let query = build_audit_query(args)?;
    let response = client
        .query_audit(&query)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&response)
    } else {
        emit_audit(&response)
    }
}

async fn add_mount(
    client: &MemoClient,
    args: &MountAddArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let request = CreateMountRequest {
        name: parse_mount_name(&args.name)?,
        root_path: args.path.clone(),
        mode: parse_mount_mode(&args.mode)?,
        audience: parse_audience(&args.audience)?,
        description: args.description.clone(),
        hide_globs: args.hide_glob.clone(),
        deny_read_globs: args.deny_read_glob.clone(),
        deny_write_globs: args.deny_write_glob.clone(),
        max_read_bytes: args.max_read_bytes,
        max_write_bytes: args.max_write_bytes,
    };

    let mount = client
        .create_mount(&request)
        .await
        .map_err(CliError::from_client_error)?;

    emit_mount(runtime, &mount)
}

async fn remove_mount(
    client: &MemoClient,
    args: &MountRemoveArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let name = parse_mount_name(&args.name)?;
    let removed = client
        .remove_mount(&name)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&removed)
    } else {
        emit_plain(&format!("removed mount {}", removed.name))
    }
}

async fn show_mount(
    client: &MemoClient,
    args: &MountShowArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let name = parse_mount_name(&args.name)?;
    let mount = client
        .get_mount(&name)
        .await
        .map_err(CliError::from_client_error)?;

    emit_mount(runtime, &mount)
}

async fn update_mount(
    client: &MemoClient,
    args: &MountUpdateArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let name = parse_mount_name(&args.name)?;
    let request = UpdateMountRequest {
        mode: args.mode.as_deref().map(parse_mount_mode).transpose()?,
        audience: args.audience.as_deref().map(parse_audience).transpose()?,
        description: patch_string(args.description.clone(), args.clear_description),
        hide_globs: patch_globs(args.hide_glob.clone(), args.clear_hide_globs),
        deny_read_globs: patch_globs(args.deny_read_glob.clone(), args.clear_deny_read_globs),
        deny_write_globs: patch_globs(args.deny_write_glob.clone(), args.clear_deny_write_globs),
        max_read_bytes: patch_u64(args.max_read_bytes, args.clear_max_read_bytes),
        max_write_bytes: patch_u64(args.max_write_bytes, args.clear_max_write_bytes),
    };

    let mount = client
        .update_mount(&name, &request)
        .await
        .map_err(CliError::from_client_error)?;

    emit_mount(runtime, &mount)
}

async fn create_token(
    client: &MemoClient,
    args: &TokenCreateArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let scopes = parse_scopes(&args.scopes)?;
    let expires_at = args.expires.as_deref().map(parse_timestamp).transpose()?;

    let created = client
        .create_token(&CreateTokenRequest {
            name: args.name.clone(),
            scopes,
            expires_at,
        })
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&created)
    } else {
        emit_plain(&format!(
            "created token {} ({})",
            created.name,
            created.id.into_uuid()
        ))?;
        emit_plain("token:")?;
        emit_plain(&created.token)?;
        emit_plain("store this token now; it may not be shown again")
    }
}

async fn revoke_token(
    client: &MemoClient,
    args: &TokenRevokeArgs,
    runtime: &RuntimeConfig,
) -> Result<(), CliError> {
    let id = parse_token_id(&args.id)?;
    let revoked = client
        .revoke_token(id)
        .await
        .map_err(CliError::from_client_error)?;

    if runtime.json {
        emit_json(&revoked)
    } else {
        emit_plain(&format!("revoked token {}", revoked.id.into_uuid()))
    }
}

fn build_audit_query(args: &AuditArgs) -> Result<AuditQuery, CliError> {
    Ok(AuditQuery {
        mount: args.mount.as_deref().map(parse_mount_name).transpose()?,
        token_id: args.token_id.as_deref().map(parse_token_id).transpose()?,
        operation: args.operation.clone(),
        result: args.result.as_deref().map(parse_audit_result).transpose()?,
        limit: args.limit,
        before: args.before.as_deref().map(parse_timestamp).transpose()?,
        after: args.after.as_deref().map(parse_timestamp).transpose()?,
        after_id: None,
    })
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

fn parse_mount_name(input: &str) -> Result<MountName, CliError> {
    MountName::new(input).map_err(|error| CliError::Command(error.to_string()))
}

fn parse_mount_mode(input: &str) -> Result<MountMode, CliError> {
    match input {
        "read_only" | "readonly" | "ro" => Ok(MountMode::ReadOnly),
        "read_write" | "readwrite" | "rw" => Ok(MountMode::ReadWrite),
        _ => Err(CliError::Command(format!(
            "invalid --mode '{input}' (expected read_only or read_write)"
        ))),
    }
}

fn parse_audience(input: &str) -> Result<Audience, CliError> {
    match input {
        "shared" => Ok(Audience::Shared),
        "agent_only" | "agent-only" => Ok(Audience::AgentOnly),
        "human_only" | "human-only" => Ok(Audience::HumanOnly),
        _ => Err(CliError::Command(format!(
            "invalid --audience '{input}' (expected shared, agent_only, or human_only)"
        ))),
    }
}

fn parse_scopes(inputs: &[String]) -> Result<ScopeSet, CliError> {
    let parsed = inputs
        .iter()
        .map(|scope| Scope::from_str(scope).map_err(|error| CliError::Command(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    if parsed.is_empty() {
        return Err(CliError::Command("--scopes must not be empty".to_owned()));
    }

    Ok(ScopeSet::new(parsed))
}

fn parse_token_id(input: &str) -> Result<TokenId, CliError> {
    let uuid = Uuid::parse_str(input)
        .map_err(|error| CliError::Command(format!("invalid token id '{input}': {error}")))?;
    Ok(TokenId::from_uuid(uuid))
}

fn parse_timestamp(input: &str) -> Result<OffsetDateTime, CliError> {
    OffsetDateTime::parse(input, &Rfc3339).map_err(|error| {
        CliError::Command(format!(
            "invalid timestamp '{input}' (expected RFC3339): {error}"
        ))
    })
}

fn parse_audit_result(input: &str) -> Result<AuditResult, CliError> {
    match input {
        "ok" => Ok(AuditResult::Ok),
        "error" => Ok(AuditResult::Error),
        _ => Err(CliError::Command(format!(
            "invalid --result '{input}' (expected ok or error)"
        ))),
    }
}

fn patch_globs(value: Option<Vec<String>>, clear: bool) -> Option<Vec<String>> {
    if clear {
        Some(Vec::new())
    } else {
        value
    }
}

fn patch_string(value: Option<String>, clear: bool) -> Option<PatchValue<String>> {
    if clear {
        Some(PatchValue::Null)
    } else {
        value.map(PatchValue::Value)
    }
}

fn patch_u64(value: Option<u64>, clear: bool) -> Option<PatchValue<u64>> {
    if clear {
        Some(PatchValue::Null)
    } else {
        value.map(PatchValue::Value)
    }
}

fn emit_mount(runtime: &RuntimeConfig, mount: &Mount) -> Result<(), CliError> {
    if runtime.json {
        emit_json(mount)
    } else {
        emit_plain(&format!(
            "{} {} {} {}",
            mount.name,
            mount.root_path.display(),
            format_mode(&mount.mode),
            format_audience(&mount.audience)
        ))
    }
}

fn emit_mount_list(mounts: &[Mount]) -> Result<(), CliError> {
    for mount in mounts {
        emit_plain(&format!(
            "{} {} {} {}",
            mount.name,
            mount.root_path.display(),
            format_mode(&mount.mode),
            format_audience(&mount.audience)
        ))?;
    }
    Ok(())
}

fn emit_token_list(tokens: &[TokenView]) -> Result<(), CliError> {
    for token in tokens {
        emit_plain(&format!(
            "{} {} scopes={}",
            token.id.into_uuid(),
            token.name,
            format_scopes(&token.scopes)
        ))?;
    }
    Ok(())
}

fn emit_audit(response: &AuditResponse) -> Result<(), CliError> {
    for entry in &response.entries {
        let mount = entry
            .mount
            .as_ref()
            .map_or_else(|| "-".to_owned(), ToString::to_string);
        let token = entry
            .token_id
            .map_or_else(|| "-".to_owned(), |id| id.into_uuid().to_string());
        emit_plain(&format!(
            "{} {} {} mount={} token={} result={:?}",
            entry.id, entry.timestamp, entry.operation, mount, token, entry.result
        ))?;
    }
    Ok(())
}

fn format_mode(mode: &MountMode) -> &'static str {
    match mode {
        MountMode::ReadOnly => "read_only",
        MountMode::ReadWrite => "read_write",
    }
}

fn format_audience(audience: &Audience) -> &'static str {
    match audience {
        Audience::Shared => "shared",
        Audience::AgentOnly => "agent_only",
        Audience::HumanOnly => "human_only",
    }
}

fn format_scopes(scopes: &ScopeSet) -> String {
    scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn emit_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let line =
        serde_json::to_string(value).map_err(|error| CliError::Command(error.to_string()))?;
    emit_plain(&line)
}

fn emit_plain(line: &str) -> Result<(), CliError> {
    writeln!(std::io::stdout(), "{line}").map_err(CliError::from)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::PathBuf;

    use memo_client::{AuditEntry, AuditResult, PatchValue};
    use memo_core::{Expiry, Scope};
    use time::OffsetDateTime;

    use super::*;

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

    fn sample_mount() -> Mount {
        Mount::new(
            MountName::new("VaultKB").expect("mount name should parse"),
            PathBuf::from("/tmp"),
            MountMode::ReadWrite,
            Audience::Shared,
            Some("shared knowledge base".to_owned()),
            memo_core::MountPolicy::default(),
            OffsetDateTime::now_utc(),
        )
        .expect("mount should construct")
    }

    fn sample_token_view() -> TokenView {
        TokenView {
            id: TokenId::new(),
            name: "agent".to_owned(),
            scopes: ScopeSet::new([Scope::from_str("meta:*:read").expect("scope should parse")]),
            created_at: OffsetDateTime::now_utc(),
            expires_at: Expiry::Never,
            last_used_at: None,
        }
    }

    #[test]
    fn parse_mount_mode_accepts_aliases() {
        assert_eq!(
            parse_mount_mode("read_only").expect("mode should parse"),
            MountMode::ReadOnly
        );
        assert_eq!(
            parse_mount_mode("readonly").expect("mode should parse"),
            MountMode::ReadOnly
        );
        assert_eq!(
            parse_mount_mode("ro").expect("mode should parse"),
            MountMode::ReadOnly
        );
        assert_eq!(
            parse_mount_mode("read_write").expect("mode should parse"),
            MountMode::ReadWrite
        );
        assert_eq!(
            parse_mount_mode("readwrite").expect("mode should parse"),
            MountMode::ReadWrite
        );
        assert_eq!(
            parse_mount_mode("rw").expect("mode should parse"),
            MountMode::ReadWrite
        );
        assert!(parse_mount_mode("invalid").is_err());
    }

    #[test]
    fn parse_audience_accepts_aliases() {
        assert_eq!(
            parse_audience("shared").expect("audience should parse"),
            Audience::Shared
        );
        assert_eq!(
            parse_audience("agent_only").expect("audience should parse"),
            Audience::AgentOnly
        );
        assert_eq!(
            parse_audience("agent-only").expect("audience should parse"),
            Audience::AgentOnly
        );
        assert_eq!(
            parse_audience("human_only").expect("audience should parse"),
            Audience::HumanOnly
        );
        assert_eq!(
            parse_audience("human-only").expect("audience should parse"),
            Audience::HumanOnly
        );
        assert!(parse_audience("robots").is_err());
    }

    #[test]
    fn parse_scope_and_token_inputs() {
        let scopes = parse_scopes(&["meta:*:read".to_owned(), "fs:VaultKB:read".to_owned()])
            .expect("scopes should parse");
        assert_eq!(scopes.len(), 2);
        assert!(parse_scopes(&["broken".to_owned()]).is_err());
        assert!(parse_scopes(&[]).is_err());

        assert!(parse_mount_name("VaultKB").is_ok());
        assert!(parse_mount_name("with space").is_err());

        let id = TokenId::new().into_uuid().to_string();
        assert_eq!(
            parse_token_id(&id)
                .expect("token id should parse")
                .into_uuid()
                .to_string(),
            id
        );
        assert!(parse_token_id("not-a-uuid").is_err());
    }

    #[test]
    fn parse_time_and_audit_result_inputs() {
        let ts = "2026-02-19T12:34:56Z";
        assert!(parse_timestamp(ts).is_ok());
        assert!(parse_timestamp("not-time").is_err());

        assert_eq!(
            parse_audit_result("ok").expect("audit result should parse"),
            AuditResult::Ok
        );
        assert_eq!(
            parse_audit_result("error").expect("audit result should parse"),
            AuditResult::Error
        );
        assert!(parse_audit_result("warn").is_err());
    }

    #[test]
    fn patch_helpers_apply_clear_semantics() {
        assert_eq!(
            patch_globs(Some(vec!["a".to_owned()]), false),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(patch_globs(None, true), Some(Vec::new()));

        assert_eq!(
            patch_string(Some("x".to_owned()), false),
            Some(PatchValue::Value("x".to_owned()))
        );
        assert_eq!(patch_string(None, true), Some(PatchValue::Null));

        assert_eq!(patch_u64(Some(42), false), Some(PatchValue::Value(42)));
        assert_eq!(patch_u64(None, true), Some(PatchValue::Null));
    }

    #[test]
    fn build_audit_query_parses_all_fields() {
        let token_id = TokenId::new().into_uuid().to_string();
        let args = AuditArgs {
            mount: Some("VaultKB".to_owned()),
            token_id: Some(token_id.clone()),
            operation: Some("read".to_owned()),
            result: Some("ok".to_owned()),
            limit: Some(50),
            before: Some("2026-02-19T00:00:00Z".to_owned()),
            after: Some("2026-02-18T00:00:00Z".to_owned()),
        };
        let query = build_audit_query(&args).expect("audit query should parse");

        assert_eq!(
            query.mount.expect("mount should be present").to_string(),
            "VaultKB"
        );
        assert_eq!(
            query
                .token_id
                .expect("token should be present")
                .into_uuid()
                .to_string(),
            token_id
        );
        assert_eq!(query.operation.as_deref(), Some("read"));
        assert_eq!(query.result, Some(AuditResult::Ok));
        assert_eq!(query.limit, Some(50));
        assert!(query.before.is_some());
        assert!(query.after.is_some());
        assert_eq!(query.after_id, None);
    }

    #[test]
    fn formatting_helpers_are_stable() {
        let scopes = ScopeSet::new([
            Scope::from_str("meta:*:read").expect("scope should parse"),
            Scope::from_str("admin:*:tokens").expect("scope should parse"),
        ]);
        assert_eq!(format_mode(&MountMode::ReadOnly), "read_only");
        assert_eq!(format_mode(&MountMode::ReadWrite), "read_write");
        assert_eq!(format_audience(&Audience::Shared), "shared");
        assert_eq!(format_audience(&Audience::AgentOnly), "agent_only");
        assert_eq!(format_audience(&Audience::HumanOnly), "human_only");

        let rendered = format_scopes(&scopes);
        assert!(rendered.contains("meta:*:read"));
        assert!(rendered.contains("admin:*:tokens"));
    }

    #[test]
    fn emit_helpers_return_ok() {
        let mount = sample_mount();
        let token = sample_token_view();
        let response = AuditResponse {
            entries: vec![AuditEntry {
                id: 1,
                timestamp: OffsetDateTime::now_utc(),
                token_id: Some(TokenId::new()),
                operation: "read".to_owned(),
                mount: Some(MountName::new("VaultKB").expect("mount should parse")),
                path: Some("notes.md".to_owned()),
                result: AuditResult::Ok,
                error_code: None,
            }],
        };

        assert!(emit_mount(&runtime(true), &mount).is_ok());
        assert!(emit_mount(&runtime(false), &mount).is_ok());
        assert!(emit_mount_list(&[mount]).is_ok());
        assert!(emit_token_list(&[token]).is_ok());
        assert!(emit_audit(&response).is_ok());
        assert!(emit_json(&serde_json::json!({ "ok": true })).is_ok());
        assert!(emit_plain("ok").is_ok());
    }

    #[test]
    fn client_builder_validates_base_url() {
        let ok_runtime = RuntimeConfig {
            host: "127.0.0.1".to_owned(),
            token: Some("memo_token".to_owned()),
            ..runtime(true)
        };
        let built_client = client(&ok_runtime).expect("client should build");
        assert!(built_client.has_token());

        let bad_runtime = RuntimeConfig {
            host: "bad host".to_owned(),
            ..runtime(false)
        };
        assert!(client(&bad_runtime).is_err());
    }

    #[tokio::test]
    async fn run_functions_fail_fast_on_input_validation() {
        let cfg = runtime(false);

        let add = MountCommand::Add(MountAddArgs {
            name: "VaultKB".to_owned(),
            path: PathBuf::from("/tmp"),
            mode: "bad".to_owned(),
            audience: "shared".to_owned(),
            description: None,
            hide_glob: Vec::new(),
            deny_read_glob: Vec::new(),
            deny_write_glob: Vec::new(),
            max_read_bytes: None,
            max_write_bytes: None,
        });
        assert!(run_mount(&add, &cfg).await.is_err());

        let remove = MountCommand::Remove(MountRemoveArgs {
            name: "bad name".to_owned(),
        });
        assert!(run_mount(&remove, &cfg).await.is_err());

        let update = MountCommand::Update(MountUpdateArgs {
            name: "VaultKB".to_owned(),
            mode: None,
            audience: Some("invalid".to_owned()),
            description: None,
            clear_description: false,
            hide_glob: None,
            clear_hide_globs: false,
            deny_read_glob: None,
            clear_deny_read_globs: false,
            deny_write_glob: None,
            clear_deny_write_globs: false,
            max_read_bytes: None,
            clear_max_read_bytes: false,
            max_write_bytes: None,
            clear_max_write_bytes: false,
        });
        assert!(run_mount(&update, &cfg).await.is_err());

        let create = TokenCommand::Create(TokenCreateArgs {
            name: "agent".to_owned(),
            scopes: vec!["bad_scope".to_owned()],
            expires: None,
        });
        assert!(run_token(&create, &cfg).await.is_err());

        let revoke = TokenCommand::Revoke(TokenRevokeArgs {
            id: "bad".to_owned(),
        });
        assert!(run_token(&revoke, &cfg).await.is_err());

        let audit = AuditArgs {
            mount: None,
            token_id: None,
            operation: None,
            result: Some("bad".to_owned()),
            limit: None,
            before: None,
            after: None,
        };
        assert!(run_audit(&audit, &cfg).await.is_err());
    }
}
