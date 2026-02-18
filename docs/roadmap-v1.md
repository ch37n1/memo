# memo v1 — Implementation Roadmap

Each step is a self-contained feature delivered as a working unit. Before starting any step, a dedicated sub-plan with subtasks is created. Steps are ordered by dependency: later steps build on earlier ones.

---

## Step 0 — Dev Infrastructure

Set up the Cargo workspace, directory skeleton, tooling, and CI so every subsequent step has a clean, reproducible foundation to build on.

**Deliverables:**
- Cargo workspace (`Cargo.toml`) with all crate stubs: `memod`, `memo`, `memo-client`, `memo-core`, `memo-ui/src-tauri`
- All workspace dependencies pinned (versions from design doc §17)
- Empty crate stubs compile with `cargo build`
- `rustfmt.toml` + `.clippy.toml` (deny warnings)
- GitHub Actions CI: `cargo fmt --check`, `cargo clippy`, `cargo test`
- `scripts/setup.sh` — create XDG dirs (`~/.config/memo`, `~/.local/share/memo`, `~/.local/state/memo`, `$XDG_RUNTIME_DIR/memo`)
- `scripts/dev.sh` — launch `memod` in debug mode with test config
- Integration test harness: helper to spin up `memod` on a random loopback port + temp SQLite DB + temp mount root, and tear down after each test
- `tests/fixtures/sample_vault/` — small directory tree used across test suites

---

## Step 1 — memo-core: Shared Types, Errors, Scopes

Pure library crate with no I/O. All other crates depend on this one. Defines the language the whole system speaks.

**Deliverables:**
- `types.rs` — `Mount`, `Token`, `AuditEntry`, `DirEntry`, `TreeNode` structs (serde Serialize/Deserialize)
- `errors.rs` — `ApiError` (with all error codes from §13), `PolicyError`, `DbError` using `thiserror`; `ApiError` implements `axum::response::IntoResponse`
- `scopes.rs` — `Scope` enum, `parse_scope()`, `check_scope(token_scopes, required)` — wildcard (`fs:*:read`) and exact matching
- Unit tests for scope matching (wildcard, exact, expired ignored at this layer)

---

## Step 2 — memod: Database Layer

SQLite database with WAL mode, migration runner, and CRUD for mounts and tokens. No HTTP, no auth — just the data layer.

**Deliverables:**
- `db/mod.rs` — `sqlx` pool (SQLite WAL), migration runner using embedded SQL via `include_str!`; `schema_migrations` version table
- SQL migrations: `001_init.sql` — `mounts`, `tokens` tables (schema from §10)
- `db/mounts.rs` — `list_mounts()`, `get_mount()`, `insert_mount()`, `update_mount()`, `delete_mount()`; invalidates glob cache on every write operation (cache injection via trait or callback)
- `db/tokens.rs` — `list_tokens()`, `get_token_by_id()`, `insert_token()`, `delete_token()`, `update_last_used()`, `find_token_for_verify()` (returns hash for a given token ID lookup)
- Unit tests: mount CRUD round-trip, token CRUD round-trip (in-memory SQLite)

---

## Step 3 — memod: Auth & Token System

Token lifecycle: generation, hashing, verification, bootstrap, scope enforcement. Depends on Step 2 (tokens table).

**Deliverables:**
- Token generation: `memo_<base62_32chars>` format using `rand`
- Argon2id hashing on create: parameters `m=19456, t=2, p=1` (OWASP minimum)
- Argon2id verification via `password-hash` crate: `PasswordHash::new()` + `Argon2::default().verify_password()` dispatched through `tokio::task::spawn_blocking`
- Bootstrap logic: on startup, if no tokens in DB, generate admin token with `admin:*` scope, write raw token to `~/.config/memo/bootstrap.token` (mode `0600`), print path to stderr, continue running
- `auth/mod.rs` — `extract_bearer(headers)` returning raw token string; `verify_token(raw, db) -> Result<TokenClaims>` checking hash + expiry; `require_scope(claims, scope)` — returns `permission_denied` if missing
- Unit tests: token generation format, hash/verify round-trip, bootstrap file creation, scope checking

---

## Step 4 — memod: Path Validation & Policy Engine

Security-critical. Pure logic (no HTTP). Validates all paths before any filesystem syscall. Depends on Step 1 (types/errors) and Step 2 (mounts).

