//! Session listing and administration commands.

use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::{
    adapters::terminal,
    domain::SessionId,
    ports::{environment::Clock, runtime::RuntimeCoordinator, store::SessionHit},
    ui::{BrowserAvailability, SessionBrowserItem},
};

pub(super) fn browse_for_session(
    context: &mut RuntimeContext,
    settings: &terminal::LoadedSettings,
) -> Result<
    Option<crate::application::LeasedSession<crate::adapters::runtime::FileSessionLease>>,
    CliError,
> {
    loop {
        let items = browser_items(context)?;
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
                session_service(context)?.rename_session(session_id, name.as_deref())?;
            }
            crate::ui::BrowserAction::Trash(id) => {
                session_service(context)?.trash_session(id)?;
            }
            crate::ui::BrowserAction::Restore(id) => {
                session_service(context)?.restore_session(id)?;
            }
            crate::ui::BrowserAction::History { undo, target } => {
                let mut service = session_service(context)?;
                service.move_presented_browser_history(target, undo)?;
            }
            crate::ui::BrowserAction::Cancel => return Ok(None),
            crate::ui::BrowserAction::Continue => {
                return Err(CliError::new(
                    "terminal_failed",
                    "session browser returned an incomplete action".to_owned(),
                    1,
                ));
            }
        }
    }
}

use super::{
    super::{args::SessionCommand, output::CliError, runtime::RuntimeContext},
    Outcome, session_service,
};

pub(super) fn execute_sessions(
    context: &mut RuntimeContext,
    command: Option<SessionCommand>,
) -> Result<Outcome, CliError> {
    match command.unwrap_or(SessionCommand::List {
        query: None,
        all: false,
    }) {
        SessionCommand::List { query, all } => list_sessions(context, query, all),
        SessionCommand::Rename {
            session,
            name,
            clear,
        } => {
            let mut service = session_service(context)?;
            let id = service.resolve_session(&session, true)?;
            let name = if clear { None } else { name };
            drop(service);
            if !super::forwarding::rename_session(context, id, name.clone())? {
                session_service(context)?.rename_session(id, name.as_deref())?;
            }
            Ok(simple_session_outcome(id, "renamed"))
        }
        SessionCommand::Trash { session } => {
            manage_session(context, &session, SessionManagement::Trash)
        }
        SessionCommand::Restore { session } => {
            manage_session(context, &session, SessionManagement::Restore)
        }
        SessionCommand::Undo => move_browser_history(context, true),
        SessionCommand::Redo => move_browser_history(context, false),
        SessionCommand::Prune { session, yes } => {
            if !yes {
                return Err(CliError::arguments(
                    "permanent pruning requires --yes".to_owned(),
                ));
            }
            manage_session(context, &session, SessionManagement::Prune)
        }
    }
}

fn move_browser_history(context: &mut RuntimeContext, undo: bool) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let before = service.browser_history_status()?;
    let target = if undo { before.undo } else { before.redo }
        .ok_or(crate::application::SessionServiceError::NoBrowserHistory { undo })?;
    let receipt = service.move_presented_browser_history(target, undo)?;
    let action = if undo { "undid" } else { "redid" };
    Ok(Outcome {
        data: json!({
            "history": if undo { "undo" } else { "redo" },
            "operation": target.kind.label(),
            "cursor": receipt.cursor,
        }),
        human: format!("{action} {}", target.kind.label()),
    })
}

pub(super) fn cancelled_browser() -> Outcome {
    Outcome {
        data: json!({ "cancelled": true }),
        human: "No session opened".to_owned(),
    }
}

pub(super) fn opened_session(id: SessionId) -> Outcome {
    let resume = format!("proqi -r {id}");
    Outcome {
        data: json!({ "session_id": id, "resume_command": resume }),
        human: format!("Session {id}\nResume later: {resume}"),
    }
}

