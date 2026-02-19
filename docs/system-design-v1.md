# memo — System Design v1

## 1. Overview

`memo` is a unified memory layer for collaborative human–agent knowledge work. It gives humans and LLM agents a shared, policy-controlled space where both can read, write, and build a common knowledge base — without exposing unrelated personal files.

In v1, memory is **file-based**: regular files and directories accessed through named mounts. Files are the right starting primitive because they are native to human tools (Obsidian, editors) and equally accessible to agents. Future versions will extend the memory model with richer types (entities, graph, embeddings), but v1 is deliberately file-only and Markdown-native.

It follows a client–server model with three binaries:

- **`memod`** — daemon process; owns all filesystem I/O; the only process that touches the real filesystem
- **`memo`** — CLI client for humans and agents
- **`memo-ui`** — Tauri v2 native macOS desktop application for managing mounts, tokens, and audit log; not a general file editor
- **`memo-client`** — shared Rust library crate; typed REST HTTP client (`reqwest`-based), used by both `memo` CLI and `memo-ui` backend
- **`memo-core`** — shared domain model: aggregates (`Mount`, `Token`), value objects (`MountPath`, `RelativePath`, `Scope`), repository interfaces, domain events, and error types; no I/O

**Primary platform: macOS.** Linux is a supported secondary target. Windows is out of scope.

All IPC is over **REST HTTP/1.1 on loopback TCP (`127.0.0.1:18301`)**. The daemon binds to localhost only — no external network exposure. No shell execution. No virtual filesystem.

**Implementation language: Rust** across the entire stack. Rationale:

- Memory safety without GC — critical for a daemon doing path manipulation
- Single static binary output for `memod` and `memo`
- Tauri requires Rust for the backend; using Rust everywhere means one toolchain (Cargo workspace)
- Strong ecosystem for async HTTP (`axum`), SQLite (`sqlx`), and path handling

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
- Exposure beyond loopback (TLS, external TCP, internet-facing)
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
| FR-12 | `memo-ui` provides a native macOS desktop application (Tauri v2) for mount and token management |
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
│                        │  REST HTTP/1.1 + JSON                  │
│                        │  127.0.0.1:18301                       │
│              ┌─────────▼──────────┐                            │
│              │       memod        │                            │
│              │   (axum/tokio)     │                            │
│              │                   │                            │
│              │  ┌─────────────┐  │                            │
│              │  │  auth layer │  │                            │
│              │  └──────┬──────┘  │                            │
│              │         │         │                            │
│              │  ┌──────▼──────┐  │     ┌─────────────────┐   │
│              │  │policy layer │  │     │     SQLite       │   │
│              │  └──────┬──────┘  │────▶│  mounts         │   │
│              │         │         │     │  tokens         │   │
│              │  ┌──────▼──────┐  │     │  metainfo       │
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

**Default bind address:** `127.0.0.1:18301` (loopback only; configurable via `daemon.bind_addr` in config)

**Config file:** `~/.config/memo/config.toml`

**Database:** `~/.local/share/memo/memo.db` (XDG data dir, overridable in config)

---

## 7. Domain Model

The system is organized around four bounded contexts. Each context owns its domain concepts and is responsible for enforcing its invariants. Inter-context communication happens through application services and domain events — not through shared mutable state.

### 7.1 Bounded Contexts

| Context | Responsibility | Core Concepts |
|---------|---------------|---------------|
| **Access Control** | Token lifecycle, scope resolution, authentication decisions | `Token`, `Scope`, `TokenId`, `BearerToken` |
| **Mount Registry** | Mount configuration, policy enforcement, path resolution | `Mount`, `MountPolicy`, `MountName`, `GlobPolicy` |
| **File System** | File I/O operations, directory traversal, atomic writes | `FileEntry`, `DirListing`, `FileContent`, `RelativePath` |
| **Audit** | Operation recording, querying, retention | `AuditEvent`, `AuditEntry`, `AuditLog` |

### 7.2 Aggregates

**`Mount` (Mount Registry BC)**

Root entity identified by `MountName`. Owns its policy as a value object. Invariants:

- `root_path` must be an existing absolute directory at registration time
- `name` must be unique across all mounts; alphanumeric + `-` + `_`, max 64 chars
- `mode` is immutable after registration (`name` and `root_path` are also immutable — remove and re-add to change them)

Key behavior:

- `mount.policy.check_read(path)` — evaluates hide and deny-read globs
- `mount.policy.check_write(path, size)` — evaluates deny-write globs and size limits
- `mount.resolve(path: RelativePath) -> AbsolutePath` — joins root with validated relative path

**`Token` (Access Control BC)**

Root entity identified by `TokenId` (UUID v4 newtype). Owns its scopes as a `ScopeSet` value object. Raw token value is never stored — only the Argon2id hash.

Key behavior:

- `token.has_scope(required: &Scope) -> bool` — scope membership check
- `token.is_expired() -> bool` — compares `expires_at` against current time
- `token.verify(raw: &str) -> Result<()>` — Argon2id hash comparison, dispatched via `spawn_blocking`

### 7.3 Value Objects

