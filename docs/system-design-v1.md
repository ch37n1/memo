# memo — System Design v1

## 1. Overview

`memo` is a unified memory layer for collaborative human–agent knowledge work. It gives humans and LLM agents a shared, policy-controlled space where both can read, write, and build a common knowledge base — without exposing unrelated personal files.

In v1, memory is **file-based**: regular files and directories accessed through named mounts. Files are the right starting primitive because they are native to human tools (Obsidian, editors) and equally accessible to agents. Future versions will extend the memory model with richer types (entities, graph, embeddings), but v1 is deliberately file-only and Markdown-native.

It follows a client–server model with three binaries:

- **`memod`** — daemon process; owns all filesystem I/O; the only process that touches the real filesystem
- **`memo`** — CLI client for humans and agents
- **`memo-ui`** — Tauri-based native macOS admin application for managing mounts, tokens, and audit log; not a general file editor

**Primary platform: macOS.** Linux is a supported secondary target. Windows is out of scope.

All IPC is over a Unix domain socket. No network exposure. No shell execution. No virtual filesystem.

**Implementation language: Rust** across the entire stack. Rationale:

- Memory safety without GC — critical for a daemon doing path manipulation
- Single static binary output for `memod` and `memo`
- Tauri requires Rust for the backend; using Rust everywhere means one toolchain (Cargo workspace)
- Strong ecosystem for async HTTP (`axum`/`hyper`), SQLite (`rusqlite`/`sqlx`), and path handling

---

## 2. Development Stage

**MVP — single-user personal tool.**

One human operator. One or more LLM agents. All on the same machine. No multi-user, no networked access, no team features. Simplicity over generality.

---

## 3. Goals & Non-Goals

### Goals

- Provide safe, policy-enforced access to named filesystem mounts
- Prevent any out-of-bounds path access (traversal, symlinks, absolute paths)
- Support collaborative knowledge files (Markdown) readable by both humans (Obsidian) and agents
- Atomic writes compatible with file watchers (Obsidian, etc.)
- Token-based auth with per-mount, per-operation scopes
- Structured, deterministic output suitable for LLM agent consumption
- Audit log of all operations

### Non-Goals (v1)

- Embeddings or semantic search
- Graph/object memory layer
- Network exposure (HTTP over TCP, TLS, etc.)
- Multi-user or multi-machine scenarios
- Shell execution
- Virtual filesystem views
- Non-file memory types
- Windows support (macOS is primary; Linux is nice-to-have)

---

## 4. Functional Requirements

| ID | Requirement |
|----|-------------|
| FR-01 | Daemon registers named mounts with a root path and access mode (ro/rw) |
| FR-02 | All client paths are expressed as `<MountName>:/relative/path` |
| FR-03 | Clients can list, stat, read, write, move, copy, delete files and directories within a mount |
| FR-04 | Clients can perform text search (grep) and glob search (find) within a mount |
| FR-05 | All write operations are atomic (write-to-temp + rename) |
| FR-06 | Symlinks are never shown, followed, or accessible |
| FR-07 | `..`, absolute paths, and malformed paths are rejected before any filesystem access |
| FR-08 | Per-mount policy: hide globs, deny-read globs, deny-write globs, max file size |
| FR-09 | Token-based auth: opaque tokens, stored hashed (Argon2id), scoped per mount and operation |
| FR-10 | All operations are logged to an audit log (SQLite) |
| FR-11 | CLI produces plain text by default; `--json` flag for structured output |
| FR-12 | `memo-ui` provides a web-based admin interface for mount and token management |
| FR-13 | Directory info from `index.md` frontmatter is surfaced via `stat` and `info` commands |

---

## 5. Non-Functional Requirements

| ID | Requirement |
|----|-------------|
| NFR-01 | All filesystem access is serialized through the daemon; no direct FS access from clients |
| NFR-02 | Daemon must handle concurrent requests safely (async runtime, no shared mutable FS state without locking) |
| NFR-03 | Path validation must be deterministic and side-effect-free before any syscall |
| NFR-04 | Response latency for metadata operations (ls, stat) < 50ms on local SSD |
| NFR-05 | Large file reads/writes are streamed; no full-file buffering in memory |
| NFR-06 | SQLite WAL mode enabled; concurrent reads must not block writes |
| NFR-07 | Daemon startup time < 500ms |
| NFR-08 | Config and DB paths follow XDG Base Directory Specification |
| NFR-09 | Error responses always include a machine-readable error code |
| NFR-10 | Exit codes: 0 = success, non-zero = failure (for CLI) |

---

