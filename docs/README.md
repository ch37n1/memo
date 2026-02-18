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

## Docs Structure

| Directory | Purpose | Truth type |
|-----------|---------|------------|
| `docs/README.md` | Entry point: project overview, docs map | — |
| `architecture/` | System context, high-level design, data flow, deployment | System truth |
| `design/` | Per bounded context: responsibilities, flows, decisions (closer to implementation) | Domain truth |
| `decision-records/` | Numbered records of significant choices with status tracking | Historical truth |
| `development/` | Local setup, coding guidelines, testing strategy | Procedural truth |
| `operations/` | Configuration, monitoring, documentation agreements | Operational truth |
| `references/` | API specs, data model, external links | Lookup material |
| `archive/` | Deprecated or superseded documents no longer actively maintained | Historical truth |

> Full conventions: [Documentation Agreements](operations/documentation.md)

---

## Tooling

_To be added._
