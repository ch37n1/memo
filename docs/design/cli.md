---
description: CLI design — global option resolution, command behavior, output policy, and exit codes
tags:
  - design
  - cli
---

# CLI Design

## Responsibility

The `memo` CLI is a thin command surface over `memo-client`.
It resolves runtime configuration (host/port/token/default mount), dispatches subcommands, and formats output.

## Global Options

Global flags are available across subcommands:

- `--json`
- `--host`
- `--port`
- `--token`
- `--mount`

## Resolution Order

### Token

1. `--token`
2. `MEMO_TOKEN`
3. `~/.config/memo/tokens/default.token`

### Host/Port

1. `--host` / `--port`
2. `MEMO_HOST` / `MEMO_PORT`
3. `~/.config/memo/config.toml` `[daemon].bind_addr`
4. `127.0.0.1:18301`

## Daemon Commands (Phase 2 / B2)

- `memo daemon start`
  - Writes launchd plist to `~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist`
  - Uses `launchctl load` to register the service
- `memo daemon stop`
  - Uses `launchctl unload` for the same plist
- `memo daemon status`
  - Checks PID file at `$XDG_RUNTIME_DIR/memo/memod.pid` (fallback: system temp dir)
  - Verifies daemon health via `GET /health`
- `memo daemon logs --tail N`
  - Reads `memod` log file directly from local filesystem

## Admin Commands (Phase 3 / B3)

- `memo mount list`
- `memo mount add --name --path --mode --audience [policy flags...]`
  - Supports repeated or CSV glob flags: `--hide-glob`, `--deny-read-glob`, `--deny-write-glob`
- `memo mount show <name>`
- `memo mount remove <name>`
- `memo mount update <name> [partial flags...]`
  - Supports explicit clear flags:
    - `--clear-description`
    - `--clear-hide-globs`
    - `--clear-deny-read-globs`
    - `--clear-deny-write-globs`
    - `--clear-max-read-bytes`
    - `--clear-max-write-bytes`
- `memo token list`
- `memo token create --name --scopes [--expires RFC3339]`
  - Plain mode prints the raw token value so it can be copied immediately.
- `memo token revoke <token-id>`
- `memo audit [--mount --token-id --operation --result --limit --before --after]`

## Output Modes

- Default mode: human-readable plain text
- JSON mode: structured JSON suitable for automation and agents

## Exit Code Policy

- `0` success
- `1` general/config/command error
- `2` auth error
- `3` permission/policy error
- `4` not found
- `5` daemon unreachable