Value objects are immutable and validated at construction. An invalid instance cannot exist.

| Value Object | Validation enforced at construction | Location |
|---|---|---|
| `MountName` | Alphanumeric + `-` + `_`, max 64 chars, non-empty | `memo-core/mount.rs` |
| `RelativePath` | No `..` components, no absolute prefix, no null bytes | `memo-core/path.rs` |
| `MountPath` | Parses `MountName:/relative/path`; delegates to `MountName` and `RelativePath` | `memo-core/path.rs` |
| `Scope` | Typed enum: `Fs { mount, action }` \| `Admin(_)` \| `Meta(_)` | `memo-core/scope.rs` |
| `TokenId` | UUID v4 newtype | `memo-core/token.rs` |

`RelativePath` and `MountPath` handle validation steps 1–4 of the path security model (parse mount prefix, reject absolute paths, reject `..` components — see Section 10). Steps 5–10 (canonicalize, symlink check, glob policy) remain in `PolicyEngine` because they require I/O or compiled glob state.

### 7.4 Repository Interfaces

Defined in `memo-core` (traits only, no I/O). SQLite implementations live in `memod`. This separation allows integration tests to inject in-memory stubs without SQLite.

```rust
#[async_trait]
pub trait MountRepository: Send + Sync {
    async fn find(&self, name: &MountName) -> Result<Mount, DbError>;
    async fn list(&self) -> Result<Vec<Mount>, DbError>;
    async fn save(&self, mount: &Mount) -> Result<(), DbError>;
    async fn delete(&self, name: &MountName) -> Result<(), DbError>;
}

#[async_trait]
pub trait TokenRepository: Send + Sync {
    async fn find(&self, id: &TokenId) -> Result<Token, DbError>;
    async fn verify(&self, raw_token: &str) -> Result<Token, AuthError>; // Argon2id verify
    async fn list(&self) -> Result<Vec<TokenView>, DbError>;             // no raw hashes
    async fn save(&self, token: &Token) -> Result<(), DbError>;
    async fn delete(&self, id: &TokenId) -> Result<(), DbError>;
    async fn touch_last_used(&self, id: &TokenId) -> Result<(), DbError>;
}
```

`touch_last_used` errors are non-fatal for request success, but they are still returned so callers can log and observe persistence problems.

The `PolicyCache` (`DashMap<MountName, Arc<CompiledMount>>`) is an implementation detail of `SqliteMountRepository` — invisible to the domain.

### 7.5 Application Services

Application services coordinate domain objects and infrastructure. Each bounded context exposes one. They sit between HTTP handlers and the domain — handlers are thin HTTP adapters that extract parameters, call a service method, and serialize the response.

| Service | Coordinates |
|---------|-------------|
| `FileSystemService` | token verify → mount policy check → fs op → emit domain event |
| `MountService` | token verify (admin scope) → mount CRUD → cache invalidation |
| `TokenService` | token verify (admin scope) → token CRUD |
| `AuditService` | consumes domain events; appends to audit log; serves queries |

### 7.6 Domain Events

Domain events decouple operations from side effects. The Audit BC is the primary consumer in v1. The WebSocket audit tail (planned v2) adds a second consumer without changing the emitters.

```rust
pub enum DomainEvent {
    FileRead     { token_id: TokenId, mount: MountName, path: RelativePath, bytes: u64 },
    FileWritten  { token_id: TokenId, mount: MountName, path: RelativePath, bytes: u64 },
    DirListed    { token_id: TokenId, mount: MountName, path: RelativePath },
    MountRegistered { name: MountName, mode: MountMode },
    MountUpdated    { name: MountName },
    MountRemoved    { name: MountName },
    TokenCreated    { id: TokenId, name: String },
    TokenRevoked    { id: TokenId },
    AccessDenied { token_id: Option<TokenId>, reason: DenialReason, mount: Option<MountName> },
}
```

`DomainEvent` uses an internally tagged serde representation:

- Tag field: `type`
- Variant naming: `snake_case`

Example serialized event:

```json
{
  "type": "mount_registered",
  "name": "VaultKB",
  "mode": "read_write"
}
```

---

## 8. Component Design

### 8.1 memod (Daemon)

Single async binary built on `tokio` + `axum`. Listens on TCP loopback (`127.0.0.1:18301` by default). All filesystem I/O runs inside this process.

**Startup sequence:**

1. Load `~/.config/memo/config.toml`
2. Open log file at `$XDG_STATE_HOME/memo/memod.log` (default: `~/.local/state/memo/memod.log`) for appending
3. Write PID file to `$XDG_RUNTIME_DIR/memo/memod.pid` (used by `memo daemon status/stop`)
4. Open SQLite database (WAL mode, apply migrations)
5. If no tokens exist in DB: generate bootstrap admin token with all scopes, write raw token to `~/.config/memo/bootstrap.token` (mode `0600`), print file path to stderr. **Daemon continues running — it does not halt.**
6. Bind `axum` server to `127.0.0.1:18301` (or configured `bind_addr`)
7. Register signal handlers: `SIGTERM`/`SIGINT` for graceful shutdown
8. Spawn background task: prune `audit_log` if row count exceeds `max_audit_log_rows` (non-blocking — daemon is already serving by this point)

