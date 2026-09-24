//! Closed inventory of stable JSON error codes and their process contract.
//!
//! Every `error.code` emitted by the CLI is one variant here. The variant owns
//! its exit status, retry guidance, and `details` shape, and the reference
//! documentation table is rendered from this inventory by a contract test.

/// Whether repeating a failed request can succeed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Retry {
    /// The identical request fails again; change its arguments or input.
    No,
    /// The identical request may succeed after the reported state changes.
    AfterChange,
    /// The identical request may succeed once contention or a transient fault clears.
    Yes,
    /// Retry only with the same operation identity so completion can be matched.
    SameIdentity,
}

impl Retry {
    /// Stable machine-readable spelling published by `capabilities`.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::AfterChange => "after_change",
            Self::Yes => "yes",
            Self::SameIdentity => "same_identity",
        }
    }

    #[cfg(test)]
    const fn label(self) -> &'static str {
        match self {
            Self::No => "No",
            Self::AfterChange => "After change",
            Self::Yes => "Yes",
            Self::SameIdentity => "Same identity",
        }
    }
}

macro_rules! error_codes {
    ($($variant:ident => ($code:literal, $exit:literal, $retry:ident, $details:literal $(,)?),)+) => {
        /// Stable machine-readable CLI error code.
        #[derive(Clone, Copy, Eq, PartialEq)]
        pub(super) enum ErrorCode {
            $(
                #[doc = concat!("`", $code, "`")]
                $variant,
            )+
        }

        impl ErrorCode {
            /// Every code in documentation order.
            pub(super) const ALL: &[Self] = &[$(Self::$variant,)+];

            /// Stable spelling used as `error.code`.
            pub(super) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)+
                }
            }

            /// Process exit status paired with this code.
            pub(super) const fn exit(self) -> u8 {
                match self {
                    $(Self::$variant => $exit,)+
                }
            }

            /// Retry guidance for automation.
            pub(super) const fn retry(self) -> Retry {
                match self {
                    $(Self::$variant => Retry::$retry,)+
                }
            }

            /// Documented shape of `error.details`.
            #[cfg(test)]
            pub(super) const fn details(self) -> &'static str {
                match self {
                    $(Self::$variant => $details,)+
                }
            }
        }
    };
}