## 6. System Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Machine                            │
│                                                                 │
│  ┌──────────┐    ┌────────────┐    ┌──────────────────────┐    │
│  │  memo    │    │  memo-ui   │    │   LLM Agent Process  │    │
│  │  (CLI)   │    │  (Tauri)   │    │   (Claude, etc.)     │    │
│  └────┬─────┘    └─────┬──────┘    └──────────┬───────────┘    │
│       │                │                       │                │
│       └────────────────┴───────────────────────┘                │
│                        │  HTTP/1.1 + JSON                       │
│                        │  Unix domain socket                    │
│              ┌─────────▼──────────┐                            │
│              │       memod        │                            │
│              │   (axum/hyper)     │                            │
│              │                   │                            │
│              │  ┌─────────────┐  │                            │
│              │  │  auth layer │  │                            │
│              │  └──────┬──────┘  │                            │
│              │         │         │                            │
│              │  ┌──────▼──────┐  │     ┌─────────────────┐   │
│              │  │policy layer │  │     │     SQLite       │   │
│              │  └──────┬──────┘  │────▶│  mounts         │   │
│              │         │         │     │  tokens         │   │
│              │  ┌──────▼──────┐  │     │  audit_log      │   │
│              │  │  fs layer   │  │     └─────────────────┘   │
│              │  └──────┬──────┘  │                            │
│              └─────────┼─────────┘                            │
│                        │                                       │
│         ┌──────────────┼──────────────────┐                   │
│         │              │                  │                   │
│  ┌──────▼──────┐ ┌─────▼──────┐ ┌────────▼──────┐           │
│  │ Vault/      │ │ ~/.local/  │ │ /any/path/    │           │
│  │ SharedKB/   │ │ share/memo/│ │ agent-data/   │           │
│  │ (shared)    │ │ (agent)    │ │ (user-defined)│           │
│  └─────────────┘ └────────────┘ └───────────────┘           │
└─────────────────────────────────────────────────────────────────┘
```

**Socket path resolution (in order):**

1. `$XDG_RUNTIME_DIR/memo/memod.sock`
2. `~/.local/run/memo/memod.sock`

**Config file:** `~/.config/memo/config.toml`

**Database:** `~/.local/share/memo/memo.db` (XDG data dir, overridable in config)

---

## 7. Component Design

### 7.1 memod (Daemon)

Single async binary built on `tokio` + `axum`. Listens on a Unix domain socket. All filesystem I/O runs inside this process.

**Startup sequence:**

1. Load `~/.config/memo/config.toml`
2. Create socket directory and acquire socket lock (PID file at `$XDG_RUNTIME_DIR/memo/memod.pid`)
3. Open SQLite database (WAL mode, apply migrations)
4. Bind `axum` server to Unix socket
5. Register signal handlers: `SIGTERM`/`SIGINT` for graceful shutdown

**Internal modules:**

```
memod/src/
├── main.rs          # startup, config loading, signal handling
├── server.rs        # axum router, middleware, request tracing
├── fs/
│   ├── mod.rs       # FsService: core dispatch
│   ├── ops.rs       # ls, stat, read, write, mkdir, mv, rm, cp
│   ├── grep.rs      # text search implementation
│   ├── find.rs      # glob search implementation
│   └── atomic.rs    # atomic write-by-rename
├── policy/
│   ├── mod.rs       # PolicyEngine: path validation + glob matching
│   └── path.rs      # canonical path resolution, out-of-bounds check
├── db/
│   ├── mod.rs       # DB pool, migration runner
│   ├── mounts.rs    # mount CRUD
│   ├── tokens.rs    # token CRUD, hash verification
│   └── audit.rs     # audit log insertion
└── auth/
    └── mod.rs       # token extraction, scope verification
```

**Request lifecycle:**

```
Incoming request
  → Extract Bearer token (auth layer)
  → Verify token hash against DB, check not expired
  → Check token scope against required scope for operation
  → Parse mount name + relative path from query param
  → Look up mount in DB
  → Run path validation (PolicyEngine)
    → Resolve canonical path, check within mount root
    → Check hide/deny globs
    → Check file size limits
  → Execute fs operation
  → Write audit log entry
  → Return response
```

**Atomic write implementation:**

```rust
// atomic.rs
pub async fn atomic_write(target: &Path, content: &[u8], fsync: bool) -> Result<()> {
    let dir = target.parent().ok_or(Error::InvalidPath)?;
    let tmp = dir.join(format!(".memo_tmp_{}", random_hex(8)));
    let mut file = fs::File::create(&tmp).await?;
    file.write_all(content).await?;
    if fsync {
        file.sync_all().await?;
    }
    drop(file);
    fs::rename(&tmp, target).await?;
    // optionally sync parent dir
    Ok(())
}
```

**Streaming reads:** For files above a configurable threshold (default: 4MB), reads are streamed using `axum::body::Body::from_stream`. Writes accept `axum::body::Bytes` streamed from request body.

### 7.2 memo (CLI)

Thin client binary. Reads token from environment variable or OS keychain. Sends HTTP requests to the daemon over the Unix socket using `hyper` with a custom `UnixConnector`.

**Command dispatch:**

```
memo/src/
├── main.rs          # clap app, global flags (--json, --socket, --token)
├── client.rs        # HTTP client over Unix socket
└── commands/
    ├── ls.rs
    ├── tree.rs
    ├── cat.rs
    ├── write.rs
    ├── mkdir.rs
    ├── mv.rs
    ├── rm.rs
    ├── cp.rs
    ├── grep.rs
    ├── find.rs
    ├── info.rs
    ├── mount.rs     # mount management (add, remove, list)
    └── token.rs     # token management (create, revoke, list)
