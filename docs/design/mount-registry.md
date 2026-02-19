---
description: Mount Registry bounded context design — mount invariants, policy model, and path safety boundaries
tags:
  - design
  - mount-registry
---

# Mount Registry Design

## Responsibility

Mount Registry owns mount metadata and policy constraints:

- mount identity (`MountName`)
- root path ownership and mode (`read_only`/`read_write`)
- policy controls (hide globs, deny-read globs, deny-write globs, size limits)

## Core Invariants

- Mount names are validated at construction and cannot contain unsafe characters.
- Root path must be absolute.
- Policy checks are deterministic for the same `(path, size)` input.

## Path Validation Boundary

Validation is intentionally split:

- Structural validation in value objects (`MountPath`, `RelativePath`)
- Canonical and filesystem-aware validation in daemon policy code

Structural checks reject malformed or obviously unsafe input before any filesystem operation.

## Policy Behavior

- Hidden path on read resolves as `NotFound` (do not leak existence).
- Denied read/write resolves as permission denial.
- Size limit violations return `TooLarge { limit, actual }`.
- Invalid policy configuration (for example invalid glob syntax) is an internal configuration issue and must not be surfaced as a user path format error.
