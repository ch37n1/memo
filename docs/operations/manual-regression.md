---
description: Full manual regression runbook for memo v1 release validation
tags:
  - operations
  - qa
  - regression
---

# Manual Regression - v1

## Purpose

This runbook defines the full manual regression suite for `memo` v1.
Use it as the final human verification pass before release.

It validates:

- daemon lifecycle and health
- authentication and token lifecycle
- mount lifecycle and policy enforcement
- filesystem command behavior
- audit recording and querying
- CLI UX and exit code mapping
- security-critical path handling
- UI smoke coverage (`memo-ui`) for v1 desktop flow

## Scope

In scope:

- macOS full pass (primary platform)
- Linux smoke pass (secondary platform, critical paths only)
- CLI + daemon behavior across all v1 features
- UI smoke for shipped v1 UI scope

Out of scope:

- Windows
- performance benchmarking as a release gate (only sanity checks here)
- chaos/stress testing at scale

## Test Environment

### Required tools

- Rust toolchain from `rust-toolchain.toml`
- `cargo`
- `sqlite3`
- `jq`
- `curl`
- `rg`

### Runtime paths

- config: `~/.config/memo/config.toml`
- bootstrap token: `~/.config/memo/bootstrap.token`
- db: `~/.local/share/memo/memo.db`
- daemon log: `~/.local/state/memo/memod.log`
- audit log: `~/.local/state/memo/audit.log`
- pid file: `$XDG_RUNTIME_DIR/memo/memod.pid`
- launchd plist (macOS): `~/Library/LaunchAgents/io.github.ch37n1.memo.memod.plist`

### Fixture layout

Create local fixture roots:

- `/tmp/memo-regression/vault`
- `/tmp/memo-regression/scratch`
- `/tmp/memo-regression/external`

Suggested content:

1. `vault/index.md` with YAML frontmatter summary.
2. `vault/notes/a.md`, `vault/notes/b.md`.
3. `vault/private/secret.md`.
4. `vault/big.bin` around 6 MB.
5. symlink inside `vault` that points to `/tmp/memo-regression/external`.

## Execution Rules

- Run tests in order.
- Record outcome per case: `PASS`, `FAIL`, `BLOCKED`.
- On failure, capture:
  - command run
  - observed output
  - expected output
  - related log excerpt
- For destructive tests (`rm`, `mv`, `update`, `revoke`), confirm preconditions before running.

## Release Gate

All items must pass:

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- manual regression cases in this document

## Test Cases

### A. Install and startup

### RG-A01 Build succeeds

1. Run `cargo build`.
2. Run `cargo build -p memod`.
3. Run `cargo build -p memo`.

Expected:

- all commands exit `0`
- no build failures

### RG-A02 Daemon starts and health endpoint responds

1. Run `memo daemon start`.
2. Run `memo daemon status`.
3. Run `curl -sS http://127.0.0.1:18301/health`.

Expected:

- daemon status reports running
- health returns success payload

### RG-A03 Daemon stop/start cycle is stable

1. Run `memo daemon stop`.
2. Run `memo daemon status`.
3. Run `memo daemon start`.
4. Run `memo daemon status`.

Expected:

- stopped state is detected
- restarted daemon is healthy

### B. Bootstrap and auth

### RG-B01 Bootstrap token is created on fresh state

Precondition: no existing DB/token state.

1. Remove local memo state paths for test user.
2. Start daemon.
3. Check `~/.config/memo/bootstrap.token`.
4. Verify file mode is `0600`.

Expected:

- bootstrap token file exists
- token format matches `memo_<base62_32chars>`
- permissions are restricted (`0600`)

### RG-B02 Unauthenticated request is rejected

1. Call `memo mount list` without token.

Expected:

- failure maps to auth-required behavior
- CLI exits with code `2`

### RG-B03 Invalid token is rejected

1. Export `MEMO_TOKEN` with fake value.
2. Run `memo mount list`.

Expected:

- token-invalid response
- CLI exits with code `2`

### RG-B04 Expired token is rejected

1. Create token with past expiration.
2. Use it for `memo mount list`.

Expected:

- token-expired response
- CLI exits with code `2`

### RG-B05 Scope enforcement works

1. Create read-only FS token (`fs:Vault:read`).
2. Run `memo ls Vault:/`.
3. Run `memo write Vault:/notes/new.md --file ./README.md`.

Expected:

- read command succeeds
- write command denied with permission behavior
- denied action exits with code `3`

### C. Token lifecycle

### RG-C01 Token create/list/revoke flow

1. Run `memo token create --name regression-rw --scopes "fs:Vault:read,fs:Vault:write"`.
2. Run `memo token list`.
3. Revoke created token with `memo token revoke <token-id>`.
4. Run `memo token list` again.