```

**Token resolution order (CLI):**

1. `--token` flag
2. `MEMO_TOKEN` environment variable
3. macOS Keychain (service: `memo`, account: mount name) — v1 best-effort
4. `~/.config/memo/tokens/<name>.token` (mode `0600`)

**Global flags:**

```
--json              Emit JSON output
--socket <path>     Override socket path
--token <token>     Override token
--mount <name>      Default mount (reduces repetition)
```

### 7.3 memo-ui (Tauri Admin UI)

Tauri application where the Rust backend connects to `memod` via the Unix socket (same `hyper` + `UnixConnector` approach as the CLI). The frontend (web) communicates with the Tauri backend via Tauri commands.

**Scope in v1:** Mount management, token management, audit log viewer, basic file browser. Not a full editor — Obsidian handles editing for shared mounts.

**Directory layout:**

```
memo-ui/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   └── commands.rs   # Tauri commands wrapping memod HTTP calls
│   └── Cargo.toml
└── src/                  # web frontend (HTML/CSS/JS or framework)
```

The UI connects to `memod` using the same token mechanism as the CLI. Admin operations (mount registration, token creation) require tokens with `admin:*` scopes.

---

## 8. IPC Protocol

**Transport:** HTTP/1.1 over Unix domain socket.

**Why HTTP over Unix socket:**

- Debuggable with `curl --unix-socket /path/to/memod.sock http://localhost/v1/fs/ls?path=VaultKB:/`
- Well-supported by `axum`/`hyper` natively
- Standard request/response semantics; streaming via chunked transfer encoding
- No custom framing protocol to maintain

**Content-Type:** `application/json` for all request and response bodies except raw file reads/writes.

**File read:** Response body is raw file bytes. `Content-Type` reflects file content where determinable, otherwise `application/octet-stream`.

**File write:** Request body is raw file bytes. `Content-Type` is ignored by the daemon.

**Auth header:** `Authorization: Bearer <token>` on every request.

**Socket path:**

```
Priority 1: $XDG_RUNTIME_DIR/memo/memod.sock
Priority 2: ~/.local/run/memo/memod.sock
Override:   MEMO_SOCKET env var or --socket flag
```

**Example curl debug session:**

```bash
export SOCK=/run/user/1000/memo/memod.sock
export TOK=memo_abc123...

# list mount root
curl --unix-socket $SOCK \
  -H "Authorization: Bearer $TOK" \
  "http://localhost/v1/fs/ls?path=VaultKB:/"

# write a file
curl --unix-socket $SOCK \
  -X PUT \
  -H "Authorization: Bearer $TOK" \
  --data-binary @notes.md \
  "http://localhost/v1/fs/write?path=VaultKB:/notes/git.md"
```

---

## 9. Path Validation & Security

Path validation is the most security-critical component. It runs before any filesystem syscall. All logic is in `policy/path.rs`.

**Validation steps (in order):**

1. **Parse mount prefix** — split `VaultKB:/notes/git.md` into mount name `VaultKB` and relative path `notes/git.md`.
2. **Reject empty or invalid mount name** — alphanumeric + dash + underscore only.
3. **Reject absolute paths** — relative path must not start with `/` after the colon.
4. **Reject `..` components** — split by `/`, reject any component equal to `..` or `.`.
5. **Join with mount root** — `mount.root_path.join(relative_path)`.
6. **Canonicalize** — call `std::fs::canonicalize` (or `tokio::fs::canonicalize`) to resolve the real path. **This call happens after the logical checks.**
7. **Verify prefix** — assert that `canonical_path.starts_with(&mount.root_path_canonical)`. If not: `out_of_bounds` error.
8. **Check symlink** — after canonicalization, verify the original `join` result matches canonical. If they differ, a symlink was traversed: reject.
9. **Apply policy globs** — check hide, deny-read, deny-write globs against the relative path component.
10. **Check size limits** — for reads: stat file, compare against `max_read_bytes`. For writes: check `Content-Length` or streamed byte count against `max_write_bytes`.

**Symlink detection detail:**

```rust
let joined = mount_root.join(&relative);           // logical path
let canonical = tokio::fs::canonicalize(&joined).await?;
let root_canonical = tokio::fs::canonicalize(&mount_root).await?;

// Symlink check: if the joined path != canonical, something was resolved
// We re-check each path component to detect symlinks
if joined != canonical {
    return Err(PolicyError::SymlinkDenied);
}
if !canonical.starts_with(&root_canonical) {
    return Err(PolicyError::OutOfBounds);
}
```

**Glob matching** uses the `globset` crate. Globs are compiled at mount load time and cached.

**Policy enforcement is uniform** — the same `PolicyEngine::check(mount, relative_path, operation)` is called for every operation type. There is no operation that bypasses policy.

---

## 10. Data Model (SQLite)

Database location: `~/.local/share/memo/memo.db` (overridable via config).

WAL mode enabled on open: `PRAGMA journal_mode=WAL;`

Foreign keys enabled: `PRAGMA foreign_keys=ON;`

