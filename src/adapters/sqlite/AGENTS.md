# SQLite adapter contract

These rules apply to the SQLite adapter in addition to the repository contract.

- SQLite is canonical for sessions, thoughts, revisions, operations, and
  integration context. Full-text indexes are derived and rebuildable.
- Preserve WAL mode, `synchronous=FULL`, bounded busy retries, and short write
  transactions. Never silently weaken acknowledged durability.
- Every multi-row state change, reorder, undo, redo, trash, restore, and
  idempotent mutation is atomic.
- Session operation sequences remain monotonic. Store typed identifiers as
  lossless 128-bit values and validate prefixes at external boundaries.
- Migrations are forward only. Require the exclusive schema lease, integrity
  check, and successful pre-migration backup before changing a schema.
- Refuse newer schemas and conservative mixed-version migration hazards. Never
  reinterpret unknown durable data.
- Active session owners receive mutations through the control protocol. Do not
  write around a verified owner directly through SQLite.
- Runtime metadata is descriptive, never authoritative. Locks and leases decide
  ownership, and stale metadata cleanup must be crash safe.
- Database, backup, and runtime files use user-only permissions where supported.
- Corruption, disk-full, contention, and backup failures remain typed and visible.
  Failed mutations must never be reported as durable.
- Test with real temporary on-disk databases, multiple connections and
  processes, contention, rollback, crashes, migrations, backup failures, and
  recovery. Do not rely only on in-memory SQLite tests.
