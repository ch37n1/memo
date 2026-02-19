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

## Operation Guarantees

- Read operations are constrained to validated mount roots.
- Write operations are guarded by mount mode and write policy.
- Directory traversal and absolute-path escapes are rejected before I/O.
- Symlink escape attempts are denied by policy layer before operation execution.

## Atomic Write Contract

Write behavior uses a write-by-rename approach:

1. write payload to temporary file in target directory
2. optionally fsync based on config
3. rename temp file to target path atomically
4. cleanup temp file on error

This prevents partially written target files and is compatible with file watchers.
