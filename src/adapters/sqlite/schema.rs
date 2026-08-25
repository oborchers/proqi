//! Forward-only SQLite schema.

pub(super) const MIGRATION_1: &str = r"
CREATE TABLE schema_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    schema_version INTEGER NOT NULL,
    storage_protocol INTEGER NOT NULL,
    migrated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE migration_history (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
) STRICT;

CREATE TABLE sessions (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    name TEXT,
    origin_cwd BLOB NOT NULL,
    last_opened_cwd BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_opened_at INTEGER NOT NULL,
    last_active_at INTEGER NOT NULL,
    last_durable_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_durable_sequence >= 0),
    board_history_cursor INTEGER NOT NULL DEFAULT 0 CHECK (board_history_cursor >= 0),
    deleted_at INTEGER
) STRICT;

CREATE TABLE thoughts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    annotations_json TEXT NOT NULL DEFAULT '[]',
    position INTEGER NOT NULL CHECK (position >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    collapsed INTEGER NOT NULL DEFAULT 0 CHECK (collapsed IN (0, 1)),
    deleted_at INTEGER,
    editor_history_cursor INTEGER NOT NULL DEFAULT 0 CHECK (editor_history_cursor >= 0)
) STRICT;

CREATE UNIQUE INDEX thoughts_live_position
ON thoughts(session_id, position)
WHERE deleted_at IS NULL;

CREATE INDEX thoughts_session ON thoughts(session_id);

CREATE TABLE board_operations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    history_index INTEGER NOT NULL CHECK (history_index >= 0),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(session_id, history_index)
) STRICT;

CREATE TABLE thought_revisions (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    history_index INTEGER NOT NULL CHECK (history_index >= 0),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(thought_id, history_index)
) STRICT;

CREATE TABLE commit_receipts (
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    entity_kind TEXT NOT NULL,
    external_id BLOB NOT NULL CHECK (length(external_id) = 16),
    request_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(session_id, sequence),
    UNIQUE(entity_kind, external_id)
) STRICT;

CREATE TABLE integration_context (
    session_id BLOB PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL
) STRICT;

CREATE TABLE submission_attempts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    source_sequence INTEGER NOT NULL CHECK (source_sequence >= 0),
    disposition TEXT NOT NULL CHECK (disposition IN ('keep', 'remove_after_success')),
    direction TEXT NOT NULL CHECK (direction IN ('up', 'right', 'down', 'left')),
    provider TEXT NOT NULL,
    protocol INTEGER NOT NULL CHECK (protocol >= 0),
    target_fingerprint BLOB NOT NULL CHECK (length(target_fingerprint) = 32),
    pre_state TEXT NOT NULL,
    post_state TEXT,
    error_code TEXT,
    deletion_operation_id BLOB CHECK (deletion_operation_id IS NULL OR length(deletion_operation_id) = 16),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'sending', 'accepted', 'failed', 'cancelled', 'outcome_unknown')),
    prepared_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX submission_attempts_active_thought
ON submission_attempts(thought_id)
WHERE state IN ('prepared', 'sending');

CREATE VIRTUAL TABLE session_search USING fts5(
    session_id UNINDEXED,
    name,
    paths,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO schema_meta(singleton, schema_version, storage_protocol, migrated_at)
VALUES (1, 3, 3, 0);
INSERT INTO migration_history(version, applied_at) VALUES (1, 0);
INSERT INTO migration_history(version, applied_at) VALUES (2, 0);
INSERT INTO migration_history(version, applied_at) VALUES (3, 0);
";

pub(super) const MIGRATION_2: &str = r"
ALTER TABLE thoughts ADD COLUMN annotations_json TEXT NOT NULL DEFAULT '[]';
UPDATE schema_meta SET schema_version = 2, storage_protocol = 2;
INSERT INTO migration_history(version, applied_at) VALUES (2, 0);
";

pub(super) const MIGRATION_3: &str = r"
CREATE TABLE submission_attempts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    session_id BLOB NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    source_sequence INTEGER NOT NULL CHECK (source_sequence >= 0),
    disposition TEXT NOT NULL CHECK (disposition IN ('keep', 'remove_after_success')),
    direction TEXT NOT NULL CHECK (direction IN ('up', 'right', 'down', 'left')),
    provider TEXT NOT NULL,
    protocol INTEGER NOT NULL CHECK (protocol >= 0),
    target_fingerprint BLOB NOT NULL CHECK (length(target_fingerprint) = 32),
    pre_state TEXT NOT NULL,
    post_state TEXT,
    error_code TEXT,
    deletion_operation_id BLOB CHECK (deletion_operation_id IS NULL OR length(deletion_operation_id) = 16),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'sending', 'accepted', 'failed', 'cancelled', 'outcome_unknown')),
    prepared_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX submission_attempts_active_thought
ON submission_attempts(thought_id)
WHERE state IN ('prepared', 'sending');
UPDATE schema_meta SET schema_version = 3, storage_protocol = 3;
INSERT INTO migration_history(version, applied_at) VALUES (3, 0);
";