**Daemon launch mechanism (`memo daemon start`):**

On macOS, `memo daemon start` installs a launchd plist and loads it via `launchctl`. This gives automatic restart on crash, proper log routing, and correct user-session lifecycle — without requiring a separate process manager.

1. Write a plist to `~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist` with:
   - `Label`: `io.github.ch37n1.memo.memod`
   - `ProgramArguments`: path to `memod` binary
   - `RunAtLoad`: `false` (only run when loaded, not on every login)
   - `StandardOutPath` / `StandardErrorPath`: `~/.local/state/memo/memod.log`
2. Call `launchctl load ~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist`.

`memo daemon stop` calls `launchctl unload` on the same plist.
`memo daemon status` checks for the PID file at `$XDG_RUNTIME_DIR/memo/memod.pid` and calls `GET /health`.

**Internal modules (organized by bounded context):**

```
memod/src/
├── main.rs                     # startup sequence, config loading, signal handling
├── server.rs                   # axum router, middleware, request tracing; handlers are thin adapters
├── access_control/             # BC: token lifecycle and authentication decisions
│   ├── mod.rs                  # TokenService (application service)
│   ├── repository.rs           # SqliteTokenRepository: MountRepository impl
│   └── middleware.rs           # Bearer token extraction (axum middleware)
├── mount_registry/             # BC: mount configuration and policy enforcement
│   ├── mod.rs                  # MountService (application service)
│   ├── repository.rs           # SqliteMountRepository: MountRepository impl + PolicyCache
│   └── policy.rs               # PolicyEngine (domain service): steps 5–10 of path validation
│                               #   (canonicalize, symlink check, glob matching)
├── filesystem/                 # BC: file I/O operations
│   ├── mod.rs                  # FileSystemService (application service)
│   ├── ops.rs                  # ls, stat, read, write, mkdir, mv, rm, cp
│   ├── atomic.rs               # atomic write-by-rename (streaming AsyncRead, temp-file cleanup on error)
│   ├── grep.rs                 # text search (regex crate)
│   └── find.rs                 # glob filename search (walkdir + globset)
├── audit/                      # BC: operation recording and querying
│   ├── mod.rs                  # AuditService: domain event consumer + query handler
│   └── log.rs                  # append-only JSON lines writer; AtomicU64 sequential id
└── db/
    └── mod.rs                  # sqlx pool (SQLite WAL), migration runner (shared infra)
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

Accepts an `AsyncRead` source to support streaming without full in-memory buffering (satisfies NFR-05). On any error, the temp file is cleaned up before returning.

```rust
// atomic.rs
pub async fn atomic_write<R>(
    target: &Path,
    mut reader: R,
    fsync: bool,
    dir_sync: bool,
) -> Result<u64>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let dir = target.parent().ok_or(Error::InvalidPath)?;
    let tmp = dir.join(format!(".memo_tmp_{}", uuid::Uuid::new_v4().simple()));
    let result = async {
        let file = fs::File::create(&tmp).await?;
        let mut buffered = tokio::io::BufWriter::new(file);
        let written = tokio::io::copy(&mut reader, &mut buffered).await?;
        if fsync {
            buffered.into_inner().sync_all().await?;
        } else {
            buffered.flush().await?;
        }
        fs::rename(&tmp, target).await?;
        if dir_sync {
            // sync parent directory entry so rename is durable
            let dir_fd = fs::File::open(dir).await?;
            dir_fd.sync_all().await?;
        }
        Ok(written)
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&tmp).await; // best-effort cleanup
    }
    result
}
```

**Streaming reads:** For files above a configurable threshold (default: 4MB), reads are streamed using `axum::body::Body::from_stream`. Writes accept `axum::body::Bytes` streamed from request body.

### 8.2 memo (CLI)

Thin client binary. Reads token from environment variable or config file. Sends REST HTTP/1.1 requests to the daemon via the shared `memo-client` crate, which uses `reqwest` with base URL `http://127.0.0.1:18301`.

**Command dispatch:**

```
memo/src/
├── main.rs          # clap app, global flags (--json, --host, --port, --token)
└── commands/        # all commands use memo-client crate for HTTP transport
    ├── ls.rs
    ├── tree.rs
    ├── cat.rs
    ├── write.rs
    ├── mkdir.rs
    ├── mv.rs
    ├── rm.rs
    ├── cp.rs        # local→mount: reads local file, calls write endpoint; also mount→mount
    ├── grep.rs
    ├── find.rs
    ├── info.rs
    ├── mount.rs     # mount management (add, remove, list, update)
    ├── token.rs     # token management (create, revoke, list)
    └── audit.rs     # audit log viewer (reads from GET /v1/meta/audit)
```

**Token resolution order (CLI):**

1. `--token` flag
2. `MEMO_TOKEN` environment variable
3. `~/.config/memo/tokens/<name>.token` (mode `0600`)
4. macOS Keychain (service: `memo`, account: mount name) — deferred to v2

**Host/port resolution order (CLI):**

