//! Orchestration of one companion toggle across host, plugin state, and sessions.

use std::fmt;

use crate::{
    domain::SessionId,
    ports::companion::{
        CompanionContext, CompanionError, CompanionHost, CompanionRecord, CompanionRecords,
        CompanionSessionState, CompanionSessions, PaneProcess,
    },
};

use super::{
    SessionChoice, TogglePlan, companion_session_cwd, companion_session_name, plan_toggle,
};

/// Completed toggle effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompanionToggleOutcome {
    /// A companion pane was opened.
    Opened {
        /// Tab that owns the companion.
        tab_id: String,
        /// New companion pane.
        pane_id: String,
        /// Session it resumes.
        session_id: SessionId,
        /// Dead pane closed after the replacement opened, when it was still idle.
        replaced_pane_id: Option<String>,
    },
    /// An existing companion received focus.
    Focused {
        /// Focused companion pane.
        pane_id: String,
    },
    /// Focus returned from a companion the plugin did not open to the tab's agent.
    Returned {
        /// Agent pane that received focus.
        pane_id: String,
    },
    /// The focused companion flushed its work and closed.
    Closed {
        /// Closed pane.
        pane_id: String,
        /// Session that remains resumable.
        session_id: SessionId,
    },
}

/// Typed toggle failure.
#[derive(Debug)]
pub enum CompanionToggleError<E> {
    /// The host, plugin state, or invocation context failed.
    Host(CompanionError),
    /// The session service failed.
    Session(E),
    /// Another pane already runs the session this tab would open.
    SessionActive {
        /// Session that is already open.
        session_id: SessionId,
        /// Session name the tab resolved to, when name-based.
        name: Option<String>,
    },
    /// A companion the plugin did not open is focused and no single agent exists.
    NoReturnTarget,
}

impl<E: fmt::Display> fmt::Display for CompanionToggleError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(error) => write!(formatter, "{error}"),
            Self::Session(error) => write!(formatter, "{error}"),
            Self::SessionActive {
                name: Some(name), ..
            } => write!(
                formatter,
                "Proqi session {name} is already open in another pane"
            ),
            Self::SessionActive { session_id, .. } => write!(
                formatter,
                "Proqi session {session_id} is already open in another pane"
            ),
            Self::NoReturnTarget => formatter.write_str(
                "this Proqi pane was not opened by the plugin and the tab has no single agent pane",
            ),
        }
    }
}

impl<E> From<CompanionError> for CompanionToggleError<E> {
    fn from(error: CompanionError) -> Self {
        Self::Host(error)
    }
}

/// Toggle the current tab's companion and report any failure to the user.
///
/// # Errors
///
/// Returns the typed failure after a best-effort host notification.
pub fn toggle_companion<H, R, S>(
    host: &mut H,
    records: &mut R,
    sessions: &mut S,
) -> Result<CompanionToggleOutcome, CompanionToggleError<S::Error>>
where
    H: CompanionHost,
    R: CompanionRecords,
    S: CompanionSessions,
{
    let result = toggle(host, records, sessions);
    if let Err(error) = &result {
        host.notify(&error.to_string());
    }
    result
}

