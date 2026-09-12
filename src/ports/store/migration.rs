//! Authority policy for durable schema migration.

/// Whether this process proved it holds the exclusive schema lease.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationMode {
    /// Migrations may run after backup and integrity checks.
    Allow,
    /// Opening an older schema fails without modifying it.
    Refuse,
}