1. `--host` / `--port` flags
2. `MEMO_HOST` / `MEMO_PORT` environment variables
3. `[daemon] bind_addr` in `~/.config/memo/config.toml`
4. Default: `127.0.0.1:18301`

**Global flags:**

```
--json              Emit JSON output
--host <addr>       Override daemon host (default: 127.0.0.1)
--port <port>       Override daemon port (default: 18301)
--token <token>     Override token
--mount <name>      Default mount (reduces repetition)
```

### 8.3 memo-ui (Tauri v2 Desktop Admin UI)

**Tauri v2** native macOS desktop application. The Rust backend (`src-tauri`) connects to `memod` via REST HTTP using the shared `memo-client` crate (`reqwest`-based). The frontend (HTML/CSS/JS rendered in the webview) communicates with the Rust backend via Tauri v2 `invoke` commands.

**Scope in v1:** Mount management, token management, audit log viewer, basic file browser. Not a full editor — Obsidian handles editing for shared mounts.

**Admin token setup:** On first launch, `memo-ui` shows a setup screen prompting the user to paste their admin token (obtained from `~/.config/memo/bootstrap.token`). The token is stored via `tauri-plugin-store` in the macOS app data directory (sandboxed). All subsequent launches read the token from the store.

**macOS app bundle identifier:** `io.github.ch37n1.memo`

**All calls to `memod` are made via Tauri `invoke` commands in the Rust backend.** No JS-side `fetch` to `127.0.0.1:18301` is used. The CSP does not need `connect-src` for loopback access.

**Content Security Policy** (`tauri.conf.json`):

```json
"app": {
  "security": {
    "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'"
  }
}
```

**`tauri-plugin-store` registration** (required — the plugin does nothing without this):

```rust
// src-tauri/src/lib.rs
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![/* commands */])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

```js
// Frontend JS import
import { Store } from '@tauri-apps/plugin-store';
```

The npm package `@tauri-apps/plugin-store` must also be installed as a frontend dependency.

**macOS App Sandbox network entitlement** (`src-tauri/entitlements.macos.plist`):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.network.client</key>
    <true/>
</dict>
</plist>
```

Required for `reqwest` in the Tauri Rust backend to reach `127.0.0.1:18301`. Without this entitlement, all API calls fail with a sandbox violation. Reference in `tauri.conf.json`:

```json
"bundle": {
  "macOS": {
    "entitlements": "entitlements.macos.plist"
  }
}
```

**Frontend framework:** React + Vite (TypeScript). Build config in `tauri.conf.json`:

```json
"build": {
  "beforeDevCommand": "npm run dev",
  "beforeBuildCommand": "npm run build",
  "devUrl": "http://localhost:5173",
  "frontendDist": "../dist"
}
```

**Directory layout:**

```
memo-ui/
├── src-tauri/
│   ├── capabilities/
│   │   └── default.json         # Tauri v2 capability declarations (required)
│   ├── entitlements.macos.plist # macOS App Sandbox network entitlement
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs               # shared lib entry point (required for Tauri v2)
│   │   └── commands.rs          # Tauri invoke commands using memo-client
│   └── Cargo.toml               # crate-type = ["staticlib", "cdylib", "rlib"]
├── src/                         # React frontend (TypeScript)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   ├── MountList.tsx
│   │   ├── TokenList.tsx
│   │   └── AuditLog.tsx
│   └── hooks/
│       └── useMemoClient.ts     # typed wrappers around Tauri invoke commands
├── index.html
├── package.json
├── tsconfig.json
└── vite.config.ts
```

**`capabilities/default.json` (minimum):**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

Admin operations (mount registration, token creation) require tokens with `admin:*` scopes.

---

## 9. IPC Protocol

**Transport:** REST HTTP/1.1 over TCP loopback.

**Base URL:** `http://127.0.0.1:18301`

**Why REST over TCP loopback:**

- Debuggable with plain `curl` — no special flags, no custom transport
- `reqwest` in `memo-client` replaces the complex `hyper + UnixConnector` — much simpler client code
- LLM agents can call the API directly as standard HTTP — no special client library required
- `axum` with `TcpListener` is trivial; same router and handlers as any other axum server
- Loopback bind (`127.0.0.1`) means no external network surface; Bearer token auth provides the access control layer

**Content-Type:** `application/json` for all request and response bodies except raw file reads/writes.

**File read:** Response body is raw file bytes. `Content-Type` reflects file content where determinable, otherwise `application/octet-stream`.

**File write:** Request body is raw file bytes. `Content-Type` is ignored by the daemon.

**Auth header:** `Authorization: Bearer <token>` on every request.

**Address resolution (in order, highest priority first):**

```
Priority 1: --host / --port CLI flags
Priority 2: MEMO_HOST / MEMO_PORT environment variables
Priority 3: ~/.config/memo/config.toml [daemon] bind_addr
Default:    127.0.0.1:18301
```

**Example curl debug session:**

```bash
export BASE=http://127.0.0.1:18301
export TOK=memo_abc123...

# list mount root
curl -H "Authorization: Bearer $TOK" \
  "$BASE/v1/fs/ls?path=VaultKB:/"

# write a file
curl -X PUT \
  -H "Authorization: Bearer $TOK" \
  --data-binary @notes.md \
  "$BASE/v1/fs/write?path=VaultKB:/notes/git.md"

# health check (no auth)
curl "$BASE/health"
```

