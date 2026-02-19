---
description: Access Control bounded context design — token lifecycle, scopes, and authorization decisions
tags:
  - design
  - access-control
---

# Access Control Design

## Responsibility

Access Control answers three questions for every request:

1. Is the caller authenticated?
2. Is the token still valid?
3. Does the token include the required scope?

## Core Model

- `Token` aggregate
- `TokenId` value object
- `Scope` and `ScopeSet` value objects
- `Expiry` for token validity window
- `SqliteTokenRepository` (in `memod`) as the persistence adapter for token lifecycle

## Current Implementation (Phase 2 / A2)

Implemented in `memod`:

- token hashing with Argon2id (`m=19456`, `t=2`, `p=1`)
- token verification on each authenticated request (hash verify dispatched via `spawn_blocking`)
- bearer auth middleware (`Authorization: Bearer <token>`)
- protected token endpoints:
  - `GET /v1/meta/tokens`
  - `POST /v1/meta/tokens`
  - `DELETE /v1/meta/tokens/:id`
- unauthenticated health endpoint:
  - `GET /health`

## Scope Semantics

- FS scopes: `fs:<mount|*>:<read|write>`
- Meta scopes: `meta:*:<action>`
- Admin scopes: `admin:*:<mounts|tokens|*>`

Matching rules:

- Exact scope match always passes.
- `fs:*:<action>` matches any mount for the same action.
- `admin:*:*` matches all admin actions.

Wire format note:

- Canonical serialized format is three-part for all scope families (`fs`, `meta`, `admin`)
- Parser accepts legacy two-part `meta/admin` forms for backward compatibility

## Failure Semantics

- Missing token: `auth_required`
- Invalid token: `token_invalid`
- Expired token: `token_expired`
- Missing scope: `permission_denied`

Access-denied outcomes are emitted as `DomainEvent::AccessDenied` for audit.

## Bootstrap Flow

On daemon startup, when `tokens` table is empty:

1. Generate bootstrap admin token (`memo_<base62_32chars>`)
2. Persist only the Argon2id hash in DB
3. Write raw token to `~/.config/memo/bootstrap.token` with mode `0600`
4. Print bootstrap token file path to stderr and continue serving

Bootstrap token is a one-time operational secret and should be rotated out after provisioning regular tokens.
