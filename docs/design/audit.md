---
description: Audit bounded context design — domain event capture, append-only storage, and query model
tags:
  - design
  - audit
---

# Audit Design

## Responsibility

Audit records every significant operation outcome for traceability:

- successful filesystem operations
- access denials
- administrative changes (mount/token lifecycle)

## Event Source

Audit consumes `DomainEvent` values emitted by other bounded contexts.
This keeps operation logic decoupled from recording logic.

## Current Implementation (Phase 5 / A5)

Implemented in `memod` as an append-only file-backed subsystem with query support.

Behavior includes:

- JSON-lines write path at `~/.local/state/memo/audit.log`
- monotonic `id` assignment via in-memory `AtomicU64`
- auth-failure records with `token_id: null`
- `GET /v1/meta/audit` filtering by mount, token id, operation, result, and time window
- forward pagination via `after_id`
- startup prune/rotation when row count exceeds `max_audit_log_rows` (rotate to `audit.log.1`)

## Storage Model

- append-only JSON lines file
- sequential ids for deterministic ordering
- file-backed source of truth (not SQLite table)

## Query Model

Audit queries filter by:

- mount
- token id
- operation/event type
- result
- time range
- pagination cursor (`after_id`)

## Failure Posture

- Operation success should not depend on non-critical audit formatting details.
- Audit write failures must be observable via daemon logs/stderr output.
- Query parsing/validation errors should fail request handling explicitly and not be silently ignored.