error_codes! {
    InvalidArguments => ("invalid_arguments", 2, No, "`{}`"),
    InvalidInput => ("invalid_input", 2, No, "`{}`"),
    InvalidIdentifier => ("invalid_identifier", 2, No, "`{}`"),
    ConfigInvalid => ("config_invalid", 2, No, "`{}`"),
    InvalidShortcutContext => ("invalid_shortcut_context", 2, No, "`{}`"),
    UnsafeStatePath => ("unsafe_state_path", 2, No, "`{}`"),
    SessionNotFound => ("session_not_found", 3, AfterChange, "`{}`"),
    ThoughtNotFound => ("thought_not_found", 3, AfterChange, "`{}`"),
    NotFound => ("not_found", 3, AfterChange, "`{}`"),
    CursorNotFound => ("cursor_not_found", 3, AfterChange, "`{}`"),
    AmbiguousSession => ("ambiguous_session", 4, AfterChange, "`{\"matches\": [session_id]}`"),
    SessionBusy => (
        "session_busy",
        5,
        Yes,
        "`{}`, or `{\"session_id\", \"holder\"}` when the active owner is known"
    ),
    SchemaBusy => ("schema_busy", 5, Yes, "`{}`"),
    StorageBusy => ("storage_busy", 5, Yes, "`{}`"),
    Unsupported => ("unsupported", 6, No, "`{}`"),
    ProtocolMismatch => ("protocol_mismatch", 6, No, "`{}`"),
    SessionTrashed => ("session_trashed", 7, AfterChange, "`{}`"),
    SessionNotTrashed => ("session_not_trashed", 7, AfterChange, "`{}`"),
    SessionNameConflict => (
        "session_name_conflict",
        7,
        AfterChange,
        "`{\"name\", \"sessions\": [{\"id\", \"origin_cwd\"}]}`"
    ),
    HistoryUnavailable => ("history_unavailable", 7, AfterChange, "`{}`"),
    IdempotencyConflict => ("idempotency_conflict", 7, No, "`{}`"),
    NoChange => ("no_change", 7, No, "`{}`"),
    ContentConflict => ("content_conflict", 7, AfterChange, "`{}`"),
    ThoughtLocked => ("thought_locked", 7, AfterChange, "`{}`"),
    InvalidState => ("invalid_state", 7, AfterChange, "`{}`"),
    InvariantViolation => ("invariant_violation", 7, No, "`{}`"),
    Conflict => ("conflict", 7, AfterChange, "`{}`"),
    MutationRejected => ("mutation_rejected", 7, AfterChange, "`{}`"),
    OperationIndeterminate => (
        "operation_indeterminate",
        8,
        SameIdentity,
        "`{\"session_id\", \"holder\"}`"
    ),
    StorageFailed => ("storage_failed", 1, AfterChange, "`{}`"),
    StorageFull => ("storage_full", 1, AfterChange, "`{}`"),
    DiskFull => ("disk_full", 1, AfterChange, "`{}`"),
    RecoveryCapacity => ("recovery_capacity", 1, AfterChange, "`{}`"),
    RuntimeFailed => ("runtime_failed", 1, AfterChange, "`{}`"),
    RuntimeMetadataInvalid => ("runtime_metadata_invalid", 1, AfterChange, "`{}`"),
    TerminalFailed => ("terminal_failed", 1, AfterChange, "`{}`"),
    TerminalWorkerFailed => ("terminal_worker_failed", 1, AfterChange, "`{}`"),
    TerminalCleanupFailed => ("terminal_cleanup_failed", 1, AfterChange, "`{}`"),
    ControlFailed => ("control_failed", 1, AfterChange, "`{}`"),
    OutputFailed => ("output_failed", 1, AfterChange, "`{}`"),
    EnvironmentFailed => ("environment_failed", 1, AfterChange, "`{}`"),
    DiagnosticsFailed => ("diagnostics_failed", 1, AfterChange, "`{}`"),
    DoctorFailed => ("doctor_failed", 1, AfterChange, "The complete `doctor` report"),
    InstallationFailed => ("installation_failed", 1, AfterChange, "`{}`"),
    InstallationUnverified => ("installation_unverified", 1, AfterChange, "`{}`"),
    InstalledVersionInvalid => ("installed_version_invalid", 1, No, "`{}`"),
    InvalidBuildVersion => ("invalid_build_version", 1, No, "`{}`"),
    ObsoleteExecutable => (
        "obsolete_executable",
        1,
        No,
        "`{\"current_version\", \"observed_installed_version\"}`"
    ),
    UpdateConvergenceActive => ("update_convergence_active", 1, Yes, "`{}`"),
    ExternalUpgradePending => (
        "external_upgrade_pending",
        1,
        AfterChange,
        "`{\"current_version\", \"pending_target_version\", \"sessions\": [{\"session_id\", \"previous_version\"}]}`"
    ),
    ExternalUpgradeBlocked => ("external_upgrade_blocked", 1, AfterChange, "`{\"blockers\": [blocker]}`"),
    ExternalUpgradeCapacity => (
        "external_upgrade_capacity",
        1,
        AfterChange,
        "`{\"blockers\": [blocker], \"participant_count\", \"maximum\", \"blockers_truncated\"}`"
    ),
    ExternalUpgradeIncomplete => ("external_upgrade_incomplete", 1, AfterChange, "`{\"blockers\": [blocker]}`"),
    ExternalUpgradePersistenceFailed => (
        "external_upgrade_persistence_failed",
        1,
        AfterChange,
        "`{\"blockers\": [blocker]}`"
    ),
    UpdateNetworkFailed => ("update_network_failed", 1, Yes, "`{}`"),
    UpdateResponseInvalid => ("update_response_invalid", 1, AfterChange, "`{}`"),
    UpdateResponseTooLarge => ("update_response_too_large", 1, AfterChange, "`{}`"),
    UpdateStateFailed => ("update_state_failed", 1, AfterChange, "`{}`"),
    UpdateCoordinationFailed => ("update_coordination_failed", 1, AfterChange, "`{}`"),
    UpdateInstallationFailed => ("update_installation_failed", 1, AfterChange, "`{}`"),
}

impl std::fmt::Debug for ErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Render the documented Markdown table from the closed inventory.
#[cfg(test)]
pub(super) fn reference_table() -> String {
    use std::fmt::Write as _;

    let mut table = String::from("| Code | Exit | Retry | `details` |\n|---|---|---|---|\n");
    for code in ErrorCode::ALL {
        let _infallible = writeln!(
            table,
            "| `{}` | {} | {} | {} |",
            code.as_str(),
            code.exit(),
            code.retry().label(),
            code.details()
        );
    }
    table
}

#[cfg(test)]
mod tests;
