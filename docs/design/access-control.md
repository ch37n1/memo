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

## Scope Semantics

- FS scopes: `fs:<mount|*>:<read|write>`
- Meta scopes: `meta:*:<action>`
- Admin scopes: `admin:*:<mounts|tokens|*>`

Matching rules:

- Exact scope match always passes.
- `fs:*:<action>` matches any mount for the same action.
- `admin:*:*` matches all admin actions.

## Failure Semantics

- Missing token: `auth_required`
- Invalid token: `token_invalid`
- Expired token: `token_expired`
- Missing scope: `permission_denied`

Access-denied outcomes are emitted as `DomainEvent::AccessDenied` for audit.
