//! `proqi herdr toggle` and `proqi herdr capture`: the Herdr plugin actions that act
//! on the invoking tab's companion session.

use std::path::Path;

use serde_json::json;

use crate::{
    adapters::{
        herdr::{FileCompanionRecords, HerdrCompanionHost, HerdrPluginEnvironment},
        process::SystemProcessRunner,
    },
    application::{
        CompanionCaptureError, CompanionCaptureOutcome, CompanionToggleError,
        CompanionToggleOutcome, SessionServiceError, capture_to_companion, toggle_companion,
    },
    domain::{SessionId, ThoughtId},
    ports::{
        companion::{CompanionError, CompanionHost as _, CompanionSessionState, CompanionSessions},
        runtime::RuntimeCoordinator as _,
    },
};

use super::{CliError, Outcome, forwarding, session_service, sessions::existing_directory};
use crate::cli::{args::Cli, error_code::ErrorCode, runtime::RuntimeContext};

/// Run the toggle inside the environment Herdr gives a plugin action.
pub(super) fn toggle(cli: &Cli) -> Result<Outcome, CliError> {
    let environment = HerdrPluginEnvironment::detect().map_err(|error| companion_error(&error))?;
    let mut host = HerdrCompanionHost::new(environment.clone(), SystemProcessRunner::default());
    let prepared = FileCompanionRecords::acquire(environment.state_dir())
        .map_err(|error| companion_error(&error))
        .and_then(|records| Ok((records, super::runtime_open::open(cli)?)));
    let (mut records, mut context) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            host.notify(error.message());
            return Err(error);
        }
    };
    let mut sessions = CliCompanionSessions {
        context: &mut context,
    };
    toggle_companion(&mut host, &mut records, &mut sessions)
        .map(outcome)
        .map_err(toggle_error)
}

/// Capture the selection or clipboard text into the tab's session without opening Proqi.
pub(super) fn capture(cli: &Cli) -> Result<Outcome, CliError> {
    let environment = HerdrPluginEnvironment::detect().map_err(|error| companion_error(&error))?;
    let mut host = HerdrCompanionHost::new(environment.clone(), SystemProcessRunner::default());
    let prepared = FileCompanionRecords::acquire(environment.state_dir())
        .map_err(|error| companion_error(&error))
        .and_then(|records| Ok((records, super::runtime_open::open(cli)?)));
    let (mut records, mut context) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            host.notify(&format!(
                "{}: {}",
                crate::application::NOTHING_CAPTURED,
                error.message()
            ));
            return Err(error);
        }
    };
    let mut clipboard = context.capture_clipboard();
    let mut sessions = CliCompanionSessions {
        context: &mut context,
    };
    capture_to_companion(&mut host, &mut records, &mut sessions, &mut *clipboard)
        .map(|outcome| captured(&outcome))
        .map_err(|error| match error {
            CompanionCaptureError::Capture(error) => super::capture::capture_error(&error),
            CompanionCaptureError::Host(error) => companion_error(&error),
            CompanionCaptureError::Session(error) => error,
        })
}

fn captured(outcome: &CompanionCaptureOutcome) -> Outcome {
    Outcome {
        human: format!(
            "Captured {} characters from the {} into {} as {}",
            outcome.characters,
            outcome.source.as_str(),
            outcome.session_id,
            outcome.thought_id
        ),
        data: json!({
            "action": "captured",
            "tab_id": outcome.tab_id,
            "session_id": outcome.session_id,
            "thought_id": outcome.thought_id,
            "source": outcome.source.as_str(),
            "characters": outcome.characters,
            "bytes": outcome.bytes,
        }),
    }
}

struct CliCompanionSessions<'context> {
    context: &'context mut RuntimeContext,
}

