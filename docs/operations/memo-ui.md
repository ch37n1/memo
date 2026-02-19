---
description: memo-ui operations guide — local development, build flow, and troubleshooting
tags:
  - operations
  - memo-ui
  - tauri
---

# memo-ui Operations

## Purpose

Operational guidance for running and troubleshooting `memo-ui` in local development.

## Prerequisites

- Rust toolchain (workspace version)
- Node.js 20+
- npm 10+
- running `memod` daemon (default `127.0.0.1:18301`)

## Local Dev Flow

1. Install dependencies:

```bash
cd crates/memo-ui
npm install
```

2. Start frontend dev server:

```bash
npm run dev
```

3. Run Rust checks from repo root:

```bash
cargo check -p memo-ui
cargo clippy -p memo-ui --all-targets -- -D warnings
```

## Build Flow

Frontend build:

```bash
cd crates/memo-ui
npm run build
```

Rust workspace build:

```bash
cd ../..
cargo build
```

## Runtime Setup Notes

- On first run, paste bootstrap/admin token from:
  - `~/.config/memo/bootstrap.token`
- Token is persisted via Tauri store plugin.
- Default daemon URL in UI: `http://127.0.0.1:18301`

## Troubleshooting

### `tauri` crate unresolved in all-features checks

Ensure `memo-ui` features map optional dependencies correctly (`tauri-app` -> `tauri`, `tauri-plugin-store`).

### Tauri macro errors during lint/test

For workspace lint/test passes, avoid requiring full app context generation in non-app paths.
Use conditional compilation boundaries so `clippy`/`test` targets do not require runtime assets.

### Entitlement/capability issues

Check:

- `src-tauri/capabilities/default.json`
- `src-tauri/entitlements.macos.plist`
- `src-tauri/tauri.conf.json`

### UI cannot connect to daemon

- verify daemon health with `memo daemon status` or `curl http://127.0.0.1:18301/health`
- verify token validity (`memo token list` using same token)
- verify mount exists (`memo mount list`)
