---
description: File System bounded context design — operation semantics, atomic writes, and safety constraints
tags:
  - design
  - file-system
---

# File System Design

## Responsibility

File System context performs all filesystem I/O through daemon-owned operations.
No client directly accesses the filesystem.

## Service Flow

Request path for filesystem operations:

1. authenticate token
2. verify required scope
3. resolve and validate path by mount policy
4. perform filesystem operation
5. emit domain event
6. append audit record

## Current Implementation (Phase 4 / A4)

Implemented in `memod` as `FileSystemService` with HTTP handlers under `/v1/fs/*`.

Covered operations:

- `ls`
- `tree`
- `stat`
- `read`
- `write`
- `mkdir`
- `mv`
- `rm`
- `cp`
- `grep`
- `find`

Implementation notes:

- reads are validated by mount policy before filesystem access
- writes use atomic write-by-rename and cleanup temp files on failure
- copy enforces source-read and destination-write checks independently
- recursive and result-limit behavior is handled by operation-specific parameters (`tree`, `rm`, `grep`, `find`)

## Operation Guarantees

- Read operations are constrained to validated mount roots.
- Write operations are guarded by mount mode and write policy.
- Directory traversal and absolute-path escapes are rejected before I/O.
- Symlink escape attempts are denied by policy layer before operation execution.

## Atomic Write Contract

Write behavior uses a write-by-rename approach:

1. write payload to temporary file in target directory
2. optionally `fsync` file based on `daemon.write.fsync`
3. rename temp file to target path atomically
4. optionally `fsync` directory based on `daemon.write.dir_sync`
5. cleanup temp file on error

This prevents partially written target files and is compatible with file watchers.

Hardening notes (Phase 6):

- Sync durability calls treat selected platform/filesystem errors as non-fatal for write completion:
  - `PermissionDenied`
  - `Unsupported`
  - `InvalidInput`
- This applies to both file sync and directory sync steps.
- Behavior is intentionally conservative: unknown sync errors still fail the write.

Runtime tuning:

- `daemon.write.fsync` and `daemon.write.dir_sync` remain the primary config toggles.
- Environment overrides are also supported:
  - `MEMOD_WRITE_FSYNC`
  - `MEMOD_WRITE_DIR_SYNC`
- Integration tests may disable these durability knobs to improve portability in constrained environments while still validating atomic rename semantics.
