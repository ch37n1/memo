---
description: DomainEvent JSON encoding reference for audit and integration consumers
tags:
  - references
  - events
  - serde
---

# Domain Event JSON

`DomainEvent` is serialized as an internally tagged enum:

- discriminator field: `type`
- variant naming: `snake_case`

## Example

```json
{
  "type": "mount_registered",
  "name": "VaultKB",
  "mode": "read_write"
}
```

## Notes

- Consumers should dispatch on `type` first.
- Unknown `type` values should be handled as forward-compatible unknown events.
- Field names are part of the wire contract and should not be changed without a documented migration.