Expected:

- created token appears in list with expected metadata
- revoked token is removed/inactive per API contract

### RG-C02 Revoked token cannot be used

1. Use revoked token to call `memo ls Vault:/`.

Expected:

- request fails as invalid token
- CLI exits with code `2`

### D. Mount lifecycle

### RG-D01 Add mount with policy fields

1. Run `memo mount add --name Vault --path /tmp/memo-regression/vault --mode read_write --audience human,agent --description "Regression mount" --hide-glob ".git/**" --deny-read-glob "private/**" --deny-write-glob "locked/**" --max-read-bytes 4194304 --max-write-bytes 2097152`.
2. Run `memo mount show Vault`.

Expected:

- mount created successfully
- all policy fields persisted correctly

### RG-D02 List and show mounts

1. Run `memo mount list`.
2. Run `memo mount show Vault`.

Expected:

- `Vault` present in list
- show output matches configured values

### RG-D03 Update mount fields and clear flags

1. Run `memo mount update Vault --description "Updated" --clear-hide-globs --max-write-bytes 1048576`.
2. Run `memo mount show Vault`.

Expected:

- description changes
- hide globs are cleared
- write size limit updated

### RG-D04 Duplicate mount name is rejected

1. Run `memo mount add` again with `--name Vault`.

Expected:

- conflict error
- CLI exits with code `1` or mapped conflict code path for implementation

### RG-D05 Remove mount and verify not found

1. Run `memo mount remove Vault`.
2. Run `memo mount show Vault`.

Expected:

- remove succeeds
- show returns not found
- CLI exits with code `4` for missing mount/path

### E. Filesystem behavior

Precondition: recreate `Vault` mount in read-write mode for `/tmp/memo-regression/vault`.

### RG-E01 ls and tree

1. Run `memo ls Vault:/`.
2. Run `memo tree Vault:/ --depth 2`.

Expected:

- directory entries listed
- tree depth respects input limits

### RG-E02 info and stat summary

1. Run `memo info Vault:/`.
2. Run `memo info Vault:/index.md`.

Expected:

- metadata is returned
- `memo_summary` is returned when frontmatter summary exists

### RG-E03 read and write file

1. Run `memo write Vault:/notes/new.md --file ./README.md`.
2. Run `memo cat Vault:/notes/new.md`.

Expected:

- write succeeds atomically
- read returns uploaded content

### RG-E04 mkdir and nested directory creation

1. Run `memo mkdir Vault:/nested/a/b/c`.
2. Run `memo ls Vault:/nested/a/b`.

Expected:

- nested directory exists

### RG-E05 move and rename

1. Run `memo mv Vault:/notes/new.md Vault:/notes/new-renamed.md`.
2. Verify old path missing and new path exists.

Expected:

- move succeeds within mount
- old path returns not found

### RG-E06 copy mount-to-mount and local-to-mount

1. Add `Scratch` mount for `/tmp/memo-regression/scratch`.
2. Run `memo cp Vault:/notes/a.md Scratch:/a-copy.md`.
3. Run `memo cp ./README.md Vault:/notes/local-copy.md`.

Expected:

- both copies succeed
- destination content matches source

### RG-E07 rm non-recursive and recursive

1. Create non-empty directory `Vault:/trash/dir`.
2. Run `memo rm Vault:/trash`.
3. Run `memo rm Vault:/trash --recursive`.

Expected:

- non-recursive delete of non-empty dir fails
- recursive delete succeeds

### RG-E08 grep behavior

1. Run `memo grep Vault:/notes "memo"`.
2. Run `memo grep Vault:/notes "MEMO" --case-insensitive`.
3. Run `memo grep Vault:/notes "memo" --max-results 1`.

Expected:

- matches returned with file and line context
- case-insensitive flag changes matching behavior
- result cap enforced

### RG-E09 find behavior

1. Run `memo find Vault:/ "*.md"`.
2. Run `memo find Vault:/ "*.md" --max-results 2`.

Expected:

- glob matches returned
- result cap enforced

### F. Policy and path security

### RG-F01 Reject absolute and traversal paths

1. Run commands with invalid paths:
   - `Vault:/../secret`
   - `Vault:/notes/../../etc/passwd`
   - absolute path input form if accepted by CLI parser

Expected:

- invalid/out-of-bounds response
- no filesystem mutation

### RG-F02 Hide glob returns not-found semantics

1. Configure `--hide-glob "private/**"` for `Vault`.
2. Run `memo ls Vault:/private`.
3. Run `memo info Vault:/private/secret.md`.

Expected:

- results behave as `not_found`
- existence is not leaked

### RG-F03 Deny read and deny write globs

