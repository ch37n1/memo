---
description: memo — secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work.
---

# memo

A secure, mount-scoped, daemon-backed filesystem layer for collaborative human–agent knowledge work. It gives humans and LLM agents a shared, policy-controlled space to read, write, and build a common knowledge base.

**Status:** In implementation. Phases 0-4 are complete; Stream A1-A4 (`memod` database layer, `Access Control BC`, `Mount Registry BC`, `File System BC`) and Stream B1-B4 (`memo-client`, `memo` CLI scaffolding + daemon commands, CLI admin commands, CLI filesystem commands) are complete.

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

## Quick Start (0 -> Mounted Folder for CLI)

This flow starts `memod`, creates one mount, and verifies CLI read/write through the daemon.

### 1. Start daemon (terminal A)

```bash
cd memo
./scripts/dev.sh
```

On first start, `memod` writes a bootstrap admin token to:
`~/.config/memo/bootstrap.token`

### 2. Export token and create a mount (terminal B)

```bash
cd memo
export MEMO_TOKEN="$(cat ~/.config/memo/bootstrap.token)"

# Create a real folder to back the mount
mkdir -p ~/memo-vault

# Register mount in daemon
cargo run -q -p memo -- mount add \
  --name VaultKB \
  --path "$HOME/memo-vault" \
  --mode read_write \
  --audience shared
```

### 3. Use the mount via CLI

```bash
# Write a file
echo "hello from memo" | cargo run -q -p memo -- write VaultKB:/notes/hello.md

# Read it back
cargo run -q -p memo -- cat VaultKB:/notes/hello.md

# List folder
cargo run -q -p memo -- ls VaultKB:/notes
```

Optional quality check:

```bash
make check
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