```sql
CREATE TABLE mounts (
  name            TEXT PRIMARY KEY,
  root_path       TEXT NOT NULL,
  mode            TEXT NOT NULL CHECK(mode IN ('ro', 'rw')),
  audience        TEXT NOT NULL CHECK(audience IN ('shared', 'agent-only', 'human-only')),
  description     TEXT,
  hide_globs      TEXT NOT NULL DEFAULT '[]',       -- JSON array of glob strings
  deny_read_globs TEXT NOT NULL DEFAULT '[]',       -- JSON array of glob strings
  deny_write_globs TEXT NOT NULL DEFAULT '[]',      -- JSON array of glob strings
  max_read_bytes  INTEGER,                          -- NULL = no limit
  max_write_bytes INTEGER,                          -- NULL = no limit
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tokens (
  id           TEXT PRIMARY KEY,                   -- random UUID v4
  name         TEXT NOT NULL,                      -- human-readable label
  hash         TEXT NOT NULL,                      -- Argon2id hash of raw token
  scopes       TEXT NOT NULL,                      -- JSON array of scope strings
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  expires_at   TEXT,                               -- NULL = no expiry; ISO 8601
  last_used_at TEXT                                -- updated on each verified use
);

CREATE TABLE audit_log (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp  TEXT NOT NULL DEFAULT (datetime('now')),
  token_id   TEXT,                                 -- NULL if auth failed
  operation  TEXT NOT NULL,                        -- ls, read, write, mv, rm, cp, mkdir, grep, find, stat
  mount      TEXT,
  path       TEXT,
  result     TEXT NOT NULL CHECK(result IN ('ok', 'error')),
  error_code TEXT,                                 -- NULL if result = 'ok'
  details    TEXT                                  -- JSON blob for extra context
);

CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
CREATE INDEX idx_audit_log_mount     ON audit_log(mount);
CREATE INDEX idx_audit_log_token_id  ON audit_log(token_id);
```

**Migrations** are embedded in the binary using `include_str!` and applied at startup via a simple version table:

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

---

## 11. API Reference

Base URL (over Unix socket): `http://localhost`

All requests require `Authorization: Bearer <token>`.

### 11.1 Filesystem Endpoints

#### `GET /v1/fs/ls`

List directory contents.

**Query params:**

| Param | Required | Description |
|-------|----------|-------------|
| `path` | yes | `MountName:/relative/path` |
| `info` | no | `true` to include `index.md` summary for directories |

**Response `200`:**

```json
{
  "path": "VaultKB:/notes",
  "entries": [
    {"name": "git.md",  "kind": "file", "size": 4096, "modified_at": "2024-01-15T10:30:00Z"},
    {"name": "docker",  "kind": "dir",  "modified_at": "2024-01-14T09:00:00Z"}
  ]
}
```

---

#### `GET /v1/fs/tree`

Recursive directory tree.

**Query params:**

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `path` | yes | — | `MountName:/relative/path` |
| `depth` | no | `3` | Max recursion depth |

**Response `200`:**

```json
{
  "path": "VaultKB:/",
  "depth": 3,
  "tree": {
    "name": "",
    "kind": "dir",
    "children": [
      {
        "name": "notes",
        "kind": "dir",
        "children": [
          {"name": "git.md", "kind": "file", "size": 4096, "modified_at": "..."}
        ]
      }
    ]
  }
}
```

---

#### `GET /v1/fs/stat`

Stat a single path.

**Query params:** `path`

**Response `200`:**

```json
{
  "path": "VaultKB:/notes/git.md",
  "kind": "file",
  "size": 4096,
  "modified_at": "2024-01-15T10:30:00Z",
  "created_at": "2024-01-10T08:00:00Z",
  "memo_summary": null
}
```

For directories with `index.md`, `memo_summary` contains the `memo.summary` frontmatter value.

---

#### `GET /v1/fs/read`

Read file contents. Streamed via chunked transfer for large files.

**Query params:** `path`

**Response `200`:** Raw file bytes. `Content-Type: application/octet-stream` (or detected MIME type). `Content-Length` set when known.

---

#### `PUT /v1/fs/write`

Write file. Creates parent directories if they do not exist. Atomic write-by-rename.

**Query params:** `path`

**Body:** Raw file bytes.

**Response `200`:**

```json
{"path": "VaultKB:/notes/git.md", "written_bytes": 4096}
```

---

#### `POST /v1/fs/mkdir`

Create directory (and intermediate directories).

**Query params:** `path`

**Response `200`:**

```json
{"path": "VaultKB:/notes/new-dir", "created": true}
```

---

#### `POST /v1/fs/mv`

Move or rename. Both `src` and `dst` must be within the same mount.

**Query params:** `src`, `dst`

**Response `200`:**

```json
{"src": "VaultKB:/drafts/x.md", "dst": "VaultKB:/notes/x.md"}
```

---

#### `DELETE /v1/fs/rm`

Delete file or empty directory. Add `?recursive=true` to delete non-empty directories.

**Query params:** `path`, `recursive` (optional, default `false`)

**Response `200`:**

```json
{"path": "VaultKB:/drafts/old.md", "deleted": true}
```

---

#### `POST /v1/fs/cp`

Copy file. `src` and `dst` may be within different mounts (if token has scope for both).

**Query params:** `src`, `dst`

**Response `200`:**

```json
{"src": "VaultKB:/notes/a.md", "dst": "VaultKB:/archive/a.md"}
```

---

#### `GET /v1/fs/grep`

Text search within a mount path.

**Query params:**

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `path` | yes | — | Search root |
| `pattern` | yes | — | Regex or literal string |
| `recursive` | no | `true` | Recurse into subdirectories |
| `case_sensitive` | no | `true` | |
| `max_results` | no | `100` | Cap on returned matches |