**Deliverables:**
- `CompiledMount` struct: holds `root_path_canonical: PathBuf` (pre-computed once at load), compiled `GlobSet`s for hide/deny-read/deny-write
- `policy/mod.rs` — `PolicyEngine` wrapping `DashMap<String, Arc<CompiledMount>>`; `load_mount()`, `invalidate()`, `get()` methods; populated from DB at startup and after every mount mutation
- `policy/path.rs` — `resolve_read_path(engine, mount_name, relative) -> Result<PathBuf>` and `resolve_write_path(...)`: implement all 10 validation steps from §9 (parse mount prefix, reject `..`/absolute, join, canonicalize, prefix check, symlink check via `path_clean`, glob policy, size limit)
- Unit tests (no actual filesystem required for most): `..` traversal, absolute path rejection, symlink detection logic, hide/deny glob matching, out-of-bounds path, valid path acceptance
- Integration-style tests: real temp dirs with symlinks to verify canonicalize behavior

---

## Step 5 — memod: Filesystem Operations & Audit Log

All file operations run through `PolicyEngine`. Atomic writes, streaming reads. Depends on Steps 1–4.

**Deliverables:**
- `fs/atomic.rs` — `atomic_write(target, reader, fsync, dir_sync)` with `BufWriter`, temp-file cleanup on error (design doc §7.1)
- `fs/ops.rs` — `ls()`, `stat()` (with `index.md` frontmatter parsing for `memo_summary`), `read()` (streamed via `AsyncRead`), `write()` (calls `atomic_write`), `mkdir()`, `mv()`, `rm()`, `cp()` — all routed through `PolicyEngine`
- `fs/grep.rs` — recursive text search using `regex` + `walkdir`; respects hide globs; returns `(path, line_number, line_content)` matches up to `max_results`
- `fs/find.rs` — glob filename search using `globset` + `walkdir`; returns matching paths with size + mtime
- `audit.rs` — `append_audit(entry)`: appends one JSON line to `~/.local/state/memo/audit.log`; sequential `id` counter (in-memory `AtomicU64`, loaded from last line on startup); startup rotation if entry count exceeds `max_audit_log_rows`
- Unit tests: atomic write (normal, error path, concurrent), ls/stat correctness, grep match/no-match, audit log format + id increment

---

## Step 6 — memod: HTTP Server & Daemon Lifecycle

Wires everything into a running axum server. Startup sequence, signal handling, config loading, launchd integration. Depends on Steps 2–5.

**Deliverables:**
- `server.rs` — axum router with all routes from §11: `/health`, `/v1/fs/*`, `/v1/meta/*`; auth middleware (runs `verify_token` + `require_scope` per route); request tracing middleware (logs mount + path + duration)
- `main.rs` — full startup sequence from §7.1: load config, open log file, write PID file, open DB + run migrations, bootstrap token if needed, bind axum server, register `SIGTERM`/`SIGINT` graceful shutdown, spawn audit rotation background task
- Config loading: `~/.config/memo/config.toml` parsed via `toml`; fallback to defaults; XDG path resolution
- `daemon/launchd.rs` — `install_plist()`, `unload_plist()` for `memo daemon start/stop`: writes `~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist` and calls `launchctl load/unload` via `std::process::Command`
- Structured logging: `tracing-subscriber` JSON format to `~/.local/state/memo/memod.log` + stderr mirror
- All API endpoints return correct error codes, HTTP status, and JSON body per §11 and §13
- Manual smoke-test via `curl` (included in `scripts/dev.sh`)

---

## Step 7 — memo-client: Shared REST Client Library

Typed reqwest-based client consumed by both `memo` CLI and `memo-ui`. Depends on Step 1 (types) and Step 6 (running server to test against).

**Deliverables:**
- `lib.rs` — `MemoClient { base_url, token, reqwest::Client }`; constructor; address resolution logic (flags → env → config → default per §8)
- `fs.rs` — typed async methods for every `/v1/fs/*` endpoint: `ls()`, `tree()`, `stat()`, `read()` (returns `AsyncRead`/`Bytes`), `write()` (accepts `AsyncRead`), `mkdir()`, `mv()`, `rm()`, `cp()`, `grep()`, `find()`
- `meta.rs` — typed async methods for every `/v1/meta/*` endpoint: mount CRUD, token CRUD, audit query, health check
- All methods return `Result<T, MemoClientError>` with typed error variants mapping from HTTP error codes
- Integration tests against a real `memod` instance (using Step 0 test harness): round-trip for each endpoint family