**Future: WebSocket (`/v1/ws`)**

Not in v1 scope, but the REST-over-TCP foundation makes WebSocket a natural addition. Planned use cases:

- **Live audit log tail** — push new `audit_log` rows to connected clients as they arrive, replacing polling `GET /v1/meta/audit`
- **File-change events** — push `fs::watch` notifications when files in mounted directories change (useful for agent awareness of human edits)
- **Streaming grep** — push grep matches incrementally as the search progresses over large trees

`axum` supports WebSocket natively via `axum::extract::ws`. The same Bearer token auth applies on the upgrade handshake.

---

## 10. Path Validation & Security

Path validation is the most security-critical component. It runs before any filesystem syscall. Steps 1–4 are enforced by value object constructors (`RelativePath`, `MountPath`) in `memo-core/path.rs`. Steps 5–10 require I/O or compiled glob state and live in `memod/mount_registry/policy.rs` (`PolicyEngine`).

**Validation steps (in order):**

1. **Parse mount prefix** — split `VaultKB:/notes/git.md` into mount name `VaultKB` and relative path `notes/git.md`.
2. **Reject empty or invalid mount name** — alphanumeric + dash + underscore only.
3. **Reject absolute paths** — relative path must not start with `/` after the colon.
4. **Reject `..` components** — split by `/`, reject any component equal to `..` or `.`.
5. **Join with mount root** — `mount.root_path.join(relative_path)`.
6. **Canonicalize** — for **read operations** (path must already exist): call `tokio::fs::canonicalize` on the full joined path. For **write operations** (target may not yet exist — `canonicalize` would fail on a new file): call `tokio::fs::canonicalize` on the *parent* directory only, then re-append the filename. **This call happens after the logical checks.**
7. **Verify prefix** — assert that `canonical_path.starts_with(&mount.root_path_canonical)`. If not: `out_of_bounds` error.
8. **Check symlink** — normalize the joined path with `path_clean::clean` (collapses `.` components without a syscall) before comparing to the canonical result. If they differ, a symlink was traversed: reject. Direct `joined != canonical` comparison produces false positives.
9. **Apply policy globs** — check hide, deny-read, deny-write globs against the relative path component.
10. **Check size limits** — for reads: stat file, compare against `max_read_bytes`. For writes: check `Content-Length` or streamed byte count against `max_write_bytes`.

**Symlink detection detail:**

`mount_root.join(&relative)` produces a non-normalized path (e.g. `/vault/notes/./git.md`). Comparing it directly to the canonicalized result produces false positives. Normalize with `path_clean::clean` before comparing.

`CompiledMount` holds the pre-computed `root_path_canonical: PathBuf` (resolved once at mount load time, stored in the `DashMap` cache). The per-request validation uses it directly — no extra `canonicalize` syscall on `mount_root` per request.

For **read operations** (path must already exist):

```rust
// mount.root_path_canonical is pre-computed at mount load time (no syscall here)
let joined = mount_root.join(&relative);
let normalized = path_clean::clean(&joined);       // collapse `.` without syscall
let canonical = tokio::fs::canonicalize(&joined).await?;

if normalized != canonical {
    return Err(PolicyError::SymlinkDenied);
}
if !canonical.starts_with(&mount.root_path_canonical) {
    return Err(PolicyError::OutOfBounds);
}
```

For **write operations** (target may not yet exist — `canonicalize` fails on new files):

```rust
let joined = mount_root.join(&relative);
let parent = joined.parent().ok_or(PolicyError::InvalidPath)?;
let filename = joined.file_name().ok_or(PolicyError::InvalidPath)?;

let canonical_parent = tokio::fs::canonicalize(parent).await
    .map_err(|_| PolicyError::NotFound)?;
let canonical = canonical_parent.join(filename);

// Symlink check on parent
let normalized_parent = path_clean::clean(parent);
if normalized_parent != canonical_parent {
    return Err(PolicyError::SymlinkDenied);
}
if !canonical.starts_with(&mount.root_path_canonical) {
    return Err(PolicyError::OutOfBounds);
}
```

**Glob matching** uses the `globset` crate. Globs are compiled at mount load time and cached.

**Policy enforcement is uniform** — the same `PolicyEngine::check(mount, relative_path, operation)` is called for every operation type. There is no operation that bypasses policy.

---

## 11. Data Model (SQLite)

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

-- Audit log is stored in a separate file, not in SQLite.
-- See Section 16 for the audit log file format and location.
```

**Migrations** are embedded in the binary using `include_str!` and applied at startup via a simple version table:

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

---

## 12. API Reference

Base URL: `http://127.0.0.1:18301`

All requests require `Authorization: Bearer <token>`.

### 12.1 Filesystem Endpoints

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
| `depth` | no | `3` | Max recursion depth (server-side maximum: `10`; requests above this are clamped) |

**Response `200`:**