fn manage_session(
    context: &mut RuntimeContext,
    reference: &str,
    action: SessionManagement,
) -> Result<Outcome, CliError> {
    let mut service = session_service(context)?;
    let id = service.resolve_session(reference, true)?;
    match action {
        SessionManagement::Trash => {
            service.trash_session(id)?;
        }
        SessionManagement::Restore => {
            service.restore_session(id)?;
        }
        SessionManagement::Prune => service.prune_session(id)?,
    }
    Ok(simple_session_outcome(id, action.label()))
}

#[derive(Clone, Copy)]
enum SessionManagement {
    Trash,
    Restore,
    Prune,
}

impl SessionManagement {
    const fn label(self) -> &'static str {
        match self {
            Self::Trash => "trashed",
            Self::Restore => "restored",
            Self::Prune => "pruned",
        }
    }
}

fn simple_session_outcome(id: SessionId, action: &str) -> Outcome {
    Outcome {
        data: json!({ "session_id": id, "status": action }),
        human: format!("Session {id} {action}"),
    }
}

pub(super) fn list_sessions(
    context: &mut RuntimeContext,
    query: Option<String>,
    all: bool,
) -> Result<Outcome, CliError> {
    let runtime = context.coordinator.scan_runtime()?;
    let active: HashSet<_> = runtime
        .active
        .into_iter()
        .map(|instance| instance.session_id)
        .collect();
    let recovered: HashSet<_> = runtime.recovered.into_iter().collect();
    let hits = session_service(context)?.list_sessions(query, all)?;
    let data: Vec<_> = hits
        .iter()
        .map(|hit| {
            json!({
                "id": hit.id,
                "name": hit.name,
                "origin_cwd": hit.origin_cwd,
                "last_opened_cwd": hit.last_opened_cwd,
                "last_opened_at": hit.last_opened_at,
                "last_active_at": hit.last_active_at,
                "thought_count": hit.thought_count,
                "excerpt": hit.excerpt,
                "previews": hit.previews,
                "integration_context": hit.integration_context,
                "state": session_state(hit, &active, &recovered),
            })
        })
        .collect();
    let human = human_list(&hits, &active, &recovered);
    Ok(Outcome {
        data: json!({ "sessions": data }),
        human,
    })
}

pub(super) fn browser_items(
    context: &mut RuntimeContext,
) -> Result<Vec<SessionBrowserItem>, CliError> {
    let runtime = context.coordinator.scan_runtime()?;
    let mut active: HashMap<_, _> = runtime
        .active
        .into_iter()
        .map(|instance| (instance.session_id, instance))
        .collect();
    let recovered: HashSet<_> = runtime.recovered.into_iter().collect();
    let hits = session_service(context)?.list_sessions(None, true)?;
    Ok(hits
        .into_iter()
        .map(|hit| {
            let availability = if hit.trashed {
                BrowserAvailability::Trashed
            } else if let Some(instance) = active.remove(&hit.id) {
                BrowserAvailability::Active(instance)
            } else if recovered.contains(&hit.id) {
                BrowserAvailability::Recovered
            } else {
                BrowserAvailability::Resumable
            };
            SessionBrowserItem { hit, availability }
        })
        .collect())
}

fn human_list(
    hits: &[SessionHit],
    active: &HashSet<SessionId>,
    recovered: &HashSet<SessionId>,
) -> String {
    if hits.is_empty() {
        return "No sessions".to_owned();
    }
    hits.iter()
        .map(|hit| {
            let state = session_state(hit, active, recovered);
            let label = hit.name.as_deref().unwrap_or(&hit.excerpt);
            format!(
                "{}  {state}  {}  {label}",
                hit.id,
                hit.last_opened_cwd.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn session_state(
    hit: &SessionHit,
    active: &HashSet<SessionId>,
    recovered: &HashSet<SessionId>,
) -> &'static str {
    if hit.trashed {
        "trashed"
    } else if active.contains(&hit.id) {
        "active"
    } else if recovered.contains(&hit.id) {
        "recovered"
    } else {
        "resumable"
    }
}