---

## Step 8 — memo: CLI

Full-featured CLI with all commands from §14. Thin layer over `memo-client`. Depends on Step 7.

**Deliverables:**
- `main.rs` — `clap` app with global flags (`--json`, `--host`, `--port`, `--token`); token resolution chain (flag → `MEMO_TOKEN` env → `~/.config/memo/tokens/<name>.token`)
- **Filesystem commands:** `ls`, `tree`, `cat`, `write`, `mkdir`, `mv`, `rm`, `cp` (with local→mount path detection), `grep`, `find`, `info`
- **Admin commands:** `mount` (add, remove, list, show, update), `token` (create, revoke, list)
- **Daemon commands:** `daemon` (start, stop, status, logs --tail N)
- **Audit command:** `audit` (with all filter flags: `--mount`, `--limit`, `--after`, etc.)
- Plain-text output (human-readable tables/lines) by default; `--json` for structured output
- Exit codes per §13 error code table
- Integration tests: CLI invocation against a live `memod` instance covering all command groups

---

## Step 9 — Integration Tests & Security Hardening

Complete test coverage for correctness and security properties. Depends on Steps 6–8 (all runtime components exist).

**Deliverables:**
- `tests/integration/fs_ops.rs` — full round-trip suite: all happy-path operations (ls, read, write, mkdir, mv, rm, cp), error paths (missing file, read on ro mount, etc.)
- `tests/integration/policy.rs` — path traversal (`../`, `%2e%2e`, null bytes), symlink attack (real symlink pointing outside mount), hide/deny glob enforcement, ro mount write rejection
- `tests/integration/auth.rs` — missing token, invalid token, expired token, wrong scope, correct scope for each operation
- `tests/integration/atomic.rs` — temp file cleanup on write error, concurrent writers produce one valid file
- Security test corpus (`traversal_corpus.txt`): list of adversarial path strings — all must return `invalid_path` or `out_of_bounds`
- Unicode normalization path tests
- All tests tagged appropriately; security tests enabled in CI with `--include-ignored`

---

## Step 10 — memo-ui: Tauri Desktop Admin App

Native macOS desktop application for mount/token management and audit log viewing. Depends on Steps 6–7 (running `memod` + `memo-client`).

**Deliverables:**
- Tauri v2 project scaffold: `src-tauri/Cargo.toml` with correct `crate-type`, `tauri.conf.json` (bundle ID `io.github.ch37n1.memo`, React + Vite build config, CSP)
- `src-tauri/entitlements.macos.plist` — `com.apple.security.network.client` entitlement
- `src-tauri/capabilities/default.json` — `core:default` capability
- `src-tauri/src/lib.rs` — `tauri_plugin_store` registration, `generate_handler!` for all commands
- `src-tauri/src/commands.rs` — Tauri `invoke` commands wrapping `memo-client`: `list_mounts`, `create_mount`, `delete_mount`, `update_mount`, `list_tokens`, `create_token`, `revoke_token`, `get_audit`, `get_health`
- React frontend (TypeScript + Vite):
  - Setup/onboarding screen: prompt for bootstrap token on first launch, persist via `tauri-plugin-store`
  - `MountList.tsx` — list, add, remove, update mounts
  - `TokenList.tsx` — list, create, revoke tokens
  - `AuditLog.tsx` — paginated audit log with filter controls (`--mount`, `--limit`, `--after`)
  - `useMemoClient.ts` — typed hook wrapping all `invoke` calls
- `App.tsx` — tab-based navigation between the three panels
- Build verified with `cargo tauri build`

---

## Dependency Graph

```
Step 0 (workspace)
  └─▶ Step 1 (memo-core)
        └─▶ Step 2 (db)
              └─▶ Step 3 (auth)
              └─▶ Step 4 (policy)
                    └─▶ Step 5 (fs ops + audit)
                          └─▶ Step 6 (http server)
                                └─▶ Step 7 (memo-client)
                                      ├─▶ Step 8 (memo CLI)
                                      ├─▶ Step 9 (integration tests)
                                      └─▶ Step 10 (memo-ui)
```