1. Configure `--deny-read-glob "private/**"` and `--deny-write-glob "locked/**"`.
2. Run `memo cat Vault:/private/secret.md`.
3. Run `memo write Vault:/locked/new.md --file ./README.md`.

Expected:

- read denied with permission behavior
- write denied with permission behavior

### RG-F04 Symlink escape is denied

1. Create symlink in `Vault` pointing outside mount root.
2. Access through symlink path with `ls` or `cat`.

Expected:

- request rejected as symlink/out-of-bounds violation

### RG-F05 Size limit enforcement

1. Set `max_read_bytes` lower than `big.bin`.
2. Run `memo cat Vault:/big.bin`.
3. Set `max_write_bytes` to small value.
4. Try writing file larger than limit.

Expected:

- operations fail with `too_large`

### G. Audit and observability

### RG-G01 Successful and denied operations are audited

1. Execute one allowed read.
2. Execute one denied write.
3. Query audit via `memo audit --limit 20`.

Expected:

- both events present
- results include success and denied outcome types

### RG-G02 Audit filters work

1. Query with `--mount Vault`.
2. Query with `--operation read`.
3. Query with `--result denied`.
4. Query with `--before` and `--after`.

Expected:

- filters narrow results correctly

### RG-G03 Audit log file integrity

1. Open `~/.local/state/memo/audit.log`.
2. Verify lines are valid JSON.
3. Verify ids are monotonically increasing.

Expected:

- append-only JSONL format remains valid
- ordering is deterministic

### RG-G04 Daemon logs provide failure diagnostics

1. Trigger auth and policy failures.
2. Run `memo daemon logs --tail 100`.

Expected:

- errors are logged with enough context to diagnose

### H. CLI behavior and output contracts

### RG-H01 JSON output mode for all command groups

1. Run with `--json`:
   - `memo --json mount list`
   - `memo --json token list`
   - `memo --json ls Vault:/`
   - `memo --json audit`

Expected:

- valid JSON output for each command
- structure is machine-parseable (`jq` succeeds)

### RG-H02 Plain text output mode readability

1. Run same commands without `--json`.

Expected:

- human-readable output
- no JSON-specific artifacts

### RG-H03 Exit code mapping

Validate:

1. success returns `0`
2. auth errors return `2`
3. permission errors return `3`
4. not found returns `4`
5. daemon unreachable returns `5`
6. other failures return `1`

Expected:

- exit codes match CLI design contract

### I. Daemon lifecycle and resilience

### RG-I01 Status reflects real process state

1. Stop daemon.
2. Check status.
3. Start daemon.
4. Check status and health.

Expected:

- status output always matches actual daemon state

### RG-I02 Restart preserves data

1. Create mount and files.
2. Restart daemon.
3. Re-run `mount list` and read files.

Expected:

- DB-backed data persists across restarts

### RG-I03 Concurrent command sanity

1. Run two or more read/write CLI commands in parallel.

Expected:

- no daemon crash
- responses remain consistent

### J. memo-ui smoke (v1 UI scope)

Run only if `memo-ui` is part of v1 release artifact.

### RG-J01 App starts and connects to daemon

1. Launch UI with daemon running.
2. Confirm health/connected indicator.

Expected:

- UI connects without direct JS fetch bypass

### RG-J02 Mount management from UI

1. Create mount from UI.
2. Edit and remove mount.

Expected:

- operations succeed
- CLI reflects same state

### RG-J03 Token flow from UI

1. Create token from UI.
2. Use token in CLI.
3. Revoke token from UI.

Expected:

- token lifecycle is consistent across UI and CLI

### RG-J04 Audit viewer in UI

1. Trigger events from CLI.
2. Filter in UI audit page.

Expected:

- UI audit entries align with CLI and log file

### K. Linux smoke subset

Run on Linux for critical confidence:

1. daemon start/stop/status
2. auth with valid/invalid token
3. mount add/list/remove
4. ls/cat/write/mkdir/rm
5. one traversal and one symlink denial test
6. audit query smoke

Expected:

- no platform-specific regression on core behaviors

## Test Report Template

Use this template per run:

```md
# memo v1 Manual Regression Report

- Date:
- Tester:
- Commit/Tag:
- Platform:
- Result: PASS | FAIL | BLOCKED

## Summary

- Total:
- Passed:
- Failed:
- Blocked:

## Failed Cases

| Case ID | Command/Area | Expected | Observed | Notes |
|---------|--------------|----------|----------|-------|

## Logs / Artifacts

- daemon log:
- audit log:
- screenshots (if UI):
```

## Post-run cleanup

1. Revoke all temporary regression tokens.
2. Remove temporary mounts (`Vault`, `Scratch`).
3. Stop daemon if not needed.
4. Archive report with logs.
