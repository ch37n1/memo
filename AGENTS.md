# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) or other agent when working with code in this repository.

## Project Overview

`memo` is a secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work. It gives humans and LLM agents a shared, policy-controlled space to read, write, and build a common knowledge base.

**Status:** In implementation. Phases 0-2 are complete. Completed streams include A1 (`memod` database layer), A2 (`Access Control BC`), B1 (`memo-client`), and B2 (`memo` CLI scaffolding + daemon commands).

**Primary platform:** macOS. Linux is a supported secondary target. Windows is out of scope.

---

## Build, Test, and Lint Commands

The project uses a Cargo workspace at the repo root:

```bash
# Build all crates
cargo build

# Build specific crate
cargo build -p memod

# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p memo-core

# Run a single test by name
cargo test -p memo-core test_name

# Run integration tests (in tests/integration/)
cargo test --test fs_ops

# Run security tests (tagged #[ignore])
cargo test -- --include-ignored

# Format check
cargo fmt --check

# Lint
cargo clippy

# Format
cargo fmt

# Start daemon in dev mode
./scripts/dev.sh

# Create XDG dirs and initial setup
./scripts/setup.sh
```

**Quality gate:** All of `cargo fmt --check`, `cargo clippy`, and `cargo test` must pass before committing (pre-commit hooks enforce this locally).

---

## Architecture

The system follows a client–server model. All filesystem I/O is serialized through the daemon — clients never touch the filesystem directly. All IPC uses REST HTTP/1.1 on loopback TCP (`127.0.0.1:18301`).

### Crates (Cargo Workspace)

| Crate | Type | Role |
|-------|------|------|
| `crates/memo-core` | lib | Shared types (`Mount`, `Token`, `AuditEntry`), error types (`ApiError`, `PolicyError`, `DbError` via `thiserror`), scope parsing/checking. No I/O. All other crates depend on this. |
| `crates/memod` | binary | Daemon. Owns all filesystem I/O. `axum`/`tokio` HTTP server. Auth, policy, fs ops, SQLite, audit log. |
| `crates/memo` | binary | CLI client. Thin `clap` app over `memo-client`. No direct filesystem or DB access. |
| `crates/memo-client` | lib | Typed `reqwest`-based REST client. Used by both `memo` CLI and `memo-ui` Rust backend. |
| `crates/memo-ui/src-tauri` | lib+cdylib | Tauri v2 native macOS desktop app. React/Vite frontend, Rust backend calls daemon via `memo-client`. |

### memod Internal Layers

Request path: **auth → scope check → path validation (policy) → fs operation → audit log**

```
memod/src/
├── main.rs         # startup sequence, config, signal handlers
├── server.rs       # axum router + middleware
├── auth/           # token extraction, scope verification
├── policy/
│   ├── mod.rs      # PolicyEngine; DashMap<String, Arc<CompiledMount>> glob cache
│   └── path.rs     # resolve_read_path / resolve_write_path (10-step validation)
├── db/
│   ├── mounts.rs   # mount CRUD; invalidates glob cache on every write
│   └── tokens.rs   # Argon2id verify via spawn_blocking
├── fs/
│   ├── ops.rs      # ls, stat, read, write, mkdir, mv, rm, cp
│   ├── atomic.rs   # atomic write-by-rename (streaming AsyncRead, temp-file cleanup)
│   ├── grep.rs     # regex text search (regex + walkdir)
│   └── find.rs     # glob filename search (globset + walkdir)
└── audit.rs        # append JSON lines; AtomicU64 sequential id
```

### Key Design Decisions

- **Paths:** All client paths use `MountName:/relative/path` format. Absolute paths, `..`, and symlinks are rejected before any syscall. Path validation is the most security-critical component (`policy/path.rs`).
- **Policy cache:** `DashMap<String, Arc<CompiledMount>>` compiles globs once at mount load; invalidated on every mount mutation.
- **Atomic writes:** Write to temp file → rename. Uses `AsyncRead` for streaming; cleans up temp file on any error.
- **Audit log:** Append-only JSON lines at `~/.local/state/memo/audit.log`. Not in SQLite. Sequential `id` from in-memory `AtomicU64`.
- **Tokens:** `memo_<base62_32chars>` format. Hashed with Argon2id (`m=19456, t=2, p=1`). Verification dispatched via `spawn_blocking`.
- **memo-ui transport:** All `memod` calls are made from the Tauri Rust backend via `memo-client`. No JS-side `fetch` to `127.0.0.1:18301`.

### File Paths (Runtime)

| Resource | Path |
|----------|------|
| Config | `~/.config/memo/config.toml` |
| Database | `~/.local/share/memo/memo.db` |
| Daemon log | `~/.local/state/memo/memod.log` |
| Audit log | `~/.local/state/memo/audit.log` |
| Bootstrap token | `~/.config/memo/bootstrap.token` |
| PID file | `$XDG_RUNTIME_DIR/memo/memod.pid` |
| launchd plist | `~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist` |

