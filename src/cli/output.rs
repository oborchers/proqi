//! Stable JSON envelopes, human diagnostics, and exit classification.

use std::{io::Write, process::ExitCode};

use serde::Serialize;
use serde_json::{Value, json};

use super::error_code::ErrorCode;
use crate::{
    adapters::terminal::TerminalError,
    application::{ApplicationError, FailureCode, SessionServiceError},
    ports::{
        runtime::RuntimeError,
        store::{StoreError, StoreFailureCode},
    },
};

pub(super) const JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub(super) struct CliError {
    code: ErrorCode,
    message: String,
    details: Value,
}

impl CliError {
    pub(super) fn arguments(message: String) -> Self {
        Self::new(ErrorCode::InvalidArguments, message)
    }

    pub(super) fn input(message: String) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }

    pub(super) fn identifier(message: String) -> Self {
        Self::new(ErrorCode::InvalidIdentifier, message)
    }

    pub(super) fn new(code: ErrorCode, message: String) -> Self {
        Self {
            code,
            message,
            details: json!({}),
        }
    }

    #[cfg(test)]
    pub(super) const fn code(&self) -> ErrorCode {
        self.code
    }

    #[cfg(test)]
    pub(super) const fn details(&self) -> &Value {
        &self.details
    }

    pub(super) fn message(&self) -> &str {
        &self.message
    }

    pub(super) fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<SessionServiceError> for CliError {
    fn from(error: SessionServiceError) -> Self {
        let message = error.to_string();
        match error {
            SessionServiceError::SessionNotFound(_) => {
                Self::new(ErrorCode::SessionNotFound, message)
            }
            SessionServiceError::AmbiguousSession { matches, .. } => {
                let ids: Vec<_> = matches.iter().map(ToString::to_string).collect();
                Self::new(ErrorCode::AmbiguousSession, message)
                    .with_details(json!({ "matches": ids }))
            }
            SessionServiceError::SessionNameConflict { name, sessions } => {
                let sessions: Vec<_> = sessions
                    .iter()
                    .map(|session| json!({ "id": session.id, "origin_cwd": session.origin_cwd }))
                    .collect();
                Self::new(ErrorCode::SessionNameConflict, message)
                    .with_details(json!({ "name": name, "sessions": sessions }))
            }
            SessionServiceError::SessionTrashed(_) => Self::new(ErrorCode::SessionTrashed, message),
            SessionServiceError::SessionNotTrashed(_) => {
                Self::new(ErrorCode::SessionNotTrashed, message)
            }
            SessionServiceError::NoBrowserHistory { .. } => {
                Self::new(ErrorCode::HistoryUnavailable, message)
            }
            SessionServiceError::IdempotencyConflict => {
                Self::new(ErrorCode::IdempotencyConflict, message)
            }
            SessionServiceError::NoDurableMutation => Self::new(ErrorCode::NoChange, message),
            SessionServiceError::InvalidIdentifier { .. } => {
                Self::new(ErrorCode::InvalidIdentifier, message)
            }
            SessionServiceError::Runtime(runtime) => runtime.into(),
            SessionServiceError::Store(store) => store.into(),
            SessionServiceError::Application(application) => application.into(),
            SessionServiceError::Domain(_) | SessionServiceError::InvalidDirectory(_) => {
                Self::new(ErrorCode::InvalidInput, message)
            }
        }
    }
}

impl From<RuntimeError> for CliError {
    fn from(error: RuntimeError) -> Self {
        let message = error.to_string();
        match error {
            RuntimeError::SessionBusy { session_id, holder } => Self::new(
                ErrorCode::SessionBusy,
                format!("session is active: {session_id}"),
            )
            .with_details(json!({ "session_id": session_id, "holder": holder })),
            RuntimeError::SchemaBusy => Self::new(ErrorCode::SchemaBusy, message),
            RuntimeError::MalformedMetadata(_) => {
                Self::new(ErrorCode::RuntimeMetadataInvalid, message)
            }
            RuntimeError::Io(_) | RuntimeError::Invalid(_) => {
                Self::new(ErrorCode::RuntimeFailed, message)
            }
        }
    }
}

