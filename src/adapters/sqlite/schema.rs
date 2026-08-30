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

CREATE TABLE onboarding_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    completed_version INTEGER NOT NULL CHECK (completed_version >= 0)
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
    presentation TEXT NOT NULL DEFAULT 'automatic'
        CHECK (presentation IN ('automatic', 'expanded', 'collapsed')),
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

CREATE TABLE submission_attempt_items (
    submission_id BLOB NOT NULL REFERENCES submission_attempts(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    PRIMARY KEY(submission_id, ordinal),
    UNIQUE(submission_id, thought_id)
) STRICT;

CREATE TABLE screenshot_capture_receipts (
    source_fingerprint BLOB PRIMARY KEY CHECK (length(source_fingerprint) = 32),
    session_id BLOB NOT NULL CHECK (length(session_id) = 16),
    thought_id BLOB NOT NULL CHECK (length(thought_id) = 16),
    operation_id BLOB NOT NULL CHECK (length(operation_id) = 16),
    accepted_at INTEGER NOT NULL,
    UNIQUE(operation_id)
) STRICT;

CREATE UNIQUE INDEX submission_attempt_items_active_thought
ON submission_attempt_items(thought_id)
WHERE active = 1;

CREATE VIRTUAL TABLE session_search USING fts5(
    session_id UNINDEXED,
    name,
    paths,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO schema_meta(singleton, schema_version, storage_protocol, migrated_at)
VALUES (1, 9, 9, 0);
INSERT INTO onboarding_state(singleton, completed_version) VALUES (1, 0);
INSERT INTO migration_history(version, applied_at) VALUES (1, 0);
INSERT INTO migration_history(version, applied_at) VALUES (2, 0);
INSERT INTO migration_history(version, applied_at) VALUES (3, 0);
INSERT INTO migration_history(version, applied_at) VALUES (4, 0);
INSERT INTO migration_history(version, applied_at) VALUES (5, 0);
INSERT INTO migration_history(version, applied_at) VALUES (6, 0);
INSERT INTO migration_history(version, applied_at) VALUES (7, 0);
INSERT INTO migration_history(version, applied_at) VALUES (8, 0);
INSERT INTO migration_history(version, applied_at) VALUES (9, 0);
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

pub(super) const MIGRATION_4: &str = r"
UPDATE schema_meta SET schema_version = 4, storage_protocol = 4;
INSERT INTO migration_history(version, applied_at) VALUES (4, 0);
";

pub(super) const MIGRATION_5: &str = r"
CREATE TABLE submission_attempt_items (
    submission_id BLOB NOT NULL REFERENCES submission_attempts(id) ON DELETE CASCADE,
    thought_id BLOB NOT NULL REFERENCES thoughts(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    source_digest BLOB NOT NULL CHECK (length(source_digest) = 32),
    active INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    PRIMARY KEY(submission_id, ordinal),
    UNIQUE(submission_id, thought_id)
) STRICT;
INSERT INTO submission_attempt_items(submission_id, thought_id, ordinal, source_digest, active)
SELECT id, thought_id, 0, source_digest,
       CASE WHEN state IN ('prepared', 'sending') THEN 1 ELSE 0 END
FROM submission_attempts;
DROP INDEX submission_attempts_active_thought;
CREATE UNIQUE INDEX submission_attempt_items_active_thought
ON submission_attempt_items(thought_id)
WHERE active = 1;
UPDATE schema_meta SET schema_version = 5, storage_protocol = 5;
INSERT INTO migration_history(version, applied_at) VALUES (5, 0);
";

pub(super) const MIGRATION_6: &str = r"
ALTER TABLE thoughts ADD COLUMN presentation TEXT NOT NULL DEFAULT 'automatic'
    CHECK (presentation IN ('automatic', 'expanded', 'collapsed'));
UPDATE thoughts SET presentation = 'collapsed' WHERE collapsed = 1;
UPDATE schema_meta SET schema_version = 6, storage_protocol = 6;
INSERT INTO migration_history(version, applied_at) VALUES (6, 0);
";

pub(super) const MIGRATION_7: &str = r"
CREATE TABLE screenshot_capture_receipts (
    source_fingerprint BLOB PRIMARY KEY CHECK (length(source_fingerprint) = 32),
    session_id BLOB NOT NULL CHECK (length(session_id) = 16),
    thought_id BLOB NOT NULL CHECK (length(thought_id) = 16),
    operation_id BLOB NOT NULL CHECK (length(operation_id) = 16),
    accepted_at INTEGER NOT NULL,
    UNIQUE(operation_id)
) STRICT;
UPDATE schema_meta SET schema_version = 7, storage_protocol = 7;
INSERT INTO migration_history(version, applied_at) VALUES (7, 0);
";

pub(super) const MIGRATION_8: &str = r"
ALTER TABLE screenshot_capture_receipts RENAME TO screenshot_capture_receipts_v7;
CREATE TABLE screenshot_capture_receipts (
    source_fingerprint BLOB PRIMARY KEY CHECK (length(source_fingerprint) = 32),
    session_id BLOB NOT NULL CHECK (length(session_id) = 16),
    thought_id BLOB NOT NULL CHECK (length(thought_id) = 16),
    operation_id BLOB NOT NULL CHECK (length(operation_id) = 16),
    accepted_at INTEGER NOT NULL,
    UNIQUE(operation_id)
) STRICT;
INSERT INTO screenshot_capture_receipts(
    source_fingerprint, session_id, thought_id, operation_id, accepted_at
)
SELECT source_fingerprint, session_id, thought_id, operation_id, accepted_at
FROM screenshot_capture_receipts_v7;
DROP TABLE screenshot_capture_receipts_v7;
UPDATE schema_meta SET schema_version = 8, storage_protocol = 8;
INSERT INTO migration_history(version, applied_at) VALUES (8, 0);
";

pub(super) const MIGRATION_9: &str = r"
CREATE TABLE onboarding_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    completed_version INTEGER NOT NULL CHECK (completed_version >= 0)
) STRICT;
INSERT INTO onboarding_state(singleton, completed_version) VALUES (1, 1);
UPDATE schema_meta SET schema_version = 9, storage_protocol = 9;
INSERT INTO migration_history(version, applied_at) VALUES (9, 0);
";