---

## Code Agreements

### Style

- Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/)
- Prefer functional style: combinators, immutability, expression-oriented code
- Prefer early return (`?`, `return`) over deep nesting
- Comments explain *why*, not *what* — comment non-obvious logic
- `unsafe`: avoid; always document the safety invariant when used
- Keep files under ~1k lines; avoid overly long functions

### Error Handling

- `Result` over `panic!`
- `thiserror` for library-level errors (`memo-core`, `memod` domain layers)
- `anyhow` for application layer (CLI entry points)
- Each layer converts its errors via `From` implementations

### Observability

- Use `tracing` for structured logging
- Log levels: `error` for failures, `info` for key events, `debug`/`trace` for internals

### Dependencies

- Prefer well-maintained, widely-used crates
- Workspace-level dependency pinning in root `Cargo.toml`

### Testing

- **TDD:** write tests for main use cases before implementation, then expand to full coverage
- Unit tests co-located in `#[cfg(test)]` modules
- Integration tests in `tests/integration/` — each test spins up `memod` on a random loopback port with a temp SQLite DB and temp mount root
- Security tests tagged `#[ignore = "security"]`, enabled in CI with `--include-ignored`

### Git Workflow

- **Branches:** `main` + feature branches named after the feature (e.g. `add-mount-command`)
- **Commits:** conventional prefixes — `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`
- Run linters, formatters, and tests on pre-commit locally before pushing

---

## Task Workflow

Every task follows the same core loop: **understand → implement → verify → document**. The depth of each phase scales with task size.

| Size | Context | Planning | Implementation | Key difference |
|------|---------|----------|----------------|----------------|
| Small (known context) | Provided, sufficient | None | Direct | Fastest path, no exploration needed |
| Small (unknown context) | Needs exploration | None | Direct | Self-guided context gathering before work |
| Mid | Always explore more | Plan with steps | Step by step | Structured plan, review gate before finish |
| Large | Always explore more | Plan with phases | Phase by phase | Decomposes into multiple mid-task processes |

### Common Steps

**Context:**

| Step | Action |
|------|--------|
| **Read provided context** | Read all referenced materials: docs, tickets, code, conversations. |
| **Explore additional context** | Investigate codebase, related modules, dependencies. Form your own understanding. |
| **Clarify** | If context is still insufficient — ask questions. Do not guess on ambiguous requirements. |
| **External blockers** | If blocked by external circumstances (network access, missing local tools, credentials, approvals), explicitly ask the user to perform the required action and then continue once confirmed. |

**Verify:**

| Step | Action |
|------|--------|
| **Run automations** | Linters, formatters, tests. All must pass. |
| **Fix issues** | Resolve any failures from automations. Re-run until clean. |

**Document:**

| Step | Action |
|------|--------|
| **Check if docs need update** | Do changes affect documented behavior, architecture, or contracts? |
| **Update docs** | If yes — update relevant documentation. |
| **Update QA cases** | Add or update manual test cases for the most important scenarios. Keep it minimal — only critical paths and edge cases. |

### Small Task — Known Context

The context provided is sufficient to start immediately.

1. Read provided context
2. If not enough — clarify, otherwise proceed
3. Implement
4. Verify
5. Document
6. Done

### Small Task — Unknown Context

Context needs self-guided exploration before implementation.

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Implement
5. Verify
6. Document
7. Done

### Mid Task

Always requires deeper context exploration. Implementation follows a structured plan.

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Create a plan with implementation steps
5. Implement step by step (single phase)
6. Verify
7. Review changes — if fixes needed, return to step 5
8. Document
9. Done

### Large Task

Decomposes into phases. Each phase is essentially a mid-task process.

1. Read provided context
2. Explore additional context
3. If not enough — clarify, otherwise proceed
4. Create a plan with phases, each containing steps
5. Execute each phase as a mid-task process (plan steps → implement → verify → review → document)
6. Done

---

## Documentation

All docs live in `docs/`. The entry point is `docs/README.md`. Conventions are in `docs/operations/documentation.md` ([!IMPORTANT] use this files).
Roadmap source of truth: `docs/roadmap-v1.md`.
When planning or checking phase status, agents must use `docs/roadmap-v1.md` by default without requiring a user reminder.

Key directories (planned, not all exist yet):

| Directory | Purpose |
|-----------|---------|
| `docs/README.md` | Project overview, architecture summary, agreements |
| `docs/architecture/` | System-level truth: components, data flow, deployment |
| `docs/design/` | Per bounded context: responsibilities and flows |
| `docs/decision-records/` | Numbered records of significant choices (immutable once written) |
| `docs/operations/` | Configuration, monitoring, documentation agreements |
| `docs/archive/` | Superseded documents (read-only, historical reference) |

- All docs use YAML frontmatter with a required `description` field
- Decision records use `dr-NNNN-slug.md` naming and are never rewritten
- Use Mermaid for diagrams
- File/directory names use `kebab-case`
