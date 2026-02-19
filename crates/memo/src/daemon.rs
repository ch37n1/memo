use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use memo_client::{HealthResponse, MemoClient};
use serde_json::json;

use crate::cli::{DaemonCommand, DaemonLogsArgs};
use crate::config::{launch_agent_plist_path, pid_file_path, RuntimeConfig};
use crate::error::CliError;

const LAUNCHD_LABEL: &str = "io.github.ch37n1.memo.memod";

pub async fn run(command: &DaemonCommand, runtime: &RuntimeConfig) -> Result<(), CliError> {
    match command {
        DaemonCommand::Start => start(runtime),
        DaemonCommand::Stop => stop(runtime),
        DaemonCommand::Status => status(runtime).await,
        DaemonCommand::Logs(args) => logs(args, runtime),
    }
}

fn start(runtime: &RuntimeConfig) -> Result<(), CliError> {
    let plist_path = launch_agent_plist_path()?;
    let log_path = &runtime.log_path;
    let memod_path = memod_binary_path()?;

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let plist = plist_contents(&memod_path, log_path);
    fs::write(&plist_path, plist)?;
    run_launchctl("load", &plist_path)?;

    emit_output(
        runtime,
        &json!({
            "status": "started",
            "plist_path": plist_path.display().to_string(),
            "binary": memod_path.display().to_string()
        }),
        &format!("daemon started ({})", plist_path.display()),
    )
}

fn stop(runtime: &RuntimeConfig) -> Result<(), CliError> {
    let plist_path = launch_agent_plist_path()?;
    run_launchctl("unload", &plist_path)?;

    emit_output(
        runtime,
        &json!({
            "status": "stopped",
            "plist_path": plist_path.display().to_string()
        }),
        "daemon stopped",
    )
}

async fn status(runtime: &RuntimeConfig) -> Result<(), CliError> {
    let pid_path = pid_file_path();
    if !pid_path.exists() {
        return Err(CliError::DaemonUnreachable(format!(
            "daemon not running (missing pid file: {})",
            pid_path.display()
        )));
    }

    let health = health_check(runtime).await?;

    emit_output(
        runtime,
        &json!({
            "running": true,
            "pid_file": pid_path.display().to_string(),
            "health": health
        }),
        &format!(
            "daemon running (pid file: {}, version: {})",
            pid_path.display(),
            health.version
        ),
    )
}

fn logs(args: &DaemonLogsArgs, runtime: &RuntimeConfig) -> Result<(), CliError> {
    let lines = tail_file(&runtime.log_path, args.tail)?;

    if runtime.json {
        let payload = json!({
            "log_path": runtime.log_path.display().to_string(),
            "lines": lines,
        });
        write_stdout(&payload.to_string())
    } else {
        for line in lines {
            write_stdout(&line)?;
        }
        Ok(())
    }
}

async fn health_check(runtime: &RuntimeConfig) -> Result<HealthResponse, CliError> {
    let base_client =
        MemoClient::for_base_url(runtime.base_url()).map_err(CliError::from_client_error)?;
    let client = if let Some(token) = &runtime.token {
        base_client.with_token(token.clone())
    } else {
        base_client
    };

    client
        .health()
        .await
        .map_err(|error| CliError::DaemonUnreachable(error.to_string()))
}

fn run_launchctl(action: &str, plist_path: &Path) -> Result<(), CliError> {
    let output = Command::new("launchctl")
        .arg(action)
        .arg(plist_path)
        .output()?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(CliError::Command(format!(
        "launchctl {action} failed for {}: {stderr}",
        plist_path.display()
    )))
}

fn memod_binary_path() -> Result<PathBuf, CliError> {
    let current = std::env::current_exe()?;
    let sibling = current.with_file_name("memod");
    if sibling.exists() {
        return Ok(sibling);
    }

    Ok(PathBuf::from("memod"))
}

fn plist_contents(binary: &Path, log_path: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
  <key>Label</key>\n\
  <string>{LAUNCHD_LABEL}</string>\n\
  <key>ProgramArguments</key>\n\
  <array>\n\
    <string>{}</string>\n\
  </array>\n\
  <key>RunAtLoad</key>\n\
  <false/>\n\
  <key>StandardOutPath</key>\n\
  <string>{}</string>\n\
  <key>StandardErrorPath</key>\n\
  <string>{}</string>\n\
</dict>\n\
</plist>\n",
        binary.display(),
        log_path.display(),
        log_path.display()
    )
}

fn tail_file(path: &Path, limit: usize) -> Result<Vec<String>, CliError> {
    if !path.exists() {
        return Err(CliError::DaemonUnreachable(format!(
            "log file not found: {}",
            path.display()
        )));
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);

    let cap = limit.max(1);
    let mut queue: VecDeque<String> = VecDeque::with_capacity(cap);
    for line in reader.lines() {
        let line = line?;
        if queue.len() == cap {
            let _ = queue.pop_front();
        }
        queue.push_back(line);
    }

    Ok(queue.into_iter().collect())
}

fn emit_output(
    runtime: &RuntimeConfig,
    json_value: &serde_json::Value,
    plain: &str,
) -> Result<(), CliError> {
    if runtime.json {
        write_stdout(&json_value.to_string())
    } else {
        write_stdout(plain)
    }
}

fn write_stdout(line: &str) -> Result<(), CliError> {
    writeln!(std::io::stdout(), "{line}").map_err(CliError::from)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;

    use tempfile::NamedTempFile;

    use super::{plist_contents, tail_file, LAUNCHD_LABEL};

    #[test]
    fn plist_contains_required_fields() {
        let content = plist_contents(Path::new("/tmp/memod"), Path::new("/tmp/memod.log"));
        assert!(content.contains(LAUNCHD_LABEL));
        assert!(content.contains("/tmp/memod"));
        assert!(content.contains("/tmp/memod.log"));
        assert!(content.contains("<false/>"));
    }

    #[test]
    fn tail_file_returns_last_n_lines() {
        let mut file = NamedTempFile::new()
            .unwrap_or_else(|error| panic!("temp file should be created: {error}"));
        writeln!(file, "one").unwrap_or_else(|error| panic!("line should write: {error}"));
        writeln!(file, "two").unwrap_or_else(|error| panic!("line should write: {error}"));
        writeln!(file, "three").unwrap_or_else(|error| panic!("line should write: {error}"));

        let lines = tail_file(file.path(), 2)
            .unwrap_or_else(|error| panic!("tail should succeed: {error}"));
        assert_eq!(lines, vec!["two".to_owned(), "three".to_owned()]);
    }
}