impl From<StoreError> for CliError {
    fn from(error: StoreError) -> Self {
        Self::storage(error.failure_code(), error.to_string())
    }
}

impl CliError {
    pub(super) fn storage(code: StoreFailureCode, message: String) -> Self {
        let code = match code {
            StoreFailureCode::Busy => ErrorCode::StorageBusy,
            StoreFailureCode::NotFound => ErrorCode::NotFound,
            StoreFailureCode::Conflict => ErrorCode::Conflict,
            StoreFailureCode::Unsupported => ErrorCode::Unsupported,
            StoreFailureCode::DiskFull => ErrorCode::DiskFull,
            StoreFailureCode::RecoveryCapacity => ErrorCode::RecoveryCapacity,
            StoreFailureCode::Failed => ErrorCode::StorageFailed,
        };
        Self::new(code, message)
    }
}

impl From<ApplicationError> for CliError {
    fn from(error: ApplicationError) -> Self {
        let code = match error.code() {
            FailureCode::ThoughtNotFound => ErrorCode::ThoughtNotFound,
            FailureCode::ContentConflict => ErrorCode::ContentConflict,
            FailureCode::ThoughtLocked => ErrorCode::ThoughtLocked,
            FailureCode::InvalidState => ErrorCode::InvalidState,
            FailureCode::ClipboardFailed => ErrorCode::ClipboardFailed,
            FailureCode::ClipboardMetadataUnsupported => ErrorCode::ClipboardMetadataUnsupported,
            FailureCode::StorageFailed => ErrorCode::StorageFailed,
            FailureCode::RecoveryCapacity => ErrorCode::RecoveryCapacity,
            FailureCode::InvariantViolation => ErrorCode::InvariantViolation,
        };
        Self::new(code, error.to_string())
    }
}

impl From<TerminalError> for CliError {
    fn from(error: TerminalError) -> Self {
        match error {
            TerminalError::Store(store) => store.into(),
            TerminalError::Io(message) => Self::new(ErrorCode::TerminalFailed, message),
            TerminalError::Worker(message) => {
                Self::new(ErrorCode::TerminalWorkerFailed, message.to_owned())
            }
            TerminalError::Cleanup(message) => Self::new(ErrorCode::TerminalCleanupFailed, message),
            TerminalError::Config(message) => Self::new(ErrorCode::ConfigInvalid, message),
            TerminalError::Control(error) => Self::new(ErrorCode::ControlFailed, error.to_string()),
            TerminalError::Runtime(error) => error.into(),
        }
    }
}

pub(super) fn render_success<T: Serialize>(value: &T, human: &str, json_output: bool) -> ExitCode {
    crate::adapters::diagnostics::record(crate::adapters::diagnostics::SafeEvent::CommandSucceeded);
    let result = if json_output {
        write_json(&json!({
            "schema_version": JSON_SCHEMA_VERSION,
            "ok": true,
            "data": value,
        }))
    } else {
        writeln!(std::io::stdout().lock(), "{human}").map_err(|error| error.to_string())
    };
    if let Err(error) = result {
        return render_error(&CliError::new(ErrorCode::OutputFailed, error), json_output);
    }
    ExitCode::SUCCESS
}

pub(super) fn render_error(error: &CliError, json_output: bool) -> ExitCode {
    crate::adapters::diagnostics::record(crate::adapters::diagnostics::SafeEvent::CommandFailed {
        code: error.code.as_str(),
        exit: error.code.exit(),
    });
    if json_output {
        let payload = json!({
            "schema_version": JSON_SCHEMA_VERSION,
            "ok": false,
            "error": {
                "code": error.code.as_str(),
                "message": error.message,
                "details": error.details,
            }
        });
        let _result = write_json(&payload);
    } else {
        let _result = writeln!(std::io::stderr().lock(), "proqi: {}", error.message);
    }
    ExitCode::from(error.code.exit())
}

fn write_json(value: &Value) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())
}
