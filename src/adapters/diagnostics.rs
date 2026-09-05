//! Private, bounded, structured, content-redacted diagnostics.

mod collect;
mod invocation;
mod writer;

use std::{path::Path, sync::OnceLock};

use thiserror::Error;
use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::domain::{Direction, InstanceId, SubmissionId};

pub use collect::{DiagnosticBundle, collect_bundle};
use writer::RotatingMakeWriter;

static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Diagnostics initialization or collection failure.
#[derive(Debug, Error)]
pub enum DiagnosticsError {
    /// A private diagnostics directory or file could not be prepared.
    #[error("diagnostics I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// Structured data could not be encoded or decoded.
    #[error("diagnostics serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Another tracing subscriber prevented Proqi from installing its file sink.
    #[error("diagnostics subscriber could not be installed: {0}")]
    Subscriber(String),
}

/// Typed events whose fields are safe for the local diagnostic journal.
#[derive(Clone, Copy, Debug)]
pub enum SafeEvent<'a> {
    /// Diagnostics became available for this instance.
    Initialized {
        /// Running process identity.
        instance_id: InstanceId,
    },
    /// Runtime composition began.
    RuntimeOpening {
        /// Running process identity.
        instance_id: InstanceId,
    },
    /// SQLite schema opening crossed one reviewed lifecycle boundary.
    SchemaLifecycle {
        /// Closed, content-free schema stage.
        stage: SchemaLifecycleStage,
    },
    /// The restored board and optional control endpoint are ready for use.
    RuntimeReady {
        /// Whether verified owner control was published at this boundary.
        control_ready: bool,
    },
    /// The exact initiating replacement reached the final convergence boundary.
    UpdateConverged,
    /// The initiating replacement could not prove the final convergence boundary.
    UpdateFinalizationFailed {
        /// Closed, content-free finalization failure.
        code: UpdateFinalizationFailure,
    },
    /// Terminal shutdown began.
    ShutdownStarted,
    /// Terminal shutdown finished.
    ShutdownFinished {
        /// Number of cleanup stages that returned an error.
        cleanup_failures: usize,
        /// Total bounded shutdown time in milliseconds.
        elapsed_ms: u64,
    },
    /// One named runtime cleanup stage failed.
    CleanupFailed {
        /// Stable, content-free cleanup stage name.
        stage: &'a str,
    },
    /// A runtime thread panicked without recording its payload.
    RuntimePanicked {
        /// Stable lane name or `owner`.
        role: &'a str,
    },
    /// One CLI command succeeded.
    CommandSucceeded,
    /// One CLI command failed with a stable code.
    CommandFailed {
        /// Stable machine-readable failure code.
        code: &'a str,
        /// Process exit status.
        exit: u8,
    },
    /// One content-free update lookup failed.
    UpdateCheckFailed {
        /// `startup` or `manual`.
        mode: &'a str,
        /// Stable coarse failure code.
        code: &'a str,
    },
    /// One attachment could not be verified, without recording its path or owner.
    AttachmentInaccessible {
        /// Typed content-free adapter failure.
        reason: &'a str,
    },
    /// One invocation source returned retained but incomplete results.
    InvocationIncomplete {
        /// Typed reason containing only stable codes and aggregate counts.
        reason: &'a crate::ports::invocation::InvocationIncompleteReason,
    },
    /// One content-redacted submission transition occurred.
    Submission {
        /// Proqi-owned submission identity.
        submission_id: SubmissionId,
        /// Durable submission state.
        state: &'a str,
        /// Target direction without pane or workspace details.
        direction: Direction,
        /// Integration provider name.
        provider: &'a str,
        /// Optional stable result code.
        outcome: Option<&'a str>,
    },
    /// One submission state changed without repeating target metadata.
    SubmissionState {
        /// Proqi-owned submission identity.
        submission_id: SubmissionId,
        /// Durable submission state.
        state: &'a str,
        /// Optional stable result code.
        outcome: Option<&'a str>,
    },
}

/// Content-free SQLite opening stages.
#[derive(Clone, Copy, Debug)]
pub enum SchemaLifecycleStage {
    /// The current executable requires a forward migration.
    MigrationRequired,
    /// This process acquired exclusive migration ownership.
    MigrationStarted,
    /// Exclusive migration and integrity validation completed.
    MigrationCompleted,
    /// Another process completed migration before this contender revalidated.
    FollowerRevalidated,
    /// The store is open under the current schema and a shared lease.
    Ready,
}

