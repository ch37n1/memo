---
description: Update guide for memod, memo CLI, and memo-ui binaries, including Semantic Versioning policy and smoke checks
tags:
  - operations
  - install
  - update
  - release
---

# Update Guide (Binary Runtime)

## Versioning Policy

`memo` uses [Semantic Versioning](https://semver.org/) in the format:

- `MAJOR.MINOR.PATCH`
- optional pre-release suffix, for example `-alpha`, `-beta`, `-rc.1`

Current project version is:

- `1.0.0-alpha`

Update impact rules:

- `MAJOR` change: may include breaking changes.
- `MINOR` change: backward-compatible feature additions.
- `PATCH` change: backward-compatible fixes only.
- pre-release tags: may change more frequently before stable release.

## Scope

This procedure updates all three runtime components installed as binaries/artifacts:

- `memod` daemon binary
- `memo` CLI binary
- `memo-ui.app` macOS desktop app

## Pre-Update Safety

1. Confirm daemon health before update:

```bash
memo daemon status
```

2. Ensure you still have admin token:

- `~/.config/memo/bootstrap.token` or another active admin token in secure storage.

3. Optional backup of persistent state:

```bash
cp "$HOME/.local/share/memo/memo.db" "$HOME/.local/share/memo/memo.db.backup.$(date +%Y%m%d%H%M%S)"
cp "$HOME/.local/state/memo/audit.log" "$HOME/.local/state/memo/audit.log.backup.$(date +%Y%m%d%H%M%S)"
```

## Update Steps

### 1. Stop daemon

```bash
memo daemon stop
```

### 2. Update `memod` and `memo` binaries

From repo root on the target release tag/commit:

```bash
cargo install --path crates/memod --root "$HOME/.local" --force
cargo install --path crates/memo  --root "$HOME/.local" --force
```

Verify:

```bash
memo --version
memod --version
```

### 3. Update `memo-ui.app`

```bash
cd crates/memo-ui
npm install
cargo tauri build
```

Replace installed app:

```bash
rm -rf /Applications/memo-ui.app
cp -R src-tauri/target/release/bundle/macos/memo-ui.app /Applications/
```

### 4. Start daemon again

```bash
memo daemon start
memo daemon status
```

## Post-Update Smoke Tests

Run quick checks:

```bash
# 1) daemon and API
memo daemon status

# 2) mounts still visible
memo mount list

# 3) fs write/read roundtrip
echo "update-smoke $(date +%s)" | memo write VaultKB:/notes/update-smoke.md
memo cat VaultKB:/notes/update-smoke.md

# 4) audit still works
memo audit --limit 5
```

Expected:

- daemon status is healthy and reports the new version.
- mount list returns existing mounts.
- write/read roundtrip succeeds.
- audit returns recent entries including smoke commands.

## Rollback

If smoke tests fail:

1. Stop daemon: `memo daemon stop`
2. Reinstall previous known-good binaries and previous `memo-ui.app`
3. Restore DB/audit backups if migration or data issue is confirmed
4. Start daemon and rerun smoke tests
