---
description: memo — secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work.
---

# memo

A secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work. It gives humans and LLM agents a shared, policy-controlled space to read, write, and build a common knowledge base.

**Status:** Implementation in progress (Phase 1 underway). Workspace scaffolding and core domain types are in place; infrastructure is being implemented stream-by-stream.

**Primary platform:** macOS. Linux is a supported secondary target.

---

## Prerequisites

- Rust toolchain (`cargo` + `rustc`)
- [pre-commit](https://pre-commit.com) — for local git hooks (`pip install pre-commit` or `brew install pre-commit`)
- [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) — required for `make check` coverage gate (`cargo install cargo-llvm-cov`)
- LLVM coverage tools (`llvm-cov`, `llvm-profdata`)
  - Homebrew setup: `brew install llvm`
  - Add to PATH (zsh): `echo 'export PATH="/opt/homebrew/opt/llvm/bin:$PATH"' >> ~/.zshrc && source ~/.zshrc`

---

## Local Setup

```bash
# 1. Clone the repo
git clone <repo-url>
cd memo

# 2. One-time setup: creates XDG dirs and installs pre-commit hooks
make setup

# 3. Build all crates
make build
```

---

## Commands

| Command | Description |
|---------|-------------|
| `make build` | Build all workspace crates |
| `make test` | Run the full test suite |
| `make lint` | Run `cargo clippy` (warnings as errors) |
| `make fmt` | Format all code with `cargo fmt` |
| `make fmt-check` | Check formatting without modifying files |
| `make coverage` | Run line coverage check (`>=80%`) via `cargo llvm-cov` |
| `make check` | Run all quality gates: fmt-check, lint, test, coverage |
| `make dev` | Build and run `memod` in debug mode (`RUST_LOG=debug`) |
| `make setup` | One-time setup: XDG dirs + pre-commit hooks |

---

## Project Structure

```
memo/
├── crates/
│   ├── memo-core/      # Shared types, errors, scope parsing. No I/O.
│   ├── memod/          # Daemon binary. Owns all filesystem I/O.
│   ├── memo/           # CLI client binary.
│   └── memo-client/    # Typed HTTP client library for the daemon API.
├── docs/               # Architecture, design, and decision records.
├── scripts/
│   ├── setup.sh        # One-time dev environment setup.
│   └── dev.sh          # Run memod in debug mode.
├── Cargo.toml          # Workspace root with shared dependencies.
├── Makefile            # One-command workflows.
└── rust-toolchain.toml # Pinned Rust toolchain.
```

---

## Links

- [Architecture overview](docs/architecture.md)
- [System design (v1)](docs/system-design-v1.md)
- [Roadmap (v1)](docs/roadmap-v1.md)
- [Development environment checklist](docs/dev-env-checklist.md)
- [Full documentation](docs/README.md)
