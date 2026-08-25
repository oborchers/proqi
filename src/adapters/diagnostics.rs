//! Private, bounded, structured, content-redacted diagnostics.

mod collect;
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
            direction = direction_name(direction),
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

const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
}
