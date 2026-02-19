---
description: Shared persistence infrastructure design — SQLite pool, migration contract, and schema encoding rules
tags:
  - design
  - persistence
---

# Persistence Infrastructure Design

## Responsibility

Persistence infrastructure provides shared database primitives for daemon contexts:

- initialize and own the SQLite connection pool
- enforce runtime SQLite settings (WAL and foreign keys)
- apply embedded schema migrations deterministically
- define stable DB encoding rules used by repositories

This layer does not implement domain repositories itself; it is the foundation used by Access Control and Mount Registry repositories.

## Runtime DB Guarantees

On pool initialization:

- `journal_mode` is set to `WAL`
- `foreign_keys` is enabled (`ON`)
- migration application runs before the pool is exposed to callers

Configuration invariants:

- `database_url` must be a SQLite URL (`sqlite:...`)
- `min_connections <= max_connections`
- invalid configuration fails fast as a connection error

## Migration Contract

Migrations are embedded in the binary and applied at startup.

- migration SQL files are loaded with `include_str!`
- applied versions are tracked in `schema_migrations`
- each version is applied once
- re-running migration application is idempotent

This guarantees safe repeated daemon startup without schema drift side effects.

## Schema Surface (Phase 1 / A1)

The initial schema contains:

- `mounts`
- `tokens`
- `schema_migrations`

`audit` remains file-backed and is intentionally not a SQLite table in v1.

## DB Encoding Rules

To avoid drift between domain enums and stored values, repositories must use explicit mappings:

- `MountMode::ReadOnly` <-> `ro`
- `MountMode::ReadWrite` <-> `rw`
- `Audience::Shared` <-> `shared`
- `Audience::AgentOnly` <-> `agent-only`
- `Audience::HumanOnly` <-> `human-only`

Repositories must treat unknown persisted values as database/data integrity errors, not as user input errors.
