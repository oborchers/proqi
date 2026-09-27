//! Recent forward-only schema migrations.

// Add payload-free durable visual separators in the shared Board order.
pub(crate) const MIGRATION_17: &str = r"
CREATE TABLE separators (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER
) STRICT;
CREATE UNIQUE INDEX separators_live_position
ON separators(session_id, position)
WHERE deleted_at IS NULL;
CREATE INDEX separators_session ON separators(session_id);
UPDATE schema_meta SET schema_version = 17, storage_protocol = 16;
INSERT INTO migration_history(version, applied_at) VALUES (17, 0);
";

// Add optional organizational names without changing authored thought content.
pub(crate) const MIGRATION_18: &str = r"
ALTER TABLE thoughts ADD COLUMN name TEXT;
UPDATE schema_meta SET schema_version = 18, storage_protocol = 17;
INSERT INTO migration_history(version, applied_at) VALUES (18, 0);
";

// Retain an exact content-redacted API request identity after history compaction.
pub(crate) const MIGRATION_19: &str = r"
ALTER TABLE commit_receipts ADD COLUMN semantic_fingerprint BLOB
    CHECK (semantic_fingerprint IS NULL OR length(semantic_fingerprint) = 32);
UPDATE schema_meta SET schema_version = 19, storage_protocol = 18;
INSERT INTO migration_history(version, applied_at) VALUES (19, 0);
";

// Recover one selected cross-session transfer with one atomic destination cohort.
pub(crate) const MIGRATION_20: &str = r"
CREATE TABLE transfer_attempts (
    operation_id BLOB PRIMARY KEY CHECK (length(operation_id) = 16),
    source_session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    destination_session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    removal_operation_id BLOB NOT NULL UNIQUE CHECK (length(removal_operation_id) = 16),
    request_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('prepared', 'sending', 'accepted', 'completed')),
    destination_receipt_json TEXT,
    completed_reason TEXT,
    created_at INTEGER NOT NULL,
    CHECK ((status IN ('accepted', 'completed')) = (destination_receipt_json IS NOT NULL))
) STRICT;
CREATE TABLE transfer_source_claims (
    source_thought_id BLOB PRIMARY KEY CHECK (length(source_thought_id) = 16),
    operation_id BLOB NOT NULL REFERENCES transfer_attempts(operation_id) ON DELETE CASCADE
) STRICT;
UPDATE schema_meta SET schema_version = 20, storage_protocol = 19;
INSERT INTO migration_history(version, applied_at) VALUES (20, 0);
";