**Response `200`:**

```json
{
  "pattern": "kubernetes",
  "matches": [
    {
      "path": "VaultKB:/notes/k8s.md",
      "line": 42,
      "content": "Kubernetes uses declarative configuration..."
    }
  ]
}
```

---

#### `GET /v1/fs/find`

Glob-based filename search.

**Query params:**

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `path` | yes | — | Search root |
| `glob` | yes | — | Glob pattern (e.g. `*.md`) |
| `max_results` | no | `500` | |

**Response `200`:**

```json
{
  "glob": "*.md",
  "results": [
    {"path": "VaultKB:/notes/git.md", "size": 4096, "modified_at": "..."},
    {"path": "VaultKB:/notes/docker.md", "size": 2048, "modified_at": "..."}
  ]
}
```

---

### 11.2 Meta Endpoints

Meta endpoints require tokens with `admin:*` or `meta:read` scopes.

#### `GET /v1/meta/mounts`

**Response `200`:**

```json
{
  "mounts": [
    {
      "name": "VaultKB",
      "root_path": "/Users/me/Obsidian/Vault/SharedKB",
      "mode": "rw",
      "audience": "shared",
      "description": "Shared knowledge base",
      "created_at": "2024-01-01T00:00:00Z"
    }
  ]
}
```

---

#### `POST /v1/meta/mounts`

Register a new mount.

**Body:**

```json
{
  "name": "VaultKB",
  "root_path": "/Users/me/Obsidian/Vault/SharedKB",
  "mode": "rw",
  "audience": "shared",
  "description": "Shared knowledge base",
  "hide_globs": [".obsidian/**", "*.private.md"],
  "deny_read_globs": [],
  "deny_write_globs": ["*.png", "*.jpg"],
  "max_read_bytes": null,
  "max_write_bytes": 10485760
}
```

**Response `201`:** Mount object as registered.

---

#### `GET /v1/meta/mounts/:name`

**Response `200`:** Full mount object including all policy fields.

---

#### `DELETE /v1/meta/mounts/:name`

Remove mount registration. Does not delete files.

**Response `200`:**

```json
{"name": "VaultKB", "removed": true}
```

---

#### `GET /v1/meta/tokens`

**Response `200`:**

```json
{
  "tokens": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "claude-agent",
      "scopes": ["fs:VaultKB:read", "fs:VaultKB:write"],
      "created_at": "2024-01-01T00:00:00Z",
      "expires_at": null,
      "last_used_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

Raw token values are never returned after creation.

---

#### `POST /v1/meta/tokens`

**Body:**

```json
{
  "name": "claude-agent",
  "scopes": ["fs:VaultKB:read", "fs:VaultKB:write"],
  "expires_at": null
}
```

**Response `201`:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "claude-agent",
  "token": "memo_abc123xyz...",
  "scopes": ["fs:VaultKB:read", "fs:VaultKB:write"],
  "created_at": "2024-01-15T10:00:00Z",
  "expires_at": null
}
```

`token` is returned **once only**. Store it immediately.

---

#### `DELETE /v1/meta/tokens/:id`

Revoke token by UUID.

**Response `200`:**

```json
{"id": "550e8400-e29b-41d4-a716-446655440000", "revoked": true}
```

---

#### `GET /v1/meta/audit`

**Query params:**

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| `mount` | no | — | Filter by mount name |
| `token_id` | no | — | Filter by token UUID |
| `operation` | no | — | Filter by operation type |
| `result` | no | — | `ok` or `error` |
| `limit` | no | `100` | Max results |
| `before` | no | — | ISO 8601 timestamp upper bound |

**Response `200`:**

```json
{
  "entries": [
    {
      "id": 1,
      "timestamp": "2024-01-15T10:30:00Z",
      "token_id": "550e8400-...",
      "operation": "read",
      "mount": "VaultKB",
      "path": "/notes/git.md",
      "result": "ok",
      "error_code": null
    }
  ]
}
```

---

## 12. Token & Auth Model

### Token Format

Raw tokens use the format `memo_<random_base62_32chars>`. This prefix aids identification and avoids confusion with other credential types. Example: `memo_aB3xK9mNpQ2rS7tU4vW1yZ6`.

### Token Storage (Daemon Side)

Tokens are hashed with Argon2id before storage. Parameters: `m=19456` (19 MiB), `t=2`, `p=1` (OWASP recommended minimum).

```sql
-- tokens table: hash column stores argon2id PHC string
hash TEXT NOT NULL   -- e.g. "$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>"
```

`last_used_at` is updated on every successful verification (non-atomic update is acceptable — approximate recency is sufficient).

### Scope Format

Scopes are strings of the form `<namespace>:<resource>:<action>`:

| Scope | Grants |
|-------|--------|
| `fs:<MountName>:read` | ls, tree, stat, read, grep, find on named mount |
| `fs:<MountName>:write` | write, mkdir, mv, rm, cp on named mount |
| `fs:*:read` | read access on all mounts |
| `fs:*:write` | write access on all mounts |
| `meta:read` | list mounts, list tokens (no raw token values), read audit log |
| `admin:mounts` | register, update, delete mounts |
| `admin:tokens` | create, revoke tokens |
| `admin:*` | all admin operations |

