use std::io;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

use memo_client::{CreateMountRequest, MemoClient, MemoClientConfig};
use memo_core::{Audience, MountMode, MountName, Scope, ScopeSet};
use tempfile::TempDir;
use tokio::time::sleep;

type TestError = Box<dyn std::error::Error + Send + Sync>;

pub type TestResult<T = ()> = Result<T, TestError>;

pub struct TestDaemon {
    _workspace: TempDir,
    child: Child,
    pub base_url: String,
    pub admin_token: String,
    pub vault_root: PathBuf,
    pub archive_root: PathBuf,
}

impl TestDaemon {
    pub async fn spawn() -> TestResult<Self> {
        let workspace = tempfile::tempdir()?;
        let port = reserve_tcp_port()?;
        let bind_addr = format!("127.0.0.1:{port}");
        let base_url = format!("http://{bind_addr}");

        let db_url = format!("sqlite://{}", workspace.path().join("memo.db").display());
        let bootstrap_path = workspace.path().join("bootstrap.token");
        let vault_root = workspace.path().join("vault");
        let archive_root = workspace.path().join("archive");
        std::fs::create_dir_all(&vault_root)?;
        std::fs::create_dir_all(&archive_root)?;

        let child = Command::new(env!("CARGO_BIN_EXE_memod"))
            .env("MEMOD_BIND_ADDR", &bind_addr)
            .env("MEMOD_DATABASE_URL", &db_url)
            .env("MEMOD_BOOTSTRAP_TOKEN_PATH", &bootstrap_path)
            .env("MEMOD_WRITE_FSYNC", "false")
            .env("MEMOD_WRITE_DIR_SYNC", "false")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        wait_until_bootstrap_file(&bootstrap_path).await?;
        let admin_token = tokio::fs::read_to_string(&bootstrap_path)
            .await?
            .trim()
            .to_owned();
        wait_until_healthy(&base_url).await?;

        Ok(Self {
            _workspace: workspace,
            child,
            base_url,
            admin_token,
            vault_root,
            archive_root,
        })
    }

    pub fn admin_client(&self) -> TestResult<MemoClient> {
        MemoClient::new(MemoClientConfig {
            base_url: self.base_url.clone(),
            token: Some(self.admin_token.clone()),
            ..MemoClientConfig::default()
        })
        .map_err(Into::into)
    }

    #[allow(dead_code)]
    pub fn client_with_token(&self, token: impl Into<Option<String>>) -> TestResult<MemoClient> {
        MemoClient::new(MemoClientConfig {
            base_url: self.base_url.clone(),
            token: token.into(),
            ..MemoClientConfig::default()
        })
        .map_err(Into::into)
    }

    pub async fn create_default_mounts(&self) -> TestResult {
        self.create_mount(
            MountName::new("VaultKB")?,
            self.vault_root.clone(),
            MountMode::ReadWrite,
            vec![],
            vec![],
            vec![],
        )
        .await?;
        self.create_mount(
            MountName::new("Archive")?,
            self.archive_root.clone(),
            MountMode::ReadWrite,
            vec![],
            vec![],
            vec![],
        )
        .await
    }

    pub async fn create_mount(
        &self,
        name: MountName,
        root_path: PathBuf,
        mode: MountMode,
        hide_globs: Vec<String>,
        deny_read_globs: Vec<String>,
        deny_write_globs: Vec<String>,
    ) -> TestResult {
        let client = self.admin_client()?;
        client
            .create_mount(&CreateMountRequest {
                name,
                root_path,
                mode,
                audience: Audience::Shared,
                description: None,
                hide_globs,
                deny_read_globs,
                deny_write_globs,
                max_read_bytes: None,
                max_write_bytes: None,
            })
            .await?;
        Ok(())
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let is_running = self.child.try_wait().ok().flatten().is_none();
        if is_running {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[allow(dead_code)]
pub fn scope_set(scopes: &[&str]) -> TestResult<ScopeSet> {
    let parsed = scopes
        .iter()
        .map(|scope| Scope::from_str(scope))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ScopeSet::new(parsed))
}

fn reserve_tcp_port() -> io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

async fn wait_until_bootstrap_file(path: &std::path::Path) -> TestResult {
    let max_attempts = 200_u16;
    for _ in 0..max_attempts {
        if path.exists() {
            return Ok(());
        }
        sleep(Duration::from_millis(25)).await;
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "timed out waiting for bootstrap token file",
    )
    .into())
}

async fn wait_until_healthy(base_url: &str) -> TestResult {
    let client = MemoClient::for_base_url(base_url.to_owned())?;
    let max_attempts = 200_u16;
    for _ in 0..max_attempts {
        if client.health().await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(25)).await;
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "timed out waiting for health").into())
}