```json
{
  "path": "VaultKB:/",
  "depth": 3,
  "truncated": false,
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

**Server-side limits:** `depth` is clamped to a maximum of `10`. Total entries returned across all levels is capped at `10 000`; if the cap is reached, `"truncated": true` is set in the response.

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

Copy file between mounts. Both `src` and `dst` must be memo mount paths. Token must have read scope on the source mount and write scope on the destination mount. Policy (deny globs, size limits) is checked on both mounts independently.

**Query params:** `src`, `dst`

**Response `200`:**

```json
{"src": "VaultKB:/notes/a.md", "dst": "VaultKB:/archive/a.md"}
```

**Local → mount copies:** The daemon `cp` endpoint only operates on mount paths. To copy a local file into a mount, the `memo` CLI reads the local file and calls `PUT /v1/fs/write`. The CLI's `memo cp ./local.png VaultKB:/assets/image.png` handles this transparently — no special daemon endpoint is required.

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

### 12.2 Meta Endpoints

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

#### `PATCH /v1/meta/mounts/:name`

Update mount policy fields. Only provided fields are changed; omitted fields retain their current values.

**Body (all fields optional):**

```json
{
  "mode": "ro",
  "description": "Now read-only",
  "hide_globs": [".obsidian/**", "*.private.md"],
  "deny_read_globs": [],
  "deny_write_globs": ["*.png"],
  "max_read_bytes": null,
  "max_write_bytes": 5242880
}
```

**Response `200`:** Full updated mount object.

**Note:** `name` and `root_path` are immutable after registration. Remove and re-add the mount to change them.

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
| `before` | no | — | ISO 8601 timestamp upper bound (returns entries older than this) |
| `after` | no | — | ISO 8601 timestamp lower bound (returns entries newer than this, ascending order) |
| `after_id` | no | — | Sequential log entry ID lower bound; use for forward pagination when polling for new entries |

When `after` or `after_id` is provided, entries are returned in **ascending** (oldest-first) order to support polling clients. When only `before` is provided (or no time filter), entries are returned in **descending** (newest-first) order.

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

### 12.3 Utility Endpoints

#### `GET /health`

No auth required. Used by `memo daemon status` to verify the daemon is reachable.

**Response `200`:**

```json
{"status": "ok", "version": "0.1.0"}
```

---

## 13. Token & Auth Model

### Token Format

Raw tokens use the format `memo_<random_base62_32chars>`. This prefix aids identification and avoids confusion with other credential types. Example: `memo_aB3xK9mNpQ2rS7tU4vW1yZ6`.

### Token Storage (Daemon Side)

Tokens are hashed with Argon2id before storage. Parameters: `m=19456` (19 MiB), `t=2`, `p=1` (OWASP recommended minimum).

**Verification** is CPU-intensive (~50–100ms at these parameters). Verification uses the `password-hash` crate traits and is dispatched via `tokio::task::spawn_blocking` to avoid blocking the async runtime thread pool:

```rust
use argon2::{Argon2, PasswordVerifier};
use password_hash::PasswordHash;

let parsed = PasswordHash::new(&hash).map_err(|_| AuthError::TokenInvalid)?;
Argon2::default().verify_password(raw_token.as_bytes(), &parsed)?;
```

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

- **Human (CLI):** `~/.config/memo/tokens/<name>.token` (mode `0600`). macOS Keychain integration deferred to v2.
- **Agent:** `MEMO_TOKEN` environment variable. Fallback to file.

### Bootstrap

The first time `memod` starts with no tokens in the database, it generates an `admin` token with all scopes and:

1. Writes the raw token to `~/.config/memo/bootstrap.token` (mode `0600`).
2. Prints the file path to stderr: `Bootstrap token written to: ~/.config/memo/bootstrap.token`.
3. **Continues running normally** — does not halt. The daemon is immediately ready to serve requests using the bootstrap token.

The operator reads the file, stores the token securely, and uses it to provision further tokens. After provisioning, `~/.config/memo/bootstrap.token` should be deleted.

---

## 14. Error Model

**Error type implementation** uses `thiserror`. `memo-core` defines:

- `ApiError` — top-level HTTP error returned to clients; implements `axum::response::IntoResponse`
- `PolicyError` — path validation errors, mapped to `ApiError` in `server.rs`
- `DbError` — database errors, wrapped into `ApiError::Internal`

Each layer converts its domain error to the next via `From` implementations.

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

## 15. CLI Reference

Global flags apply to all commands:

```
--json              Structured JSON output
--host <addr>       Daemon host (overrides env/config, default: 127.0.0.1)
--port <port>       Daemon port (overrides env/config, default: 18301)
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
memo mount update VaultKB --mode ro
memo mount update VaultKB --hide-glob ".obsidian/**" --max-write-bytes 5242880

# Token management
memo token list
memo token create --name claude-agent --scopes "fs:VaultKB:read,fs:VaultKB:write"
memo token create --name human-admin --scopes "admin:*" --expires 2024-12-31T00:00:00Z
memo token revoke <token-id>

# Audit log
memo audit
memo audit --mount VaultKB
memo audit --limit 50
memo audit --after 2024-01-15T00:00:00Z

