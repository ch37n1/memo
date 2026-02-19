---
description: API error code reference mapped to semantic meaning
tags:
  - references
  - api
  - errors
---

# API Error Codes

`ApiError::code()` returns stable machine-readable identifiers.

| Code | Meaning |
|------|---------|
| `auth_required` | No authentication token provided |
| `token_invalid` | Token failed verification |
| `token_expired` | Token is expired |
| `permission_denied` | Request is authenticated but lacks required permission |
| `policy_violated` | Request violates policy rules |
| `invalid_path` | Provided path format is invalid |
| `out_of_bounds` | Path escapes mount root |
| `symlink_denied` | Symlink access denied |
| `not_found` | Target resource not found or hidden |
| `mount_not_found` | Referenced mount does not exist |
| `conflict` | State conflict (for example duplicate or invalid transition) |
| `too_large` | Payload or file exceeds configured size limit |
| `internal_error` | Internal or configuration error |
