use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use memo_client::{CreateMountRequest, CreateTokenRequest, MemoClient, MemoClientConfig};
use memo_core::{Audience, MountMode, MountName, Scope, ScopeSet};
use tempfile::TempDir;
use time::OffsetDateTime;

pub struct TestHarness {
    _tempdir: TempDir,
    child: Child,
    pub base_url: String,
    pub bootstrap_token: String,
    pub mount_root: PathBuf,
    pub archive_root: PathBuf,
}

impl TestHarness {
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let tempdir = tempfile::tempdir()?;
        let work = tempdir.path();
        let config_home = work.join("xdg-config");
        let data_home = work.join("xdg-data");
        let state_home = work.join("xdg-state");
        let runtime_dir = work.join("xdg-runtime");
        std::fs::create_dir_all(&config_home)?;
        std::fs::create_dir_all(&data_home)?;
        std::fs::create_dir_all(&state_home)?;
        std::fs::create_dir_all(&runtime_dir)?;

        let db_path = work.join("memo.db");
        let bootstrap_path = work.join("bootstrap.token");
        let mount_root = work.join("vault");
        let archive_root = work.join("archive");
        std::fs::create_dir_all(&mount_root)?;
        std::fs::create_dir_all(&archive_root)?;

        let port = reserve_port()?;
        let base_url = format!("http://127.0.0.1:{port}");

        let mut child = Command::new(env!("CARGO_BIN_EXE_memod"))
            .env("HOME", work)
            .env("XDG_CONFIG_HOME", &config_home)
            .env("XDG_DATA_HOME", &data_home)
            .env("XDG_STATE_HOME", &state_home)
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .env("MEMOD_BIND_ADDR", format!("127.0.0.1:{port}"))
            .env(
                "MEMOD_DATABASE_URL",
                format!("sqlite://{}", db_path.display()),
            )
            .env("MEMOD_BOOTSTRAP_TOKEN_PATH", &bootstrap_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        wait_until_ready(&mut child, &base_url, &bootstrap_path).await?;
        let bootstrap_token = std::fs::read_to_string(&bootstrap_path)?.trim().to_owned();

        Ok(Self {
            _tempdir: tempdir,
            child,
            base_url,
            bootstrap_token,
            mount_root,
            archive_root,
        })
    }

    pub fn admin_client(&self) -> Result<MemoClient, memo_client::MemoClientError> {
        MemoClient::new(MemoClientConfig {
            base_url: self.base_url.clone(),
            token: Some(self.bootstrap_token.clone()),
            ..MemoClientConfig::default()
        })
    }

    #[allow(dead_code)]
    pub fn client_with_token(
        &self,
        token: impl Into<String>,
    ) -> Result<MemoClient, memo_client::MemoClientError> {
        MemoClient::new(MemoClientConfig {
            base_url: self.base_url.clone(),
            token: Some(token.into()),
            ..MemoClientConfig::default()
        })
    }

    #[allow(dead_code)]
    pub async fn ensure_default_mounts(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.admin_client()?;
        ensure_mount(&client, "VaultKB", &self.mount_root, MountMode::ReadWrite).await?;
        ensure_mount(&client, "Archive", &self.archive_root, MountMode::ReadWrite).await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn create_token(
        &self,
        name: &str,
        scopes: &[&str],
        expires_at: Option<OffsetDateTime>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let client = self.admin_client()?;
        let parsed = scopes
            .iter()
            .map(|scope| scope.parse::<Scope>())
            .collect::<Result<Vec<_>, _>>()?;

        let created = client
            .create_token(&CreateTokenRequest {
                name: name.to_owned(),
                scopes: ScopeSet::new(parsed),
                expires_at,
            })
            .await?;

        Ok(created.token)
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[allow(dead_code)]
async fn ensure_mount(
    client: &MemoClient,
    name: &str,
    path: &Path,
    mode: MountMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let mount_name = MountName::new(name)?;

    if client.get_mount(&mount_name).await.is_ok() {
        return Ok(());
    }

    let _ = client
        .create_mount(&CreateMountRequest {
            name: mount_name,
            root_path: path.to_path_buf(),
            mode,
            audience: Audience::Shared,
            description: None,
            hide_globs: vec![],
            deny_read_globs: vec![],
            deny_write_globs: vec![],
            max_read_bytes: None,
            max_write_bytes: None,
        })
        .await?;

    Ok(())
}

fn reserve_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn wait_until_ready(
    child: &mut Child,
    base_url: &str,
    bootstrap_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = MemoClient::for_base_url(base_url)?;

    for _ in 0..200 {
        if let Some(status) = child.try_wait()? {
            return Err(format!("memod exited early with status {status}").into());
        }

        if bootstrap_path.exists() && client.health().await.is_ok() {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    Err("memod did not become ready in time".into())
}
