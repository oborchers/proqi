//! Session listing, named creation, and administration commands.

mod administration;
mod listing;
mod named;

use serde_json::json;

use crate::{
    adapters::terminal, cli::error_code::ErrorCode, domain::SessionId, ports::environment::Clock,
};

use super::{
    super::{args::SessionCommand, output::CliError, runtime::RuntimeContext},
    Outcome, session_service,
};

pub(super) use listing::list_sessions;
pub(super) use named::existing_directory;

pub(super) fn browse_for_session(
    context: &mut RuntimeContext,
    settings: &terminal::LoadedSettings,
) -> Result<
    Option<crate::application::LeasedSession<crate::adapters::runtime::FileSessionLease>>,
    CliError,
> {
    loop {
        let items = listing::browser_items(context)?;
        let now = context.clock.now();
        let history = session_service(context)?.browser_history_status()?;
        match terminal::pick_session(items, now, settings, history)? {
            crate::ui::BrowserAction::Open(id) => {
                return session_service(context)?
                    .resume(id)
                    .map(Some)
                    .map_err(Into::into);
            }
            crate::ui::BrowserAction::Rename { session_id, name } => {
                session_service(context)?.rename_session(session_id, name.as_deref(), None)?;
            }
            crate::ui::BrowserAction::Trash(id) => {
                session_service(context)?.trash_session(id, None)?;
            }
            crate::ui::BrowserAction::Restore(id) => {
                session_service(context)?.restore_session(id, None)?;
            }
            crate::ui::BrowserAction::History { undo, target } => {
                let mut service = session_service(context)?;
                service.move_presented_browser_history(target, undo)?;
            }
            crate::ui::BrowserAction::Cancel => return Ok(None),
            crate::ui::BrowserAction::Continue => {
                return Err(CliError::new(
                    ErrorCode::TerminalFailed,
                    "session browser returned an incomplete action".to_owned(),
                ));
            }
        }
    }
}

pub(super) fn execute_sessions(
    context: &mut RuntimeContext,
    command: Option<SessionCommand>,
) -> Result<Outcome, CliError> {
    match command.unwrap_or(SessionCommand::List {
        query: None,
        all: false,
        page: super::super::args::PageArgs::default(),
    }) {
        SessionCommand::List { query, all, page } => list_sessions(context, query, all, &page),
        SessionCommand::Ensure { name, cwd } => named::ensure(context, name, &cwd),
        SessionCommand::Create {
            name,
            cwd,
            operation_id,
        } => named::create(context, name, cwd.as_deref(), operation_id.as_deref()),
        SessionCommand::Rename {
            session,
            name,
            clear,
            operation_id,
        } => administration::rename(
            context,
            &session,
            if clear { None } else { name.as_deref() },
            operation_id.as_deref(),
        ),
        SessionCommand::Trash {
            session,
            operation_id,
        } => administration::manage(
            context,
            &session,
            administration::Management::Trash,
            operation_id.as_deref(),
        ),
        SessionCommand::Restore {
            session,
            operation_id,
        } => administration::manage(
            context,
            &session,
            administration::Management::Restore,
            operation_id.as_deref(),
        ),
        SessionCommand::Undo { operation_id } => {
            administration::move_history(context, true, operation_id.as_deref())
        }
        SessionCommand::Redo { operation_id } => {
            administration::move_history(context, false, operation_id.as_deref())
        }
        SessionCommand::Prune {
            session,
            yes,
            operation_id,
        } => {
            if !yes {
                return Err(CliError::arguments(
                    "permanent pruning requires --yes".to_owned(),
                ));
            }
            administration::manage(
                context,
                &session,
                administration::Management::Prune,
                operation_id.as_deref(),
            )
        }
    }
}

pub(super) fn cancelled_browser() -> Outcome {
    Outcome {
        data: json!({ "cancelled": true }),
        human: "No session opened".to_owned(),
    }
}

pub(super) fn opened_session(id: SessionId) -> Outcome {
    let resume = resume_command(id);
    Outcome {
        data: json!({ "session_id": id, "resume_command": resume }),
        human: format!("Session {id}\nResume later: {resume}"),
    }
}

/// Exact command that resumes one session in an interactive terminal.
pub(super) fn resume_command(id: SessionId) -> String {
    format!("proqi -r {id}")
}