**Scope checking** is a simple set membership check: the token's scopes must contain the required scope for the operation being performed.

### Token Storage (Client Side)

- **Human (CLI):** macOS Keychain via `security` CLI (v1 best-effort); fallback to `~/.config/memo/tokens/<name>.token` at mode `0600`.
- **Agent:** `MEMO_TOKEN` environment variable. Fallback to file.

### Bootstrap

The first time `memod` is initialized with no tokens in the database, it generates an `admin` token with all scopes, prints it once to stdout, and halts. The operator stores this token and uses it to provision further tokens.

---

## 13. Error Model

All error responses have HTTP status ≥ 400 and body:

```json
{
  "error": {
    "code": "not_found",
    "message": "Path does not exist: /notes/missing.md",
    "mount": "VaultKB",
    "path": "/notes/missing.md"
  }
}
```

`mount` and `path` fields are omitted when not applicable (e.g. auth errors).

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `auth_required` | 401 | Missing or malformed `Authorization` header |
| `token_invalid` | 401 | Token not found or hash mismatch |
| `token_expired` | 401 | Token past `expires_at` |
| `permission_denied` | 403 | Token lacks required scope |
| `policy_violated` | 403 | Operation blocked by mount policy (deny glob, ro mode) |
| `invalid_path` | 400 | Malformed path syntax (bad mount prefix, empty, etc.) |
| `out_of_bounds` | 403 | Resolved path escapes mount root |
| `symlink_denied` | 403 | Path resolves through a symlink |
| `not_found` | 404 | Path does not exist |
| `mount_not_found` | 404 | Named mount is not registered |
| `conflict` | 409 | Target path already exists (e.g. mkdir on existing file) |
| `too_large` | 413 | File exceeds mount `max_read_bytes` or `max_write_bytes` |
| `internal_error` | 500 | Unhandled daemon error |

### CLI Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error (see stderr) |
| 2 | Auth error |
| 3 | Policy / permission error |
| 4 | Not found |
| 5 | Daemon not running |

Structured error (JSON mode):

```json
{
  "error": {
    "code": "not_found",
    "message": "Path does not exist: /notes/missing.md",
    "mount": "VaultKB",
    "path": "/notes/missing.md"
  }
}
```

---

## 14. CLI Reference

Global flags apply to all commands:

```
--json              Structured JSON output
--socket <path>     Unix socket path (overrides env/config)
--token <token>     Auth token (overrides env/config)
```

### Filesystem Commands

```bash
memo ls VaultKB:/
memo ls --info VaultKB:/notes          # include index.md summary
memo ls --json VaultKB:/notes

memo tree VaultKB:/
memo tree VaultKB:/ --depth 2

memo cat VaultKB:/notes/git.md
memo cat --json VaultKB:/notes/git.md  # base64-encodes content in JSON

memo write VaultKB:/notes/git.md < input.md
memo write VaultKB:/notes/git.md --file ./local.md

memo mkdir VaultKB:/notes/new-topic

memo mv VaultKB:/drafts/x.md VaultKB:/notes/x.md

memo rm VaultKB:/drafts/old.md
memo rm --recursive VaultKB:/drafts/

memo cp VaultKB:/notes/a.md VaultKB:/archive/a.md
memo cp ./local.png VaultKB:/assets/image.png      # local file → mount

memo grep "kubernetes" VaultKB:/
memo grep --no-recursive "pattern" VaultKB:/notes
memo grep --case-insensitive "TODO" VaultKB:/

memo find "*.md" VaultKB:/notes
memo find "*.md" VaultKB:/ --json

memo info VaultKB:/notes               # stat + index.md summary
```

### Admin Commands

```bash
# Mount management
memo mount list
memo mount add --name VaultKB --path /Users/me/Obsidian/Vault/SharedKB --mode rw --audience shared
memo mount add --name AgentData --path ~/.local/share/memo/agent --mode rw --audience agent-only
memo mount remove VaultKB
memo mount show VaultKB

# Token management
memo token list
memo token create --name claude-agent --scopes "fs:VaultKB:read,fs:VaultKB:write"
memo token create --name human-admin --scopes "admin:*" --expires 2024-12-31T00:00:00Z
memo token revoke <token-id>

# Daemon
memo daemon start
memo daemon stop
memo daemon status
memo daemon logs --tail 50
```

---

## 15. Mount Configuration Schema

Mounts are stored in SQLite (not in config files). Registration happens via `memo mount add` or `POST /v1/meta/mounts`.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Unique identifier. Alphanumeric, `-`, `_`. Max 64 chars. |
| `root_path` | string | Absolute path to mount root. Must exist at registration time. |
| `mode` | `ro` \| `rw` | Read-only or read-write. |
| `audience` | `shared` \| `agent-only` \| `human-only` | Informational; affects UI presentation. Does not enforce access by itself — scopes do. |
| `description` | string? | Human-readable description. |
| `hide_globs` | string[] | Paths matching these globs are excluded from `ls`/`tree`/`find` results and return `not_found` on direct access. |
| `deny_read_globs` | string[] | Direct access denied for matching paths; `policy_violated` error. |
| `deny_write_globs` | string[] | Write/move/delete denied for matching paths. |
| `max_read_bytes` | integer? | Max file size for reads. `null` = no limit. |
| `max_write_bytes` | integer? | Max file size for writes. `null` = no limit. |

