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
