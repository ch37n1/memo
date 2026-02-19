# memo-ui

Desktop admin UI scaffold for `memo` (Phase B6).

## Scope

- Mount management (`list/create/remove`)
- Token management (`list/create/revoke`)
- Audit log viewer (filtered query)
- Basic read-only tree browser

All daemon calls are routed through Tauri Rust commands that use `memo-client`.

## Layout

- `src-tauri/` — Rust backend commands + Tauri config/capabilities/entitlements
- `src/` — React + TypeScript UI
- `vite.config.ts` — Vite dev/build settings

## Prerequisites

- Rust toolchain (workspace default)
- Node.js 20+
- npm 10+

## Frontend Dev

```bash
cd crates/memo-ui
npm install
npm run dev
```

## Frontend Build

```bash
cd crates/memo-ui
npm run build
```

## Rust Checks (workspace)

```bash
cargo check -p memo-ui
cargo clippy -p memo-ui --all-targets -- -D warnings
```

## Runtime Notes

- Default daemon URL in UI: `http://127.0.0.1:18301`
- Admin token is stored in the UI store via `@tauri-apps/plugin-store`
- macOS network entitlement file is present at:
  - `src-tauri/entitlements.macos.plist`
