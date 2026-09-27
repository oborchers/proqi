//! The one rule that picks a tab's Proqi session, shared by toggle and capture.

use crate::{
    domain::SessionId,
    ports::companion::{
        CompanionContext, CompanionError, CompanionHost, CompanionSessionState, CompanionSessions,
    },
};

use super::{SessionChoice, companion_session_cwd, companion_session_name};

/// A tab's resolved session.
pub(super) struct TabSession {
    /// Session the tab uses.
    pub(super) session_id: SessionId,
    /// Name the tab resolved to, or `None` when a recorded session was reused.
    pub(super) name: Option<String>,
}

/// Failure while resolving a tab's session.
pub(super) enum SessionResolutionError<E> {
    /// The host could not report the tab's agents.
    Host(CompanionError),
    /// The session service failed.
    Session(E),
}

impl<E> From<CompanionError> for SessionResolutionError<E> {
    fn from(error: CompanionError) -> Self {
        Self::Host(error)
    }
}

/// Reuse the tab's recorded session unless it was trashed or deleted, else get
/// or create the tab's named session in its stable origin directory.
///
/// A failed agent query stops here. The tab records its first choice, so
/// falling back to the label would pin a different session for good.
pub(super) fn resolve_tab_session<H, S>(
    host: &mut H,
    sessions: &mut S,
    context: &CompanionContext,
    choice: SessionChoice,
) -> Result<TabSession, SessionResolutionError<S::Error>>
where
    H: CompanionHost,
    S: CompanionSessions,
{
    if let SessionChoice::Recorded(session_id) = choice {
        match sessions
            .state(session_id)
            .map_err(SessionResolutionError::Session)?
        {
            CompanionSessionState::Unavailable => {}
            CompanionSessionState::Resumable | CompanionSessionState::Active => {
                return Ok(TabSession {
                    session_id,
                    name: None,
                });
            }
        }
    }
    let agent_names = host.tab_agent_names(&context.tab_id)?;
    let name = companion_session_name(context, &agent_names);
    let session_id = sessions
        .ensure(&name, &companion_session_cwd(context))
        .map_err(SessionResolutionError::Session)?;
    Ok(TabSession {
        session_id,
        name: Some(name),
    })
}
