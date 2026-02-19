use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "memo", version, about = "memo CLI", propagate_version = true)]
pub struct Cli {
    /// Emit JSON output.
    #[arg(long, global = true)]
    pub json: bool,

    /// Daemon host override.
    #[arg(long, global = true)]
    pub host: Option<String>,

    /// Daemon port override.
    #[arg(long, global = true)]
    pub port: Option<u16>,

    /// Auth token override.
    #[arg(long, global = true)]
    pub token: Option<String>,

    /// Default mount name.
    #[arg(long, global = true)]
    pub mount: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(subcommand)]
    Daemon(DaemonCommand),
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Install and load the launchd service.
    Start,
    /// Unload the launchd service.
    Stop,
    /// Check daemon status using PID file and /health.
    Status,
    /// Show memod logs from the local log file.
    Logs(DaemonLogsArgs),
}

#[derive(Debug, Args)]
pub struct DaemonLogsArgs {
    /// Number of lines from the end of the log file.
    #[arg(long, default_value_t = 100)]
    pub tail: usize,
}