# Daemon
memo daemon start
memo daemon stop
memo daemon status
memo daemon logs --tail 50
```

---

## 16. Mount Configuration Schema

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
bind_addr = "127.0.0.1:18301"  # TCP address to listen on; change port if 18301 conflicts
db_path = ""                    # empty = use XDG_DATA_HOME default (~/.local/share/memo/memo.db)
log_path = ""                   # empty = use XDG_STATE_HOME default (~/.local/state/memo/memod.log)
log_level = "info"              # trace | debug | info | warn | error

[daemon.write]
fsync = false             # enable fsync before rename (safer, slower)
dir_sync = false          # sync parent directory after rename

[daemon.limits]
max_audit_log_rows = 100000   # entries before rotation (background task on startup rotates audit.log → audit.log.1)
```

---

## 17. Observability & Audit

### Audit Log

Every operation (successful or failed) appends one JSON line to `$XDG_STATE_HOME/memo/audit.log` (default: `~/.local/state/memo/audit.log`). This includes auth failures — auth failures write `token_id: null`.

Audit entries are **not** stored in SQLite. The file is append-only JSON lines (one JSON object per line), with monotonically increasing `id` (sequential counter, not autoincrement from DB) for forward pagination support.

Audit log entry format:

```json
{"id": 1, "timestamp": "2024-01-15T10:30:00.123Z", "token_id": "550e8400-...", "operation": "read", "mount": "VaultKB", "path": "/notes/git.md", "result": "ok", "error_code": null}
```

The audit log is queryable via `GET /v1/meta/audit` (reads and filters from the log file) and `memo audit` (v1 CLI command — see Section 14). Pruning: if `audit.log` exceeds `max_audit_log_rows` entries, a background task at startup rotates it to `audit.log.1` and starts a new file.

### Structured Logging (memod)

`memod` uses `tracing` + `tracing-subscriber` with configurable log level. Log output is written to **`$XDG_STATE_HOME/memo/memod.log`** (default: `~/.local/state/memo/memod.log`) in JSON-lines format, and also mirrored to stderr. The `memo daemon logs --tail N` CLI command reads the log file directly (no DB involvement).

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

`GET /health` — no auth required. See Section 11.3 for the full endpoint spec. Used by `memo daemon status` to verify the daemon is reachable.

---

## 18. Repository Layout

