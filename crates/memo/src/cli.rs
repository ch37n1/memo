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
    #[command(subcommand)]
    Mount(MountCommand),
    #[command(subcommand)]
    Token(TokenCommand),
    /// Query audit entries.
    Audit(AuditArgs),
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

#[derive(Debug, Subcommand)]
pub enum MountCommand {
    /// List mounts.
    List,
    /// Add a mount.
    Add(MountAddArgs),
    /// Remove a mount by name.
    Remove(MountRemoveArgs),
    /// Show a mount by name.
    Show(MountShowArgs),
    /// Update an existing mount.
    Update(MountUpdateArgs),
}

#[derive(Debug, Args)]
pub struct MountAddArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub path: std::path::PathBuf,
    #[arg(long)]
    pub mode: String,
    #[arg(long)]
    pub audience: String,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long, value_delimiter = ',')]
    pub hide_glob: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub deny_read_glob: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub deny_write_glob: Vec<String>,
    #[arg(long)]
    pub max_read_bytes: Option<u64>,
    #[arg(long)]
    pub max_write_bytes: Option<u64>,
}

#[derive(Debug, Args)]
pub struct MountRemoveArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct MountShowArgs {
    pub name: String,
}

#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "CLI clear flags are independent switches and map directly to user intent"
)]
pub struct MountUpdateArgs {
    pub name: String,
    #[arg(long)]
    pub mode: Option<String>,
    #[arg(long)]
    pub audience: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long, conflicts_with = "description")]
    pub clear_description: bool,
    #[arg(long, value_delimiter = ',')]
    pub hide_glob: Option<Vec<String>>,
    #[arg(long, conflicts_with = "hide_glob")]
    pub clear_hide_globs: bool,
    #[arg(long, value_delimiter = ',')]
    pub deny_read_glob: Option<Vec<String>>,
    #[arg(long, conflicts_with = "deny_read_glob")]
    pub clear_deny_read_globs: bool,
    #[arg(long, value_delimiter = ',')]
    pub deny_write_glob: Option<Vec<String>>,
    #[arg(long, conflicts_with = "deny_write_glob")]
    pub clear_deny_write_globs: bool,
    #[arg(long)]
    pub max_read_bytes: Option<u64>,
    #[arg(long, conflicts_with = "max_read_bytes")]
    pub clear_max_read_bytes: bool,
    #[arg(long)]
    pub max_write_bytes: Option<u64>,
    #[arg(long, conflicts_with = "max_write_bytes")]
    pub clear_max_write_bytes: bool,
}

#[derive(Debug, Subcommand)]
pub enum TokenCommand {
    /// List tokens.
    List,
    /// Create a token.
    Create(TokenCreateArgs),
    /// Revoke a token.
    Revoke(TokenRevokeArgs),
}

#[derive(Debug, Args)]
pub struct TokenCreateArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long, required = true, value_delimiter = ',')]
    pub scopes: Vec<String>,
    #[arg(long)]
    pub expires: Option<String>,
}

#[derive(Debug, Args)]
pub struct TokenRevokeArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    #[arg(long)]
    pub mount: Option<String>,
    #[arg(long)]
    pub token_id: Option<String>,
    #[arg(long)]
    pub operation: Option<String>,
    #[arg(long)]
    pub result: Option<String>,
    #[arg(long)]
    pub limit: Option<u64>,
    #[arg(long)]
    pub before: Option<String>,
    #[arg(long)]
    pub after: Option<String>,
}
