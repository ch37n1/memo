use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::Cli;
use crate::error::CliError;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 18_301;
const DEFAULT_TOKEN_NAME: &str = "default";

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub json: bool,
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
    pub default_mount: Option<String>,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    daemon: DaemonConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DaemonConfig {
    bind_addr: Option<String>,
    log_path: Option<String>,
}

impl RuntimeConfig {
    pub fn resolve(cli: &Cli) -> Result<Self, CliError> {
        let config_path = config_file_path()?;
        let file_config = load_file_config(&config_path)?;

        let env_host = std::env::var("MEMO_HOST").ok();
        let env_port = std::env::var("MEMO_PORT").ok();
        let (host, port) = resolve_daemon_addr(
            cli.host.as_deref(),
            cli.port,
            env_host.as_deref(),
            env_port.as_deref(),
            file_config.daemon.bind_addr.as_deref(),
        )?;

        let token = resolve_token(
            cli.token.as_deref(),
            std::env::var("MEMO_TOKEN").ok().as_deref(),
            &token_file_path(DEFAULT_TOKEN_NAME)?,
        )?;

        let log_path = resolve_log_path(file_config.daemon.log_path.as_deref())?;

        Ok(Self {
            json: cli.json,
            host,
            port,
            token,
            default_mount: cli.mount.clone(),
            log_path,
        })
    }

    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

fn load_file_config(path: &Path) -> Result<FileConfig, CliError> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }

    let contents = fs::read_to_string(path)?;
    Ok(toml::from_str::<FileConfig>(&contents)?)
}

fn config_file_path() -> Result<PathBuf, CliError> {
    Ok(home_dir()?.join(".config/memo/config.toml"))
}

fn token_file_path(name: &str) -> Result<PathBuf, CliError> {
    Ok(home_dir()?.join(format!(".config/memo/tokens/{name}.token")))
}

pub fn launch_agent_plist_path() -> Result<PathBuf, CliError> {
    Ok(home_dir()?.join("Library/LaunchAgents/io.github.ch37n1.memo.memod.plist"))
}

pub fn pid_file_path() -> PathBuf {
    let runtime_root =
        std::env::var("XDG_RUNTIME_DIR").map_or_else(|_| std::env::temp_dir(), PathBuf::from);

    runtime_root.join("memo/memod.pid")
}

fn resolve_log_path(path_from_config: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(path) = path_from_config {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return expand_user_path(trimmed);
        }
    }

    Ok(home_dir()?.join(".local/state/memo/memod.log"))
}

fn resolve_token(
    flag_token: Option<&str>,
    env_token: Option<&str>,
    token_file: &Path,
) -> Result<Option<String>, CliError> {
    if let Some(token) = flag_token {
        return Ok(Some(token.to_owned()));
    }

    if let Some(token) = env_token {
        return Ok(Some(token.to_owned()));
    }

    if !token_file.exists() {
        return Ok(None);
    }

    let file_token = fs::read_to_string(token_file)?;
    let trimmed = file_token.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed.to_owned()))
}

fn resolve_daemon_addr(
    flag_host: Option<&str>,
    flag_port: Option<u16>,
    env_host: Option<&str>,
    env_port: Option<&str>,
    config_bind_addr: Option<&str>,
) -> Result<(String, u16), CliError> {
    let (config_host, config_port) = parse_bind_addr(config_bind_addr)?;

    let host = flag_host
        .map(str::to_owned)
        .or_else(|| env_host.map(str::to_owned))
        .or(config_host)
        .unwrap_or_else(|| DEFAULT_HOST.to_owned());

    let port = if let Some(port) = flag_port {
        port
    } else if let Some(port) = env_port {
        port.parse::<u16>()
            .map_err(|_| CliError::Config(format!("invalid MEMO_PORT: {port}")))?
    } else {
        config_port.unwrap_or(DEFAULT_PORT)
    };

    Ok((host, port))
}

fn parse_bind_addr(bind_addr: Option<&str>) -> Result<(Option<String>, Option<u16>), CliError> {
    let Some(bind_addr) = bind_addr else {
        return Ok((None, None));
    };

    let bind_addr = bind_addr.trim();
    if bind_addr.is_empty() {
        return Ok((None, None));
    }

    let (host, port) = bind_addr
        .rsplit_once(':')
        .ok_or_else(|| CliError::Config(format!("invalid daemon.bind_addr: {bind_addr}")))?;

    let port = port
        .parse::<u16>()
        .map_err(|_| CliError::Config(format!("invalid daemon.bind_addr port: {bind_addr}")))?;

    Ok((Some(host.to_owned()), Some(port)))
}

fn home_dir() -> Result<PathBuf, CliError> {
    let home = std::env::var("HOME")
        .map_err(|_| CliError::Config("HOME environment variable is not set".to_owned()))?;
    Ok(PathBuf::from(home))
}

fn expand_user_path(path: &str) -> Result<PathBuf, CliError> {
    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }

    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{resolve_daemon_addr, resolve_token};

    #[test]
    fn daemon_addr_uses_priority_chain() {
        let from_config = match resolve_daemon_addr(None, None, None, None, Some("10.0.0.1:9000")) {
            Ok(value) => value,
            Err(error) => panic!("config should parse: {error}"),
        };
        assert_eq!(from_config.0, "10.0.0.1");
        assert_eq!(from_config.1, 9000);

        let from_env = resolve_daemon_addr(
            None,
            None,
            Some("10.0.0.2"),
            Some("9001"),
            Some("10.0.0.1:9000"),
        )
        .unwrap_or_else(|error| panic!("env should parse: {error}"));
        assert_eq!(from_env.0, "10.0.0.2");
        assert_eq!(from_env.1, 9001);

        let from_flag = resolve_daemon_addr(
            Some("10.0.0.3"),
            Some(9002),
            Some("10.0.0.2"),
            Some("9001"),
            Some("10.0.0.1:9000"),
        )
        .unwrap_or_else(|error| panic!("flags should parse: {error}"));
        assert_eq!(from_flag.0, "10.0.0.3");
        assert_eq!(from_flag.1, 9002);
    }

    #[test]
    fn token_resolution_prefers_flag_then_env() {
        let token = resolve_token(
            Some("flag-token"),
            Some("env-token"),
            Path::new("/does/not/exist"),
        )
        .unwrap_or_else(|error| panic!("token should resolve: {error}"));
        assert_eq!(token.as_deref(), Some("flag-token"));

        let token = resolve_token(None, Some("env-token"), Path::new("/does/not/exist"))
            .unwrap_or_else(|error| panic!("token should resolve: {error}"));
        assert_eq!(token.as_deref(), Some("env-token"));

        let token = resolve_token(None, None, Path::new("/does/not/exist"))
            .unwrap_or_else(|error| panic!("token should resolve: {error}"));
        assert!(token.is_none());
    }
}