/// Content-free update finalization failures.
#[derive(Clone, Copy, Debug)]
pub enum UpdateFinalizationFailure {
    /// Owner control was not published for the restored board.
    ControlUnavailable,
    /// The private cache could not be opened or atomically updated.
    StateUnavailable,
    /// Cached target state no longer matched this replacement.
    StateMismatch,
}

impl UpdateFinalizationFailure {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ControlUnavailable => "control_unavailable",
            Self::StateUnavailable => "state_unavailable",
            Self::StateMismatch => "state_mismatch",
        }
    }
}

impl SchemaLifecycleStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::MigrationRequired => "migration_required",
            Self::MigrationStarted => "migration_started",
            Self::MigrationCompleted => "migration_completed",
            Self::FollowerRevalidated => "follower_revalidated",
            Self::Ready => "ready",
        }
    }
}

/// Install one process-wide JSONL subscriber for a typed running instance.
///
/// # Errors
///
/// Returns a typed error when private files, rotation, retention, or subscriber
/// installation fails.
pub fn initialize(data_dir: &Path, instance_id: InstanceId) -> Result<(), DiagnosticsError> {
    if INITIALIZED.get().is_some() {
        return Ok(());
    }
    let writer = RotatingMakeWriter::open(&data_dir.join("diagnostics"), instance_id)?;
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_ansi(false)
        .with_max_level(Level::INFO)
        .with_target(true)
        .with_writer(writer)
        .finish()
        .try_init()
        .map_err(|error| DiagnosticsError::Subscriber(error.to_string()))?;
    INITIALIZED
        .set(())
        .map_err(|()| DiagnosticsError::Subscriber("initialization raced".to_owned()))?;
    record(SafeEvent::Initialized { instance_id });
    Ok(())
}

/// Record one typed event without accepting arbitrary user data.
pub fn record(event: SafeEvent<'_>) {
    match event {
        SafeEvent::Initialized { instance_id } => tracing::info!(
            event = "diagnostics_initialized",
            version = env!("CARGO_PKG_VERSION"),
            instance_id = %instance_id
        ),
        SafeEvent::RuntimeOpening { instance_id } => {
            tracing::info!(event = "runtime_opening", instance_id = %instance_id);
        }
        SafeEvent::SchemaLifecycle { stage } => {
            tracing::info!(event = "schema_lifecycle", stage = stage.as_str());
        }
        SafeEvent::RuntimeReady { control_ready } => {
            tracing::info!(event = "runtime_ready", control_ready);
        }
        SafeEvent::UpdateConverged => {
            tracing::info!(
                event = "update_convergence",
                complete = true,
                stage = "board_ready"
            );
        }
        SafeEvent::UpdateFinalizationFailed { code } => tracing::warn!(
            event = "update_failure",
            stage = "finalization",
            code = code.as_str()
        ),
        SafeEvent::ShutdownStarted => tracing::info!(event = "shutdown_started"),
        SafeEvent::ShutdownFinished {
            cleanup_failures,
            elapsed_ms,
        } => {
            tracing::info!(event = "shutdown_finished", cleanup_failures, elapsed_ms);
        }
        SafeEvent::CleanupFailed { stage } => {
            tracing::error!(event = "cleanup_failed", stage);
        }
        SafeEvent::RuntimePanicked { role } => {
            tracing::error!(event = "runtime_panicked", role);
        }
        SafeEvent::CommandSucceeded => tracing::info!(event = "command_succeeded"),
        SafeEvent::CommandFailed { code, exit } => {
            tracing::error!(event = "command_failed", code, exit);
        }
        SafeEvent::UpdateCheckFailed { mode, code } => {
            tracing::warn!(event = "update_check_failed", mode, code);
        }
        SafeEvent::AttachmentInaccessible { reason } => {
            tracing::warn!(event = "attachment_inaccessible", reason);
        }
        SafeEvent::InvocationIncomplete { reason } => invocation::record(reason),
        SafeEvent::Submission {
            submission_id,
            state,
            direction,
            provider,
            outcome,
        } => tracing::info!(
            event = "submission_transition",
            submission_id = %submission_id,
            state,
            direction = direction.as_str(),
            provider,
            outcome
        ),
        SafeEvent::SubmissionState {
            submission_id,
            state,
            outcome,
        } => tracing::info!(
            event = "submission_transition",
            submission_id = %submission_id,
            state,
            outcome
        ),
    }
}

/// Record one content-free input lease reset without widening the public event vocabulary.
pub(crate) fn record_input_lease_reset(observer_gap_ms: u64) {
    tracing::warn!(
        event = "input_lease_reset",
        reason = "supervisor_gap",
        observer_gap_ms
    );
}