fn toggle<H, R, S>(
    host: &mut H,
    records: &mut R,
    sessions: &mut S,
) -> Result<CompanionToggleOutcome, CompanionToggleError<S::Error>>
where
    H: CompanionHost,
    R: CompanionRecords,
    S: CompanionSessions,
{
    let context = host.context()?;
    let panes = host.tab_panes(&context.tab_id)?;
    let record = records.load(&context.tab_id)?;
    let recorded_pane = record
        .as_ref()
        .and_then(|record| record.pane_id.as_deref())
        .filter(|recorded| panes.iter().any(|pane| pane.pane_id == *recorded));
    let process = match recorded_pane {
        Some(pane_id) => host.process(pane_id)?,
        None => None,
    };
    match plan_toggle(&context, &panes, record.as_ref(), process.as_ref()) {
        TogglePlan::Close {
            pane_id,
            session_id,
        } => {
            sessions
                .flush(session_id)
                .map_err(CompanionToggleError::Session)?;
            host.close(&pane_id)?;
            // The tab keeps its session, so the next toggle reopens it from any pane.
            records.save(&CompanionRecord {
                tab_id: context.tab_id.clone(),
                pane_id: None,
                session_id,
            })?;
            Ok(CompanionToggleOutcome::Closed {
                pane_id,
                session_id,
            })
        }
        TogglePlan::Focus { pane_id } => {
            host.focus(&pane_id)?;
            Ok(CompanionToggleOutcome::Focused { pane_id })
        }
        TogglePlan::ReturnFocus { pane_id } => {
            host.focus(&pane_id)?;
            Ok(CompanionToggleOutcome::Returned { pane_id })
        }
        TogglePlan::NoReturnTarget => Err(CompanionToggleError::NoReturnTarget),
        TogglePlan::Open {
            target_pane_id,
            session,
            dead_pane_id,
        } => {
            let session_id = open_session(host, records, sessions, &context, session)?;
            let pane_id =
                host.open_beside(&target_pane_id, &context.focused_pane_cwd, session_id)?;
            // On failure the new Proqi stays open and usable. Closing it could
            // discard edits, and later toggles still recognize and focus it.
            // The dead pane is kept because nothing recorded its replacement.
            records.save(&CompanionRecord {
                tab_id: context.tab_id.clone(),
                pane_id: Some(pane_id.clone()),
                session_id,
            })?;
            let replaced_pane_id = match dead_pane_id {
                Some(dead) if still_idle(host, &dead) => {
                    host.close(&dead)?;
                    Some(dead)
                }
                _ => None,
            };
            Ok(CompanionToggleOutcome::Opened {
                tab_id: context.tab_id,
                pane_id,
                session_id,
                replaced_pane_id,
            })
        }
    }
}

/// Opening can take seconds; the user may have started a command in the dead
/// shell meanwhile. Only a pane that is still an idle shell right now is closed.
fn still_idle<H: CompanionHost>(host: &mut H, pane_id: &str) -> bool {
    matches!(host.process(pane_id), Ok(Some(PaneProcess::IdleShell)))
}

/// Resolve the session to open and prove that no other pane already runs it.
fn open_session<H, R, S>(
    host: &mut H,
    records: &mut R,
    sessions: &mut S,
    context: &CompanionContext,
    choice: SessionChoice,
) -> Result<SessionId, CompanionToggleError<S::Error>>
where
    H: CompanionHost,
    R: CompanionRecords,
    S: CompanionSessions,
{
    let recorded = match choice {
        SessionChoice::Recorded(session_id) => {
            match sessions
                .state(session_id)
                .map_err(CompanionToggleError::Session)?
            {
                CompanionSessionState::Unavailable => None,
                CompanionSessionState::Resumable | CompanionSessionState::Active => {
                    Some(session_id)
                }
            }
        }
        SessionChoice::Named => None,
    };
    let (session_id, name) = if let Some(session_id) = recorded {
        (session_id, None)
    } else {
        let name = companion_session_name(context);
        let session_id = sessions
            .ensure(&name, &companion_session_cwd(context))
            .map_err(CompanionToggleError::Session)?;
        (session_id, Some(name))
    };
    let active = CompanionToggleError::SessionActive { session_id, name };
    if sessions
        .state(session_id)
        .map_err(CompanionToggleError::Session)?
        == CompanionSessionState::Active
    {
        return Err(active);
    }
    if other_tab_is_opening(host, records, &context.tab_id, session_id)? {
        return Err(active);
    }
    Ok(session_id)
}

/// Catch a companion another tab opened moments ago, before its Proqi owns the lease.
fn other_tab_is_opening<H, R>(
    host: &mut H,
    records: &mut R,
    tab_id: &str,
    session_id: SessionId,
) -> Result<bool, CompanionError>
where
    H: CompanionHost,
    R: CompanionRecords,
{
    for record in records.all()? {
        if record.tab_id == tab_id || record.session_id != session_id {
            continue;
        }
        let Some(pane_id) = record.pane_id.as_deref() else {
            continue;
        };
        // An unclassifiable pane is not proof of an opening companion; the
        // session lease checked before remains the authority.
        match host.process(pane_id).unwrap_or(Some(PaneProcess::Unknown)) {
            Some(PaneProcess::Launcher) => return Ok(true),
            // The same predicate as the tab's own record: only the exact
            // session is the plugin's. A Proqi resuming it by name holds the
            // session lease, which the preceding state check already reports.
            Some(PaneProcess::Proqi {
                session_id: Some(running),
            }) if running == session_id => return Ok(true),
            Some(_) => {}
            None => records.save(&CompanionRecord {
                pane_id: None,
                ..record
            })?,
        }
    }
    Ok(false)
}
