//! Storage-protocol compatibility stamps that change no table shape.
//!
//! Each migration registers durable payloads that an older writer must not
//! interpret, so it advances the protocol stamp without rewriting rows.

pub(in crate::adapters::sqlite) const MIGRATION_9: &str = r"
UPDATE schema_meta SET schema_version = 9, storage_protocol = 9;
INSERT INTO migration_history(version, applied_at) VALUES (9, 0);
";

pub(in crate::adapters::sqlite) const MIGRATION_10: &str = r"
UPDATE schema_meta SET schema_version = 10, storage_protocol = 10;
INSERT INTO migration_history(version, applied_at) VALUES (10, 0);
";

pub(in crate::adapters::sqlite) const MIGRATION_12: &str = r"
UPDATE schema_meta SET schema_version = 12, storage_protocol = 11;
INSERT INTO migration_history(version, applied_at) VALUES (12, 0);
";

// Register the Reflow operation kind after the attachment numbering schema.
pub(in crate::adapters::sqlite) const MIGRATION_15: &str = r"
UPDATE schema_meta SET schema_version = 15, storage_protocol = 14;
INSERT INTO migration_history(version, applied_at) VALUES (15, 0);
";

// Register session-administration request receipts, including creation
// receipts that must survive a permanent prune, so an older writer cannot
// discard them.
pub(in crate::adapters::sqlite) const MIGRATION_21: &str = r"
UPDATE schema_meta SET schema_version = 21, storage_protocol = 20;
INSERT INTO migration_history(version, applied_at) VALUES (21, 0);
";
