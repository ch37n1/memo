---
description: Project documentation entry point — agreements, structure, and navigation
---

# memo — Documentation

## Project Overview

`memo` is a secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work.

It gives humans and LLM agents a shared, policy-controlled space where both can read, write, and build a common knowledge base — without exposing unrelated personal files. Memory in v1 is **file-based**: regular files and directories accessed through named mounts. Files are the right starting primitive because they are native to human tools (Obsidian, editors) and equally accessible to agents.

### Architecture

| Component | Role |
|-----------|------|
| `memod` | Daemon — owns all filesystem I/O; the only process that touches the real filesystem |
| `memo` | CLI client — for humans and agents |
| `memo-ui` | Tauri v2 native macOS desktop app — mount/token management and audit log viewer |
| `memo-client` | Shared Rust library — typed HTTP client used by CLI and UI backend |

All IPC is REST HTTP/1.1 on loopback TCP (`127.0.0.1:18301`). No external network exposure.

**Primary platform:** macOS. Linux is a supported secondary target. Windows is out of scope.
**Implementation language:** Rust across the entire stack.

### Development Stage

MVP — single-user personal tool. One human operator, one or more LLM agents, all on the same machine. No multi-user, no networked access.

### Implementation Status

- Phase 0 (foundation) is complete.
- Phase 1 (core infrastructure) is complete.
- Phase 2 (authentication) is complete.
- Phase 3 (mount system) is complete.
- Phase 4 (file operations) is complete.
- Stream A, A1 (`memod` database layer) is complete.
- Stream A, A2 (`Access Control BC`) is complete.
- Stream A, A3 (`Mount Registry BC`) is complete.
- Stream A, A4 (`File System BC`) is complete.
- Stream B Phase 1 (`B1: memo-client`) is complete.
- Stream B Phase 2 (`B2: CLI scaffolding + daemon commands`) is complete.
- Stream B Phase 3 (`B3: CLI admin commands`) is complete.
- Stream B Phase 4 (`B4: CLI filesystem commands`) is complete.
- Next active milestones are Stream A A5 (`Audit BC + Daemon Lifecycle`) and Stream B B5 (`Integration & Security Tests`).

### Goals (v1)

- Safe, policy-enforced access to named filesystem mounts
- No out-of-bounds path access (traversal, symlinks, absolute paths)
- Collaborative Markdown knowledge base accessible to both humans (Obsidian) and agents
- Atomic writes compatible with file watchers (Obsidian, etc.)
- Token-based auth with per-mount, per-operation scopes
- Structured, deterministic output suitable for LLM agent consumption
- Audit log of all operations

### Non-Goals (v1)

- Embeddings or semantic search
- Graph/object memory layer
- Exposure beyond loopback (TLS, external TCP)
- Multi-user or multi-machine scenarios
- Shell execution or virtual filesystem views
- Windows support

> [!IMPORTANT] Check [Documentation Agreements](operations/documentation.md) on how to work with docs.

---

## Project Agreements

_In future, some agreements can be moved to `operations` if grow to much._

### Design

- **Domain-Driven Design (DDD)** — use DDD for system design: bounded contexts, ubiquitous language, aggregates, domain events.

### Code Style

- Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/).
- **Functional style** — idiomatic in Rust; prefer combinators, immutability, and expression-oriented code.
- **Early return** — prefer `return` / `?` early to avoid excessive nesting.
- **Comprehensive comments** — explain *why*, not *what*. Non-trivial logic must be commented.
- Use common patterns (Builder, Newtype, From/Into, etc.) where they fit naturally.
- **Error handling** — `Result` over `panic!`. Use `thiserror` for library-level errors, `anyhow` for application layer. Provide meaningful error messages.
- **`unsafe`** — avoid unless absolutely necessary; always document the safety invariant.
- **Observability** — use `tracing` for structured logging. Use appropriate log levels (`error` for failures, `info` for key events, `debug`/`trace` for internals).

### Dependencies

- Prefer well-maintained, widely-used crates.

### Testing

- **TDD** — start with main use cases before implementation; expand to full test coverage after.

### Architecture

- In general follow common best practices for code style, architecture, and UI design.
- Keep solutions simple — avoid premature abstraction.
- **Keep units small** — avoid files over ~1k lines, overly long functions, and structs with too many methods. Exceptions are acceptable where complexity is inherent (e.g. core domain logic).

### Git Workflow

- **Branching** — `main` + feature branches. Branch name = feature name (e.g. `add-mount-command`).
- **Commits** — use conventional prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- **Quality gate** — run linters, formatters, and tests on pre-commit locally. Fast feedback over CI pipelines.

---

## Docs Structure (Current)

| Path | Purpose | Truth type |
|------|---------|------------|
| `docs/README.md` | Entry point: project overview, docs map | — |
| `docs/architecture.md` | Architecture summary and boundaries | System truth |
| `docs/system-design-v1.md` | Detailed v1 design document | Domain truth |
| `docs/design/README.md` | Domain design index and deep-dive navigation | Domain truth |
| `docs/design/access-control.md` | Access control logic, invariants, and failure semantics | Domain truth |
| `docs/design/persistence.md` | SQLite persistence layer design and migration strategy | Domain truth |
| `docs/design/mount-registry.md` | Mount and policy logic, path-validation boundaries | Domain truth |
| `docs/design/file-system.md` | File operation semantics, atomicity, and constraints | Domain truth |
| `docs/design/audit.md` | Audit event model and append-only log behavior | Domain truth |
| `docs/roadmap-v1.md` | Near-term planning and sequencing | Planning truth |
| `docs/dev-env-checklist.md` | Local engineering environment checklist | Procedural truth |
| `docs/references/README.md` | Quick-reference index (formats, codes, contracts) | Reference truth |
| `docs/references/domain-event-json.md` | `DomainEvent` JSON wire format reference | Reference truth |
| `docs/references/api-error-codes.md` | API error code reference and meaning | Reference truth |
| `docs/operations/task-workflow.md` | Standard task execution workflow | Procedural truth |
| `docs/operations/parallel-dev.md` | Collaboration guidance for parallel work | Operational truth |
| `docs/operations/documentation.md` | Documentation conventions and taxonomy | Operational truth |
| `docs/operations/manual-regression-v1.md` | Full manual regression test runbook for v1 release validation | Operational truth |
| `docs/archive/vision.md` | Historical vision snapshot | Historical truth |

Planned directories (`decision-records/`, `development/`) will be introduced as content grows.

> Full conventions: [Documentation Agreements](operations/documentation.md)

---

## Tooling

Core project checks:
- `make fmt-check`
- `make lint`
- `make test`
- `make coverage`
- `make check` (runs all gates above)

Coverage tooling requirements:
- Install `cargo-llvm-cov`:
  - `cargo install cargo-llvm-cov`
- Ensure LLVM tools are available (`llvm-cov`, `llvm-profdata`):
  - `brew install llvm`
  - `echo 'export PATH="/opt/homebrew/opt/llvm/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc`