/// Record every stable reason in one incomplete invocation result.
pub fn record_invocation_completeness(
    completeness: &crate::ports::invocation::InvocationCompleteness,
) {
    for reason in completeness.reasons() {
        record(SafeEvent::InvocationIncomplete { reason });
    }
}

/// Record one complete, aggregated, content-free update attempt.
pub fn record_update_execution(execution: &crate::application::UpdateExecution) {
    tracing::info!(
        event = "update_participants",
        selected = execution.selected_participants,
        prepared = execution.prepared_participants
    );
    tracing::info!(
        event = "update_restarts",
        requested = execution.restart_requests,
        accepted = execution.restart_accepted
    );
    tracing::info!(
        event = "update_replacements",
        ready = execution.replacement_ready,
        missing = execution.replacement_missing
    );
    let complete = update_execution_complete(execution);
    if let Some((stage, code)) = update_execution_failure(execution) {
        tracing::warn!(event = "update_failure", stage, code);
    }
    tracing::info!(
        event = "update_convergence",
        complete,
        stage = "coordinator"
    );
}

fn update_execution_complete(execution: &crate::application::UpdateExecution) -> bool {
    matches!(
        execution.status,
        crate::application::UpdateExecutionStatus::Installed { .. }
    ) && execution.restart_failed.is_empty()
        && execution.replacement_missing == 0
        && execution.restart_requests == execution.restart_accepted
        && execution.convergence_state_recorded
}

/// Record a typed update error without its arbitrary adapter detail.
pub fn record_update_error(error: &crate::ports::update::UpdateError) {
    let code = match error {
        crate::ports::update::UpdateError::Network => "network",
        crate::ports::update::UpdateError::InvalidResponse => "invalid_response",
        crate::ports::update::UpdateError::ResponseTooLarge => "response_too_large",
        crate::ports::update::UpdateError::Installation(_) => "installation",
        crate::ports::update::UpdateError::State(_) => "state",
        crate::ports::update::UpdateError::Coordination(_) => "coordination",
        crate::ports::update::UpdateError::InstallerFailed => "installer_failed",
    };
    tracing::warn!(event = "update_failure", stage = "execution", code);
}

fn update_execution_failure(
    execution: &crate::application::UpdateExecution,
) -> Option<(&'static str, &'static str)> {
    match &execution.status {
        crate::application::UpdateExecutionStatus::Aborted { code, .. } => {
            Some(("preparation", safe_abort_code(code)))
        }
        crate::application::UpdateExecutionStatus::Installed { .. }
            if !execution.restart_failed.is_empty() || execution.replacement_missing > 0 =>
        {
            Some(("restart", "incomplete_convergence"))
        }
        crate::application::UpdateExecutionStatus::AlreadyInProgress
        | crate::application::UpdateExecutionStatus::Installed { .. } => None,
    }
}

fn safe_abort_code(code: &str) -> &'static str {
    match code {
        "no_compatible_participants" => "no_compatible_participants",
        "coordinator_not_registered" => "coordinator_not_registered",
        "invalid_coordinator_version" => "invalid_coordinator_version",
        "invalid_readiness_receipt" => "invalid_readiness_receipt",
        "participant_unavailable" => "participant_unavailable",
        "save_failed" => "save_failed",
        "deadline_expired" => "deadline_expired",
        _ => "participant_blocked",
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        application::{UpdateExecution, UpdateExecutionStatus},
        domain::StableVersion,
        ports::environment::IdGenerator as _,
    };

    #[test]
    fn arbitrary_participant_failure_codes_are_redacted() {
        assert_eq!(
            super::safe_abort_code("private-owner-detail"),
            "participant_blocked"
        );
        assert_eq!(super::safe_abort_code("save_failed"), "save_failed");
    }

    #[test]
    fn successful_coordinator_completion_compares_requests_with_acceptances() {
        let mut ids = FakeIdGenerator::new(1_800_000_000_000);
        let mut execution = UpdateExecution {
            operation_id: ids.request_id(),
            selected_participants: 2,
            prepared_participants: 2,
            restart_requests: 2,
            restart_accepted: 2,
            replacement_ready: 1,
            replacement_missing: 0,
            restart_failed: Vec::new(),
            convergence_state_recorded: true,
            status: UpdateExecutionStatus::Installed {
                version: StableVersion::parse("1.2.0").expect("version"),
            },
        };
        assert!(super::update_execution_complete(&execution));
        execution.restart_accepted = 1;
        assert!(!super::update_execution_complete(&execution));
    }
}