### Example: Obsidian Shared KB

```json
{
  "name": "VaultKB",
  "root_path": "/Users/me/Obsidian/Vault/SharedKB",
  "mode": "rw",
  "audience": "shared",
  "description": "Shared knowledge base — human + agent collaborative",
  "hide_globs": [".obsidian/**", ".DS_Store", "*.private.md", "_drafts/**"],
  "deny_read_globs": [],
  "deny_write_globs": [],
  "max_read_bytes": null,
  "max_write_bytes": 10485760
}
```

### Example: Agent Scratch Space

```json
{
  "name": "AgentData",
  "root_path": "/Users/me/.local/share/memo/agent",
  "mode": "rw",
  "audience": "agent-only",
  "description": "Agent scratch space — not human-visible",
  "hide_globs": [],
  "deny_read_globs": [],
  "deny_write_globs": [],
  "max_read_bytes": 1073741824,
  "max_write_bytes": 1073741824
}
```

### Daemon Config File

`~/.config/memo/config.toml`:

```toml
[daemon]
socket_path = ""          # empty = use XDG_RUNTIME_DIR default
db_path = ""              # empty = use XDG_DATA_HOME default
log_level = "info"        # trace | debug | info | warn | error

[daemon.write]
fsync = false             # enable fsync before rename (safer, slower)
dir_sync = false          # sync parent directory after rename

[daemon.limits]
max_audit_log_rows = 100000   # rows before oldest are pruned
```

---

## 16. Observability & Audit

### Audit Log

Every operation (successful or failed) writes one row to `audit_log`. This includes auth failures. Auth failures write `token_id = NULL`.

The audit log is queryable via `GET /v1/meta/audit` and `memo audit` (future CLI command). In v1, no automatic pruning — the `max_audit_log_rows` config triggers a prune on startup.

### Structured Logging (memod)

`memod` uses `tracing` + `tracing-subscriber` with configurable log level. Output: JSON lines to stderr (suitable for `systemd` or log aggregation) or human-readable for development.

Format (JSON):

```json
{
  "timestamp": "2024-01-15T10:30:00.123Z",
  "level": "INFO",
  "target": "memod::fs::ops",
  "message": "read completed",
  "mount": "VaultKB",
  "path": "/notes/git.md",
  "bytes": 4096,
  "duration_ms": 2
}
```

### Health Check

`GET /health` — no auth required. Returns `{"status": "ok", "version": "0.1.0"}`.

Used by `memo daemon status` to verify the daemon is reachable.

---

## 17. Repository Layout

```
memo/
├── Cargo.toml              # workspace [workspace] members = ["crates/*"]
├── Cargo.lock
├── crates/
│   ├── memod/              # daemon binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs     # startup, config, signal handlers
│   │       ├── server.rs   # axum router, middleware, request tracing
│   │       ├── fs/
│   │       │   ├── mod.rs  # FsService
│   │       │   ├── ops.rs  # ls, stat, read, write, mkdir, mv, rm, cp
│   │       │   ├── grep.rs
│   │       │   ├── find.rs
│   │       │   └── atomic.rs
│   │       ├── policy/
│   │       │   ├── mod.rs  # PolicyEngine
│   │       │   └── path.rs # canonical path resolution, symlink check
│   │       ├── db/
│   │       │   ├── mod.rs  # DB pool, migrations
│   │       │   ├── mounts.rs
│   │       │   ├── tokens.rs
│   │       │   └── audit.rs
│   │       └── auth/
│   │           └── mod.rs  # token extraction + scope verification
│   ├── memo/               # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs     # clap app + global flags
│   │       ├── client.rs   # hyper UnixConnector, request builder
│   │       └── commands/
│   │           ├── ls.rs
│   │           ├── tree.rs
│   │           ├── cat.rs
│   │           ├── write.rs
│   │           ├── mkdir.rs
│   │           ├── mv.rs
│   │           ├── rm.rs
│   │           ├── cp.rs
│   │           ├── grep.rs
│   │           ├── find.rs
│   │           ├── info.rs
│   │           ├── mount.rs
│   │           ├── token.rs
│   │           └── daemon.rs
│   ├── memo-ui/            # Tauri admin UI
│   │   ├── Cargo.toml
│   │   ├── src-tauri/
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── main.rs
│   │   │       └── commands.rs   # Tauri commands wrapping memod HTTP calls
│   │   └── src/                  # web frontend
│   │       ├── index.html
│   │       ├── app.js
│   │       └── style.css
│   └── memo-core/          # shared types, protocol structs, error types
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── types.rs    # Mount, Token, AuditEntry, DirEntry structs
│           ├── errors.rs   # ErrorCode enum, ApiError struct
│           └── scopes.rs   # scope parsing + checking
├── docs/
│   ├── vision.md
│   └── system-design-v1.md
├── tests/
│   ├── integration/
│   │   ├── fs_ops.rs       # full round-trip: ls, read, write, mv, rm
│   │   ├── policy.rs       # path traversal, glob policy
│   │   ├── auth.rs         # token verification, scope enforcement
│   │   └── atomic.rs       # write atomicity
│   └── fixtures/
│       └── sample_vault/   # fixture directory tree for tests
└── scripts/
    ├── setup.sh            # create XDG dirs, generate initial token
    └── dev.sh              # start memod in debug mode with test config
```

