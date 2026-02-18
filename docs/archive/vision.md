---
description: Project vision, doc that created before design doc. First view on the project.
created_at: 2026.02.18
deprecated_at: 2026.02.19
---

> [!warning] Archived
> This document is archived and no longer current. See `docs/README.md` for current information.

# memo — Vision (v1)

## 1. Purpose

> Document for brief preparation before full system design.

I want to create a shared memory system where I can store notes and manuals that I find useful. It should be designed so that both the agent and I can use them.

For example, I would like to link a subdirectory of my Obsidian vault so that part of it is accessible, but the agent cannot read personal notes. There, we could develop a knowledge base together. (Something like skills, but ones that are also useful for humans.)

At the same time, it is possible to link other folders that will contain data only for the agent or agents.

`memo` is a mount-scoped, daemon-based collaborative filesystem layer designed for safe human–agent knowledge work.

It provides controlled access to selected subdirectories (e.g. `Vault/SharedKB/` in Obsidian) while strictly preventing access to parent or neighboring directories. The system is file-based, Markdown-native, and intentionally minimal.

File-based is the main memory type for now, but in future we will extend `memo` with embeddings and graph. File is main because it's best way to have unified memory for human and agent.

`memo` follows a client–server architecture similar in spirit to Docker:

* **`memod`** — daemon (server, owns all filesystem I/O)
* **`memo`** — CLI client
* **`memo-ui`** — Tauri-based admin UI client

All filesystem access occurs exclusively inside the daemon.

---

## 2. Core Principles

### 2.1 Mount-Scoped Isolation

* All client paths are relative to a **named mount**.
* Clients never use raw system paths.
* A mount points to a specific root directory.
* Access outside the mount root is impossible.

### 2.2 Strict Boundary Guarantees

The daemon guarantees:

* No access to parent directories.
* No access to sibling directories.
* No traversal using `..`, absolute paths, or malformed paths.
* No symlink traversal.
* Symlinks are not shown and not accessible.

### 2.3 File-Based Model

* The system exposes a simple filesystem abstraction.
* No virtual views.
* No embedding store.
* No shell execution.
* No full Unix semantics.
* Only regular files and directories are supported.

### 2.4 Atomic Writes

All write operations use atomic write-by-rename:

1. Write to temporary file in same directory.
2. `fsync` (optional but supported).
3. Atomic rename to target.
4. Optional directory sync.

This ensures compatibility with file watchers (e.g. Obsidian).

### 2.5 Uniform Policy Enforcement

Policies apply consistently to:

* list
* read
* write
* move
* delete
* search

There are no cosmetic-only hide rules. Enforcement is centralized in the daemon.

---

## 3. Mount Types

### 3.1 Shared Mount (Human + Agent)

Example:

```
Vault/SharedKB/
```

* Read-write
* Markdown-first
* Designed for collaborative refinement
* Edited in Obsidian, written by agent, polished by human

### 3.2 Agent-Only Mount

Two supported approaches:

1. Managed internal directory (e.g. `~/.local/share/memo/agent/`)
2. User-defined mount pointing to any directory

Intended for:

* Scratch files
* Intermediate artifacts
* Cache
* Binary assets

---

## 4. Metadata Model

### 4.1 Directory-Level Metadata (File-Based)

If a directory contains `index.md`, it is treated as the canonical description of that directory.

Optional frontmatter:

```yaml
---
memo:
  summary: "Short description"
  owners: ["human", "agent"]
  status: "stable"
---
```

This is human-native and Obsidian-compatible.

### 4.2 Mount-Level Metadata (Daemon-Managed)

Stored in configuration / SQLite:

* Mount name
* Root path
* Mode (ro/rw)
* Description
* Audience (shared / agent-only / human-only)
* Policy summary

This metadata is used for UI display and quick CLI output.

---

## 5. Architecture

### 5.1 Binaries

* `memod` — daemon
* `memo` — CLI
* `memo-ui` — Tauri application

### 5.2 Communication

* Local-only (Unix domain socket on macOS/Linux)
* Token-based authentication
* Scope-based authorization

### 5.3 Database (v1)

* SQLite
* Stores:

  * Mount configuration
  * Tokens (hashed)
  * Audit log
  * Optional cached metadata

No external dependency required.

---

## 6. Token Model

### 6.1 Characteristics

* Opaque random tokens
* Stored hashed (Argon2)
* Scoped per mount and per operation
* Named tokens for clarity

### 6.2 Scope Examples

* `fs:VaultKB:read`
* `fs:VaultKB:write`
* `meta:read`
* `admin:mounts`
* `admin:tokens`

### 6.3 Storage

* Human tokens: OS keychain preferred, fallback to secure file (`0600`)
* Agent tokens: environment variable or secure file

---

## 7. Filesystem API (v1)

All endpoints under:

```
/v1/fs/*
```

All paths are:

```
<MountName>:/relative/path
```