impl CompanionSessions for CliCompanionSessions<'_> {
    type Error = CliError;

    fn ensure(&mut self, name: &str, cwd: &Path) -> Result<SessionId, CliError> {
        let cwd = existing_directory(cwd)?;
        Ok(session_service(self.context)?
            .ensure_named_session(name.to_owned(), cwd)?
            .session_id)
    }

    fn state(&mut self, session_id: SessionId) -> Result<CompanionSessionState, CliError> {
        let snapshot = match session_service(self.context)?.inspect_session(session_id) {
            Ok(snapshot) => snapshot,
            Err(SessionServiceError::SessionNotFound(_)) => {
                return Ok(CompanionSessionState::Unavailable);
            }
            Err(error) => return Err(error.into()),
        };
        if snapshot.board.session.deleted_at.is_some() {
            return Ok(CompanionSessionState::Unavailable);
        }
        let runtime = self.context.coordinator.scan_runtime()?;
        Ok(
            if runtime
                .active
                .iter()
                .any(|instance| instance.session_id == session_id)
            {
                CompanionSessionState::Active
            } else {
                CompanionSessionState::Resumable
            },
        )
    }

    fn name(&mut self, session_id: SessionId) -> Result<Option<String>, CliError> {
        Ok(session_service(self.context)?
            .inspect_session(session_id)?
            .board
            .session
            .name)
    }

    fn flush(&mut self, session_id: SessionId) -> Result<(), CliError> {
        forwarding::sync_confirmed(self.context, session_id)
    }

    fn capture(&mut self, session_id: SessionId, text: &str) -> Result<ThoughtId, CliError> {
        super::capture::store(self.context, session_id, text, None)
            .map(|mutation| mutation.thought_id)
    }
}

fn outcome(outcome: CompanionToggleOutcome) -> Outcome {
    match outcome {
        CompanionToggleOutcome::Opened {
            tab_id,
            pane_id,
            session_id,
            replaced_pane_id,
        } => Outcome {
            human: replaced_pane_id.as_ref().map_or_else(
                || format!("Opened Proqi {session_id} in {pane_id}"),
                |dead| format!("Replaced {dead} with Proqi {session_id} in {pane_id}"),
            ),
            data: json!({
                "action": "opened",
                "tab_id": tab_id,
                "pane_id": pane_id,
                "session_id": session_id,
                "replaced_pane_id": replaced_pane_id,
            }),
        },
        CompanionToggleOutcome::Focused { pane_id } => Outcome {
            human: format!("Focused Proqi in {pane_id}"),
            data: json!({ "action": "focused", "pane_id": pane_id }),
        },
        CompanionToggleOutcome::Returned { pane_id } => Outcome {
            human: format!("Returned focus to {pane_id}"),
            data: json!({ "action": "returned", "pane_id": pane_id }),
        },
        CompanionToggleOutcome::Closed {
            pane_id,
            session_id,
        } => Outcome {
            human: format!("Closed Proqi in {pane_id}; resume with proqi -r {session_id}"),
            data: json!({ "action": "closed", "pane_id": pane_id, "session_id": session_id }),
        },
    }
}

fn toggle_error(error: CompanionToggleError<CliError>) -> CliError {
    let message = error.to_string();
    match error {
        CompanionToggleError::Host(error) => companion_error(&error),
        CompanionToggleError::Session(error) => error,
        CompanionToggleError::SessionActive { session_id, name } => {
            CliError::new(ErrorCode::CompanionSessionActive, message)
                .with_details(json!({ "session_id": session_id, "name": name }))
        }
        CompanionToggleError::NoReturnTarget => CliError::new(ErrorCode::InvalidState, message),
        CompanionToggleError::Unclassified { .. } => CliError::new(ErrorCode::HerdrFailed, message),
    }
}

fn companion_error(error: &CompanionError) -> CliError {
    let message = error.to_string();
    match error {
        CompanionError::Unavailable(_) => CliError::new(ErrorCode::Unsupported, message),
        CompanionError::Host(_) => CliError::new(ErrorCode::HerdrFailed, message),
        CompanionError::State(_) => CliError::new(ErrorCode::PluginStateFailed, message),
    }
}
