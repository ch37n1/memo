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
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::Cli;

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
}