**Cargo workspace `Cargo.toml`:**

```toml
[workspace]
resolver = "2"
members = [
  "crates/memod",
  "crates/memo",
  "crates/memo-ui/src-tauri",
  "crates/memo-core",
]

[workspace.dependencies]
tokio        = { version = "1", features = ["full"] }
axum         = { version = "0.7", features = ["macros"] }
hyper        = { version = "1" }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
rusqlite     = { version = "0.31", features = ["bundled"] }
argon2       = "0.5"
globset      = "0.4"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
uuid         = { version = "1", features = ["v4"] }
clap         = { version = "4", features = ["derive"] }
toml         = "0.8"
```

---

## 18. Testing Strategy

### Unit Tests

Co-located with source in `#[cfg(test)]` modules.

**Path validation (`policy/path.rs`):**

```rust
#[test]
fn rejects_dotdot_traversal() {
    assert!(validate_relative("../secret").is_err());
    assert!(validate_relative("notes/../../etc/passwd").is_err());
}

#[test]
fn rejects_absolute_path() {
    assert!(validate_relative("/etc/passwd").is_err());
}

#[test]
fn accepts_valid_paths() {
    assert!(validate_relative("notes/git.md").is_ok());
    assert!(validate_relative("a/b/c/d.txt").is_ok());
}
```

**Glob policy evaluation (`policy/mod.rs`):**

- Test each deny/hide glob category with matching and non-matching paths
- Test glob compilation failure handling

**Token scope checking (`memo-core/scopes.rs`):**

- `fs:VaultKB:read` grants read on VaultKB, not write, not other mounts
- `fs:*:read` grants read on any mount
- Expired token is rejected regardless of scope

### Integration Tests (`tests/integration/`)

Each test:

1. Creates a temporary directory as mount root
2. Creates a temporary SQLite database
3. Starts `memod` bound to a temp Unix socket
4. Runs operations via direct HTTP (using `reqwest` with Unix socket support) or the `memo` CLI binary
5. Asserts responses and filesystem state
6. Tears down daemon and temp files

**Test cases:**

| Suite | Key cases |
|-------|-----------|
| `fs_ops` | ls empty dir, ls with entries, read existing file, read missing file, write new file, overwrite existing, mkdir, mv file, mv dir, rm file, rm non-empty dir without recursive flag (expect error), cp within mount |
| `policy` | `..` in path, absolute path in path param, path escaping mount root via symlink (setup symlink in temp dir), hide glob hides file from ls, deny_read glob blocks read, deny_write glob blocks write, ro mount rejects writes |
| `auth` | missing token → 401, invalid token → 401, expired token → 401, wrong scope → 403, correct scope → 200 |
| `atomic` | write fails partway through: temp file is cleaned up, target is unchanged; concurrent writes do not corrupt target |

### Security Tests

Run as part of integration suite, tagged `#[test] #[ignore = "security"]` and enabled in CI:

- **Traversal corpus:** A list of path strings known to be used in traversal attacks (`../`, `%2e%2e%2f`, null bytes, etc.) — all must return `invalid_path` or `out_of_bounds`.
- **Symlink attack:** Create a symlink inside the mount root pointing outside; attempt to read via symlink path — must be rejected.
- **Unicode normalization:** Paths with Unicode normalization differences are handled consistently.

### Atomic Write Tests

- Write a file; interrupt with `SIGKILL` simulation (close FD before rename); verify no partial file at target path and temp file is absent.
- Concurrent writers to the same path: final state must be one complete file (not corrupt).

---

## 19. Open Questions

1. **`memo-ui` socket access:** Should `memo-ui` connect to `memod` over the Unix socket directly (same as CLI), or should `memod` optionally expose a loopback TCP port (e.g. `127.0.0.1:18301`) for UI convenience? Direct Unix socket is simpler and avoids any network surface. The Tauri Rust backend can use the same `hyper` + `UnixConnector` approach as the CLI — this is the preferred path unless it proves unworkable.

2. **Token expiry defaults:** Should `POST /v1/meta/tokens` accept a `default_ttl_days` config, or should tokens always be non-expiring unless `expires_at` is explicitly set? The current design defaults to non-expiring; a sensible alternative is a 90-day default for agent tokens.

3. **Daemon auto-start:** Should the `memo` CLI auto-start `memod` if the socket is unreachable (like Docker)? This is ergonomic but adds complexity (process management, stdout capture for the initial admin token). Alternative: require manual `memo daemon start` or a launchd/systemd service.

4. **`SIGHUP` config reload:** Should `memod` support `SIGHUP` to reload `config.toml` without restart? Mount and token changes already take effect immediately (SQLite reads per request). The only config that would benefit from reload is log level and write options. Low priority for v1.

5. **macOS Keychain integration:** Store human tokens in the macOS Keychain (via `security add-generic-password`) in v1, or defer to v2 and use the `~/.config/memo/tokens/` file fallback exclusively? Keychain adds a dependency on macOS-specific APIs but improves security posture for the human operator.