```
memo/
├── Cargo.toml              # workspace [workspace] members = ["crates/*"]
├── Cargo.lock
├── crates/
│   ├── memod/              # daemon binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs                 # startup, config, signal handlers
│   │       ├── server.rs               # axum router, middleware; handlers are thin adapters
│   │       ├── access_control/         # BC: token lifecycle and auth
│   │       │   ├── mod.rs              # TokenService (application service)
│   │       │   ├── repository.rs       # SqliteTokenRepository impl
│   │       │   └── middleware.rs       # Bearer token extraction (axum middleware)
│   │       ├── mount_registry/         # BC: mount config and policy
│   │       │   ├── mod.rs              # MountService (application service)
│   │       │   ├── repository.rs       # SqliteMountRepository impl + PolicyCache
│   │       │   └── policy.rs           # PolicyEngine: path validation steps 5–10
│   │       ├── filesystem/             # BC: file I/O
│   │       │   ├── mod.rs              # FileSystemService (application service)
│   │       │   ├── ops.rs              # ls, stat, read, write, mkdir, mv, rm, cp
│   │       │   ├── atomic.rs           # atomic write-by-rename (streaming AsyncRead)
│   │       │   ├── grep.rs             # text search (regex crate)
│   │       │   └── find.rs             # glob search (walkdir + globset)
│   │       ├── audit/                  # BC: operation recording
│   │       │   ├── mod.rs              # AuditService: event consumer + query handler
│   │       │   └── log.rs              # append-only JSON lines; AtomicU64 sequential id
│   │       └── db/
│   │           └── mod.rs              # sqlx pool (SQLite WAL), migrations
│   ├── memo/               # CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs     # clap app + global flags
│   │       └── commands/   # all commands use memo-client for HTTP transport
│   │           ├── ls.rs
│   │           ├── tree.rs
│   │           ├── cat.rs
│   │           ├── write.rs
│   │           ├── mkdir.rs
│   │           ├── mv.rs
│   │           ├── rm.rs
│   │           ├── cp.rs   # local→mount: reads local file, calls write endpoint; also mount→mount
│   │           ├── grep.rs
│   │           ├── find.rs
│   │           ├── info.rs
│   │           ├── mount.rs
│   │           ├── token.rs
│   │           ├── audit.rs
│   │           └── daemon.rs
│   ├── memo-client/        # shared typed REST client (reqwest-based)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs      # MemoClient struct; base URL config; bearer token injection
│   │       ├── fs.rs       # typed wrappers for /v1/fs/* endpoints
│   │       └── meta.rs     # typed wrappers for /v1/meta/* endpoints
│   ├── memo-ui/            # Tauri v2 native macOS desktop admin application
│   │   ├── src-tauri/
│   │   │   ├── Cargo.toml  # crate-type = ["staticlib", "cdylib", "rlib"]
│   │   │   ├── capabilities/
│   │   │   │   └── default.json         # Tauri v2 capability declarations
│   │   │   ├── entitlements.macos.plist # macOS App Sandbox network entitlement
│   │   │   └── src/
│   │   │       ├── main.rs
│   │   │       ├── lib.rs               # shared lib entry (required for Tauri v2)
│   │   │       └── commands.rs          # Tauri invoke commands using memo-client
│   │   ├── src/                         # React frontend (TypeScript)
│   │   │   ├── main.tsx
│   │   │   ├── App.tsx
│   │   │   ├── components/
│   │   │   │   ├── MountList.tsx
│   │   │   │   ├── TokenList.tsx
│   │   │   │   └── AuditLog.tsx
│   │   │   └── hooks/
│   │   │       └── useMemoClient.ts
│   │   ├── index.html
│   │   ├── package.json
│   │   ├── tsconfig.json
│   │   └── vite.config.ts
│   └── memo-core/          # shared domain model; no I/O
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── mount.rs        # Mount aggregate, MountPolicy, MountName value object
│           ├── token.rs        # Token aggregate, TokenId, ScopeSet
│           ├── path.rs         # MountPath, RelativePath value objects (validation at construction)
│           ├── scope.rs        # Scope enum, scope parsing + checking
│           ├── events.rs       # DomainEvent enum
│           ├── errors.rs       # ApiError, PolicyError, DbError (thiserror)
│           └── repositories.rs # MountRepository, TokenRepository traits (no impl)
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
  "crates/memo-client",
  "crates/memo-ui/src-tauri",
  "crates/memo-core",
]

[workspace.dependencies]
tokio              = { version = "1", features = ["full"] }
axum               = { version = "0.7", features = ["macros"] }
# hyper removed — axum 0.7 already depends on hyper 1.x; no direct use
reqwest            = { version = "0.12", features = ["json", "stream"] }  # memo-client HTTP transport
serde              = { version = "1", features = ["derive"] }
serde_json         = "1"
# macros feature dropped for v1 — avoids DATABASE_URL compile-time requirement.
# Use runtime query() API instead of query!(). Add macros + sqlx prepare workflow if
# compile-time SQL checking is desired in the future.
sqlx               = { version = "0.8", features = ["sqlite", "runtime-tokio-rustls"] }
argon2             = "0.5"
password-hash      = "0.5"                           # required for argon2 v0.5 PasswordHash API
rand               = "0.8"                           # token generation (base62 random bytes)
globset            = "0.4"
dashmap            = "6"                             # concurrent hashmap for glob cache
path-clean         = "1"                             # path normalization for symlink check
tracing            = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
uuid               = { version = "1", features = ["v4"] }
clap               = { version = "4", features = ["derive"] }
toml               = "0.8"
thiserror          = "2"                             # ergonomic error types
regex              = "1"                             # grep implementation
walkdir            = "2"                             # recursive directory traversal
mime_guess         = "2"                             # Content-Type detection
chrono             = { version = "0.4", features = ["serde"] }  # ISO 8601 timestamps
tauri              = { version = "2", features = [] }
tauri-plugin-store = "2"                             # token storage for memo-ui
```

---

## 19. Testing Strategy

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
3. Starts `memod` bound to a random loopback port (e.g. `127.0.0.1:0`, OS assigns port)
4. Runs operations via `reqwest` pointed at the test port, or via the `memo` CLI binary with `--port`
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

## 20. Open Questions

1. ~~**`memo-ui` transport**~~ — **Resolved:** `memo-ui` connects to `memod` via REST HTTP on `127.0.0.1:18301` using the shared `memo-client` crate.

2. **Token expiry defaults:** Should `POST /v1/meta/tokens` accept a `default_ttl_days` config, or should tokens always be non-expiring unless `expires_at` is explicitly set? The current design defaults to non-expiring; a sensible alternative is a 90-day default for agent tokens.

3. **Daemon auto-start:** Should the `memo` CLI auto-start `memod` if the daemon is unreachable (connection refused on port 18301)? This is ergonomic but adds complexity (process management, reading bootstrap token from file). Alternative: require manual `memo daemon start` or a launchd/systemd service.

4. **`SIGHUP` config reload:** Should `memod` support `SIGHUP` to reload `config.toml` without restart? Mount and token changes already take effect immediately (SQLite reads per request). The only config that would benefit from reload is log level and write options. Low priority for v1.

5. ~~**macOS Keychain integration**~~ — **Resolved for v1:** CLI stores tokens in `~/.config/memo/tokens/<name>.token` (mode `0600`). Keychain integration deferred to v2.

6. ~~**REST API / TCP exposure**~~ — **Resolved:** `memod` exposes REST HTTP/1.1 on `127.0.0.1:18301` (loopback only). `memo-client` uses `reqwest`.

7. **`cp` at the daemon level for external sources:** The daemon `cp` endpoint currently handles mount-to-mount only. Local→mount copies are handled by the CLI transparently (read local + write). Should there be a dedicated `/v1/fs/upload` endpoint for external-path ingestion at the daemon level, or is the CLI-side approach sufficient for v1? **Needs decision.**
