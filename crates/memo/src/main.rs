mod admin;
mod cli;
mod config;
mod daemon;
mod error;

use clap::Parser;
use std::io::Write;

use crate::cli::{Cli, Commands};
use crate::config::RuntimeConfig;
use crate::error::CliError;

#[tokio::main]
async fn main() {
    let exit_code = match run().await {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "error: {error}");
            error.exit_code()
        }
    };

    std::process::exit(exit_code);
}

async fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    let runtime = RuntimeConfig::resolve(&cli)?;
    let _default_mount = runtime.default_mount.as_deref();

    match &cli.command {
        Commands::Daemon(command) => daemon::run(command, &runtime).await,
        Commands::Mount(command) => admin::run_mount(command, &runtime).await,
        Commands::Token(command) => admin::run_token(command, &runtime).await,
        Commands::Audit(args) => admin::run_audit(args, &runtime).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::{Cli, Commands, MountCommand, TokenCommand};

    #[test]
    fn parses_global_flags_after_subcommand() {
        let parsed =
            Cli::try_parse_from(["memo", "daemon", "status", "--json", "--host", "127.0.0.2"]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("cli should parse: {error}"),
        };

        assert!(cli.json);
        assert_eq!(cli.host.as_deref(), Some("127.0.0.2"));
    }

    #[test]
    fn parses_mount_add_flags() {
        let parsed = Cli::try_parse_from([
            "memo",
            "mount",
            "add",
            "--name",
            "VaultKB",
            "--path",
            "/tmp/vault",
            "--mode",
            "read_write",
            "--audience",
            "shared",
            "--hide-glob",
            "**/*.secret",
        ]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("mount add should parse: {error}"),
        };

        let Commands::Mount(MountCommand::Add(args)) = cli.command else {
            panic!("expected mount add command");
        };
        assert_eq!(args.name, "VaultKB");
        assert_eq!(args.mode, "read_write");
        assert_eq!(args.audience, "shared");
        assert_eq!(args.hide_glob, vec!["**/*.secret".to_owned()]);
    }

    #[test]
    fn parses_mount_add_glob_csv() {
        let parsed = Cli::try_parse_from([
            "memo",
            "mount",
            "add",
            "--name",
            "VaultKB",
            "--path",
            "/tmp/vault",
            "--mode",
            "read_write",
            "--audience",
            "shared",
            "--hide-glob",
            "**/*.secret,**/*.tmp",
        ]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("mount add csv should parse: {error}"),
        };

        let Commands::Mount(MountCommand::Add(args)) = cli.command else {
            panic!("expected mount add command");
        };
        assert_eq!(
            args.hide_glob,
            vec!["**/*.secret".to_owned(), "**/*.tmp".to_owned()]
        );
    }

    #[test]
    fn parses_mount_update_clear_flags() {
        let parsed = Cli::try_parse_from([
            "memo",
            "mount",
            "update",
            "VaultKB",
            "--clear-hide-globs",
            "--clear-max-read-bytes",
        ]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("mount update clear flags should parse: {error}"),
        };

        let Commands::Mount(MountCommand::Update(args)) = cli.command else {
            panic!("expected mount update command");
        };
        assert!(args.clear_hide_globs);
        assert!(args.clear_max_read_bytes);
    }

    #[test]
    fn parses_token_create_scopes() {
        let parsed = Cli::try_parse_from([
            "memo",
            "token",
            "create",
            "--name",
            "agent",
            "--scopes",
            "admin:*:tokens,meta:*:read",
        ]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("token create should parse: {error}"),
        };

        let Commands::Token(TokenCommand::Create(args)) = cli.command else {
            panic!("expected token create command");
        };
        assert_eq!(
            args.scopes,
            vec!["admin:*:tokens".to_owned(), "meta:*:read".to_owned()]
        );
    }

    #[test]
    fn parses_audit_query_filters() {
        let parsed = Cli::try_parse_from([
            "memo", "audit", "--mount", "VaultKB", "--result", "ok", "--limit", "25",
        ]);
        let cli = match parsed {
            Ok(cli) => cli,
            Err(error) => panic!("audit query should parse: {error}"),
        };

        let Commands::Audit(args) = cli.command else {
            panic!("expected audit command");
        };
        assert_eq!(args.mount.as_deref(), Some("VaultKB"));
        assert_eq!(args.result.as_deref(), Some("ok"));
        assert_eq!(args.limit, Some(25));
    }
}
