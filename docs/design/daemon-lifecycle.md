---
description: Daemon lifecycle design — startup sequence, runtime management, and graceful shutdown semantics
tags:
  - design
  - daemon
  - lifecycle
---

# Daemon Lifecycle Design

## Responsibility

Daemon lifecycle defines how `memod` starts, runs, and shuts down safely:

- load config and initialize runtime dependencies
- expose HTTP API only after required state is ready
- manage process identity and runtime files
- shut down gracefully without corrupting state

## Startup Sequence

Startup is ordered to reduce partial-initialization risk:

1. Resolve config (`~/.config/memo/config.toml`) and defaults.
2. Initialize logging sinks (log file + stderr).
3. Create runtime directories if missing (`~/.config/memo/`, `~/.local/share/memo/`, `~/.local/state/memo/`).
4. Create PID file in `$XDG_RUNTIME_DIR/memo/memod.pid` (fallback runtime dir when needed).
5. Initialize SQLite and run embedded migrations.
6. Run bootstrap token check when no tokens exist.
7. Assemble router and bind HTTP server.
8. Start background tasks (for example audit log prune/rotation).
9. Register signal handlers and enter serving loop.

If any required step fails, startup aborts and returns a non-zero exit code.

## Config Surface

Lifecycle-relevant config includes:

- daemon bind address (`daemon.bind_addr`)
- write durability toggles (`daemon.write.fsync`, `daemon.write.dir_sync`)
- audit limits (`daemon.limits.max_audit_log_rows`)
- log level and log path (`daemon.log_level`, `log_path`)
- database path (`db_path`)

Config parse/validation errors are startup failures, not deferred runtime warnings.

Config precedence for write durability:

1. Environment overrides (`MEMOD_WRITE_FSYNC`, `MEMOD_WRITE_DIR_SYNC`) when present and parseable.
2. File config (`daemon.write.fsync`, `daemon.write.dir_sync`).
3. Built-in defaults (`true` for both).

Operational intent:

- Keep defaults enabled for stronger durability.
- Disable selectively only for controlled environments (for example specific test harnesses or known filesystem limitations).

## Runtime Contracts

- `GET /health` stays unauthenticated and reflects daemon liveness.
- Filesystem I/O stays daemon-owned; clients only call HTTP endpoints.
- PID file reflects a single active daemon instance per runtime directory.
- Structured logs are emitted with `tracing` for operational visibility.

## Shutdown Semantics

On `SIGTERM`/`SIGINT`:

1. Stop accepting new work.
2. Let in-flight requests complete within graceful-shutdown bounds.
3. Flush/close shared resources (including DB pool/log sinks as needed).
4. Remove PID file.
5. Exit cleanly.

PID cleanup is best-effort on abnormal termination and guaranteed on graceful shutdown path.
