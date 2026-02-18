---
description: Project documentation entry point — agreements, structure, and navigation
---

# memo — Documentation

## Project Overview

`memo` is a mount-scoped, daemon-based collaborative filesystem layer for safe human–agent knowledge work. See [vision](vision.md) for full details.

!> [!IMPORTANT] Check [Documentation Agreements](operations/documentation.md) on how to work with doc.

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
| `overview/` | Problem space, goals, non-goals, glossary | Vision truth |
| `architecture/` | System context, high-level design, data flow, deployment | System truth |
| `design/` | Per bounded context: responsibilities, flows, decisions (closer to implementation) | Domain truth |
| `decision-records/` | Numbered records of significant choices with status tracking | Historical truth |
| `development/` | Local setup, coding guidelines, testing strategy | Procedural truth |
| `operations/` | Configuration, monitoring, documentation agreements | Operational truth |
| `references/` | API specs, data model, external links | Lookup material |

> Full conventions: [Documentation Agreements](operations/documentation.md)

---

## Tooling

_To be added._
