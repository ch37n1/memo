CREATE TABLE mounts (
  name            TEXT PRIMARY KEY,
  root_path       TEXT NOT NULL,
  mode            TEXT NOT NULL CHECK(mode IN ('ro', 'rw')),
  audience        TEXT NOT NULL CHECK(audience IN ('shared', 'agent-only', 'human-only')),
  description     TEXT,
  hide_globs      TEXT NOT NULL DEFAULT '[]',
  deny_read_globs TEXT NOT NULL DEFAULT '[]',
  deny_write_globs TEXT NOT NULL DEFAULT '[]',
  max_read_bytes  INTEGER,
  max_write_bytes INTEGER,
  created_at      TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tokens (
  id           TEXT PRIMARY KEY,
  name         TEXT NOT NULL,
  hash         TEXT NOT NULL,
  scopes       TEXT NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  expires_at   TEXT,
  last_used_at TEXT
);
