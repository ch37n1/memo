---
description: Full binary installation guide for memod, memo CLI, and memo-ui app, including initial mount setup and smoke tests
tags:
  - operations
  - install
  - release
---

# Full Install (Binary-Only Runtime)

## Goal

Install all three components so daily usage does not require `cargo run` or `npm run`:

- `memod` (daemon binary)
- `memo` (CLI binary)
- `memo-ui` (macOS `.app` bundle)

This guide targets macOS.

## 1. Prerequisites (build machine)

- Xcode Command Line Tools
- Rust toolchain
- Node.js 20+
- npm 10+

```bash
xcode-select --install
rustup toolchain install stable
```

## 2. Build and install `memod` + `memo` as binaries

From repo root:

```bash
# install both binaries into ~/.local/bin
cargo install --path crates/memod --root "$HOME/.local" --force
cargo install --path crates/memo  --root "$HOME/.local" --force
```

Add binary directory to PATH (zsh):

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
source "$HOME/.zshrc"
```

Verify:

```bash
which memod
which memo
memo --version
```

## 3. Build and install `memo-ui` as `.app`

`memo-ui` is packaged as a macOS app bundle via Tauri.

```bash
# one-time CLI for Tauri builds
cargo install tauri-cli --version "^2" --locked

# build frontend + app bundle
cd crates/memo-ui
npm install
cargo tauri build
```

App artifact is generated under:

- `crates/memo-ui/src-tauri/target/release/bundle/macos/memo-ui.app`

Install it to Applications:

```bash
cp -R crates/memo-ui/src-tauri/target/release/bundle/macos/memo-ui.app /Applications/
```

## 4. First-time daemon bootstrap

Start daemon as a managed service:

```bash
memo daemon start
memo daemon status
```

On first startup, bootstrap token is written to:

- `~/.config/memo/bootstrap.token`

Set token for CLI session:

```bash
export MEMO_TOKEN="$(cat ~/.config/memo/bootstrap.token)"
```

## 5. Create a mounted folder for CLI usage

```bash
# backing folder on host filesystem
mkdir -p "$HOME/memo-vault"

# register mount in daemon
memo mount add \
  --name VaultKB \
  --path "$HOME/memo-vault" \
  --mode read_write \
  --audience shared
```

## 6. Daily usage (no cargo/npm)

- CLI: use `memo ...` directly
- Daemon: `memo daemon start|stop|status|logs`
- UI: launch `/Applications/memo-ui.app`

## Smoke Tests

Run these after install:

```bash
# 1) daemon reachable
memo daemon status

# 2) mount exists
memo mount list

# 3) write/read roundtrip through daemon
echo "smoke $(date +%s)" | memo write VaultKB:/notes/smoke.md
memo cat VaultKB:/notes/smoke.md

# 4) filesystem listing
memo ls VaultKB:/notes

# 5) audit log endpoint
memo audit --limit 5
```

Expected:

- `memo daemon status` reports running daemon + version.
- `memo cat` returns written smoke content.
- `memo ls` contains `smoke.md`.
- `memo audit` returns recent operations.
