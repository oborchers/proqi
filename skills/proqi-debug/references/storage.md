# Proqi storage and diagnostics

Read this reference only for persistence, migration, lease, undo, or submission
failures. It describes the durable model. It is not a repair procedure.

## Storage layout

Proqi stores one SQLite database named `proqi.sqlite3` in the platform-native
application data directory. SQLite runs in WAL mode with full synchronous
durability, a busy timeout, and bounded contention retries. The `-wal` and
`-shm` companions are part of a live database and must not be deleted, edited,
or copied independently.

Important tables have distinct responsibilities:

- `schema_meta` records the current schema and storage protocol.
- `migration_history` is the append-only record of forward migrations.
- `sessions` and `thoughts` hold current durable board state.
- `board_operations` and `thought_revisions` hold bounded persistent undo and
  redo history.
- `commit_receipts` make durable mutations idempotent and replayable.
- `integration_context` stores non-content integration metadata.
- `submission_attempts` records content-redacted submission intent and outcome.
- `session_search` is a derived FTS5 index and is not authoritative data.

Typed identifiers are stored as complete 16-byte UUIDv7 blobs. Their public
forms retain the entity prefix, such as `ses_`, `tht_`, `op_`, and `sub_`.
SQLite statements use bound parameters. Migrations are forward-only, guarded by
schema coordination, preceded by backup and integrity checks, and refuse a
newer unsupported schema.

The operating-system lease is authoritative for active ownership. Database and
runtime metadata are descriptive and must not be used to bypass a verified
owner. Two processes must never write one session directly.

## Submission interpretation

A submission progresses through `prepared`, `sending`, then `accepted`,
`failed`, `cancelled`, or `outcome_unknown`. The journal stores a source digest
and revision, not prompt content. One active attempt is permitted per thought.

An accepted semantic Herdr receipt establishes delivery. Agent readiness after
submission is advisory. If Proqi crashes after delivery but before recording
the receipt, `outcome_unknown` prevents an unsafe automatic retry.

## Diagnostics

Each running instance owns a content-redacted JSONL stream in the
`diagnostics` directory. It retains up to five 1 MiB segments. Inactive streams
are pruned toward a 20 MiB installation-wide limit. Expected event families
include:

- `diagnostics_initialized` and `runtime_opening`.
- `shutdown_started` and `shutdown_finished`.
- `command_succeeded` and `command_failed` with a stable code.
- `submission_transition` with `sub_` identity, state, direction, provider, and
  optional outcome code.

Use `proqi diagnostics collect --output PATH` to create a versioned, private
JSON bundle. The command refuses overwrite and performs no upload.

## Safe investigation rules

1. Prefer public CLI output and the collected diagnostics bundle.
2. Correlate by time, stable error code, and typed identifier.
3. Work against a minimal test session when reproduction is needed.
4. If low-level inspection is unavoidable, stop and obtain explicit permission
   before copying user data. Quiesce all owners and preserve the database, WAL,
   and SHM as one snapshot.
5. Never run repair, migration, delete, vacuum, or update SQL against the live
   database.
6. Never publish raw database content or a diagnostic bundle.
