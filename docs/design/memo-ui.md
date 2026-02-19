---
description: memo-ui design — Tauri backend command boundary, token setup flow, and frontend component responsibilities
tags:
  - design
  - memo-ui
  - tauri
---

# memo-ui Design

## Responsibility

`memo-ui` is the desktop admin surface for `memo` v1.
It provides a GUI for mount and token administration, audit inspection, and basic read-only tree browsing.

In v1, `memo-ui` is intentionally an admin/control-plane app, not a general file editor.

## Security Boundary

`memo-ui` preserves the same daemon boundary as the CLI:

- all filesystem and metadata operations are executed by `memod`
- frontend code does not call daemon HTTP endpoints directly
- frontend uses Tauri `invoke` -> Rust backend commands -> `memo-client` -> `memod`

This keeps auth, policy, and audit behavior centralized in the daemon.

## Backend Command Contract

The Rust backend exposes Tauri commands that wrap typed `memo-client` calls.
Current command surface:

- `health`
- `list_mounts`
- `create_mount`
- `remove_mount`
- `list_tokens`
- `create_token`
- `revoke_token`
- `query_audit`
- `browse_tree`

Command error behavior:

- command handlers return `Result<T, String>`
- daemon/client errors are mapped to user-visible message strings
- machine-readable semantics remain daemon-owned in HTTP error payloads

## Token Setup and Persistence

First-launch flow:

1. User pastes bootstrap/admin token.
2. Token is persisted through `tauri-plugin-store`.
3. UI loads mounts/tokens/audit from daemon.

Operational notes:

- default daemon URL is `http://127.0.0.1:18301`
- token storage is local app state, not source-controlled config
- missing/invalid token keeps app in setup/retry path

## Frontend Composition

Primary frontend parts:

- `MountList`
  - shows registered mounts
  - supports add/remove actions
- `TokenList`
  - lists token metadata
  - supports create/revoke actions
  - surfaces newly created raw token value once
- `AuditLog`
  - filterable list by mount/operation/result/limit
- `FileBrowser`
  - read-only tree view for quick mount inspection

Integration layer:

- `useMemoClient` contains typed wrappers around Tauri `invoke` command names and payloads
- `App` coordinates token setup, initial data load, and cross-widget refresh

## Platform and Packaging Constraints

`memo-ui` depends on Tauri capability and platform config:

- capability file includes `core:default`
- macOS network entitlement is required for daemon loopback access
- CSP remains restrictive (`default-src 'self'`) while backend performs daemon calls

## Non-Goals (v1)

Out of scope for `memo-ui` v1:

- rich file editing
- long-running background sync jobs
- alternate daemon transport modes
- multi-profile token/session management