### Core Operations

* `ls`
* `tree`
* `stat`
* `read`
* `write`
* `mkdir`
* `mv`
* `rm`
* `cp`
* `grep` (text search)
* `find` (glob-based search)

### Large Files

* Streamed reads/writes
* Max size configurable (default supports large files up to ~1GB)
* No type restrictions

---

## 8. CLI Design

Unix-inspired command naming:

```
memo ls VaultKB:/
memo tree VaultKB:/ --depth 3
memo cat VaultKB:/manuals/git.md
memo mkdir VaultKB:/manuals
memo mv VaultKB:/drafts/x.md VaultKB:/manuals/x.md
memo rm VaultKB:/drafts/x.md
memo cp ./image.png VaultKB:/assets/image.png
memo grep "pattern" VaultKB:/
memo find "*.md" VaultKB:/manuals
```

### Directory Description

Two supported forms:

```
memo info VaultKB:/manuals
memo ls --info VaultKB:/manuals
```

Both show:

* Mount metadata
* Directory summary (if `index.md` exists)
* Directory listing

---

## 9. Mount Configuration (Conceptual)

Each mount defines:

* name
* root path
* mode (ro/rw)
* hide globs
* deny read globs
* deny write globs
* max read bytes
* max write bytes
* follow_symlinks = false (always false in v1)

Mount name defaults to folder basename but can be overridden.

---

## 10. Security Guarantees

* No symlinks exposed or traversed
* No `..` traversal
* No absolute paths
* No raw filesystem access from clients
* Daemon owns all filesystem I/O
* Atomic write-by-rename
* Uniform policy enforcement

---

## 11. Future Extension (v2 / v3)

The API namespace is structured for future growth:

* `/v1/fs/*` — file operations
* `/v1/meta/*` — mounts, tokens, audit
* `/v1/objects/*` — reserved for future non-file memory (graph/object layer)

v1 remains purely file-based but structurally ready for additional resource families.

---

## 12. LLM-Agent Friendliness

`memo` must be explicitly designed to be usable by LLM-based agents without additional adaptation layers.

### 12.1 Deterministic CLI Output

All CLI commands must:

* Produce stable, structured, and predictable output.
* Avoid decorative formatting (no colors unless explicitly requested).
* Support machine-friendly formats (e.g. `--json` flag for all commands).
* Use consistent error codes and structured error messages.

This ensures agents can reliably parse and react to results.

### 12.2 Skill-Oriented `--help`

`memo --help` and `<command> --help` must:

* Clearly describe command purpose in concise operational terms.
* Provide minimal but complete usage examples.
* Avoid ambiguity or marketing-style prose.
* Be formatted in a way that can be directly transformed into an agent “skill” definition.

The help output should make it straightforward for an agent to:

* Identify available commands.
* Understand required vs optional parameters.
* Infer side effects (read-only vs write operations).
* Understand mount-qualified path syntax.

### 12.3 Explicit, Intent-Centric Commands

Commands should be named by intent (e.g. `ls`, `read`, `write`, `grep`, `find`, `cp`, `mv`) and not expose low-level filesystem details.

The CLI surface must remain:

* Small
* Orthogonal
* Composable

This reduces ambiguity in agent planning and improves reliability.

### 12.4 Structured Exit Semantics

* Exit code `0` → success.
* Non-zero → failure with structured error message.
* Errors must include:

  * error type
  * mount name (if relevant)
  * path (if relevant)
  * policy violation reason (if applicable)

This allows agents to distinguish between:

* path-not-found
* permission-denied
* policy-restricted
* invalid-syntax
* internal-error

### 12.5 Stable Contract

The CLI and `/v1/fs/*` API together form a stable contract.

Agents should be able to:

* Generate skills automatically from `--help`
* Operate without hidden state
* Avoid undefined behaviors

LLM compatibility is a first-class design requirement, not an afterthought.

---

## 13. Summary

`memo` is a secure, mount-scoped, daemon-backed filesystem façade for collaborative human–agent knowledge work.

It enforces strict directory isolation, provides atomic and policy-controlled file operations, and integrates naturally with Markdown-based workflows such as Obsidian.

The architecture mirrors a client–server model (`memod`, `memo`, `memo-ui`), with all filesystem access owned by the daemon and all paths scoped to named mounts.

The system is intentionally minimal, file-native, and local-first, while being explicitly designed to be LLM-agent friendly through deterministic CLI behavior, structured outputs, and skill-oriented command design.

It prioritizes:

* strict directory isolation
* Markdown-native workflow
* atomic safety
* Unix-like ergonomics
* simple, local-first architecture
* extensibility without premature complexity

v1 intentionally excludes:

* embeddings
* semantic indexing
* shell execution
* virtual filesystem views
* non-file memory types

v1 delivers a robust and safe collaborative filesystem layer, ready for future expansion into richer memory models without compromising its foundational guarantees.

