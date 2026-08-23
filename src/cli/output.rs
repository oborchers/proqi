//! Stable JSON envelopes, human diagnostics, and exit classification.

use std::{io::Write, process::ExitCode};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    adapters::terminal::TerminalError,
    application::{ApplicationError, SessionServiceError},
    ports::{runtime::RuntimeError, store::StoreError},
};

pub(super) const JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub(super) struct CliError {
    code: &'static str,
    message: String,
    exit: u8,
    details: Value,
}

impl CliError {
    pub(super) fn arguments(message: String) -> Self {
        Self::new("invalid_arguments", message, 2)
    }

    pub(super) fn input(message: String) -> Self {
        Self::new("invalid_input", message, 2)
    }

    pub(super) fn identifier(message: String) -> Self {
        Self::new("invalid_identifier", message, 2)
    }

    pub(super) fn unsupported(message: String) -> Self {
        Self::new("unsupported", message, 6)
    }

    pub(super) fn new(code: &'static str, message: String, exit: u8) -> Self {
        Self {
            code,
            message,
            exit,
            details: json!({}),
        }
    }

    pub(super) fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

impl From<SessionServiceError> for CliError {
    fn from(error: SessionServiceError) -> Self {
        let message = error.to_string();
        match error {
            SessionServiceError::SessionNotFound(_) => Self::new("session_not_found", message, 3),
            SessionServiceError::AmbiguousSession { matches, .. } => {
                let ids: Vec<_> = matches.iter().map(ToString::to_string).collect();
                Self::new("ambiguous_session", message, 4).with_details(json!({ "matches": ids }))
            }
            SessionServiceError::SessionTrashed(_) => Self::new("session_trashed", message, 7),
            SessionServiceError::IdempotencyConflict => {
                Self::new("idempotency_conflict", message, 7)
            }
            SessionServiceError::NoDurableMutation => Self::new("no_change", message, 7),
            SessionServiceError::InvalidIdentifier { .. } => {
                Self::new("invalid_identifier", message, 2)
            }
            SessionServiceError::Runtime(runtime) => runtime.into(),
            SessionServiceError::Store(store) => store.into(),
            SessionServiceError::Application(application) => application.into(),
            SessionServiceError::Domain(_) | SessionServiceError::InvalidDirectory(_) => {
                Self::new("invalid_input", message, 2)
            }
        }
    }
}

impl From<RuntimeError> for CliError {
    fn from(error: RuntimeError) -> Self {
        let message = error.to_string();
        match error {
            RuntimeError::SessionBusy { session_id, holder } => Self::new(
                "session_busy",
                format!("session is active: {session_id}"),
                5,
            )
            .with_details(json!({ "session_id": session_id, "holder": holder })),
            RuntimeError::SchemaBusy => Self::new("schema_busy", message, 5),
            RuntimeError::MalformedMetadata(_) => Self::new("runtime_metadata_invalid", message, 1),
            RuntimeError::Io(_) | RuntimeError::Invalid(_) => {
                Self::new("runtime_failed", message, 1)
            }
        }
    }
}

impl From<StoreError> for CliError {
    fn from(error: StoreError) -> Self {
        let (code, exit) = match &error {
            StoreError::Busy => ("storage_busy", 5),
            StoreError::NotFound(_) => ("not_found", 3),
            StoreError::Conflict(_) => ("conflict", 7),
            StoreError::UnsupportedSchema { .. }
            | StoreError::UnsupportedStorageProtocol { .. }
            | StoreError::MigrationRequired { .. } => ("unsupported", 6),
            StoreError::DiskFull => ("disk_full", 1),
            _ => ("storage_failed", 1),
        };
        Self::new(code, error.to_string(), exit)
    }
}

impl From<ApplicationError> for CliError {
    fn from(error: ApplicationError) -> Self {
        let code = error.code().as_str();
        let exit = if matches!(error, ApplicationError::ThoughtNotFound(_)) {
            3
        } else {
            7
        };
        Self::new(code, error.to_string(), exit)
    }
}

impl From<TerminalError> for CliError {
    fn from(error: TerminalError) -> Self {
        match error {
            TerminalError::Store(store) => store.into(),
            TerminalError::Io(message) => Self::new("terminal_failed", message, 1),
            TerminalError::Worker(message) => {
                Self::new("terminal_worker_failed", message.to_owned(), 1)
            }
        }
    }
}

pub(super) fn render_success<T: Serialize>(value: &T, human: &str, json_output: bool) -> ExitCode {
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
        return render_error(&CliError::new("output_failed", error, 1), json_output);
    }
    ExitCode::SUCCESS
}

pub(super) fn render_error(error: &CliError, json_output: bool) -> ExitCode {
    if json_output {
        let payload = json!({
            "schema_version": JSON_SCHEMA_VERSION,
            "ok": false,
            "error": {
                "code": error.code,
                "message": error.message,
                "details": error.details,
            }
        });
        let _result = write_json(&payload);
    } else {
        let _result = writeln!(std::io::stderr().lock(), "proqi: {}", error.message);
    }
    ExitCode::from(error.exit)
}

fn write_json(value: &Value) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())
}
