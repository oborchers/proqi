//! Tab-scoped companion policy for the multiplexer plugin toggle.
//!
//! One tab owns at most one Proqi companion. The policy decides from one host
//! snapshot whether the toggle closes, focuses, returns focus, or opens a pane.
//! It never adopts a process: only a pane this plugin recorded, still running
//! its own session, may be closed, and only an idle shell left in that exact
//! pane after a host restart may be replaced.

mod toggle;

use std::path::PathBuf;

use crate::{
    domain::SessionId,
    ports::companion::{CompanionContext, CompanionRecord, PaneObservation, PaneProcess},
};

pub use toggle::{CompanionToggleError, CompanionToggleOutcome, toggle_companion};

/// Persisted pane label the plugin manifest gives every companion pane.
pub const COMPANION_PANE_LABEL: &str = "Proqi";

/// Derive the Proqi session name for one tab.
///
/// A meaningful tab label names the session directly, which matches the name a
/// user or reconciler gives a tab's companion by hand. Herdr's default labels
/// are tab numbers, so a missing or numeric-only label is qualified by the
/// workspace label, or by the workspace identity when that label is blank.
#[must_use]
pub fn companion_session_name(context: &CompanionContext) -> String {
    let label = context
        .tab_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty());
    if let Some(label) = label
        && label.chars().any(|character| !character.is_ascii_digit())
    {
        return label.to_owned();
    }
    let tab = label.unwrap_or(&context.tab_id);
    let workspace = label_or_id(context.workspace_label.as_deref(), &context.workspace_id);
    format!("{workspace}-{tab}")
}

fn label_or_id<'a>(label: Option<&'a str>, id: &'a str) -> &'a str {
    label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(id)
}

/// Which session an opened companion resumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionChoice {
    /// Reopen the session of a recorded companion that died with its host.
    Recorded(SessionId),
    /// Get or create the tab's named session.
    Named,
}

/// One toggle decision derived from a single snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TogglePlan {
    /// The focused pane is this plugin's live companion.
    Close {
        pane_id: String,
        session_id: SessionId,
    },
    /// A live companion exists elsewhere in the tab.
    Focus { pane_id: String },
    /// A companion this plugin did not open is focused; return to the tab's agent.
    ReturnFocus { pane_id: String },
    /// No live companion exists.
    Open {
        target_pane_id: String,
        cwd: PathBuf,
        session: SessionChoice,
        forget_record: bool,
        dead_pane_id: Option<String>,
    },
    /// A foreign companion is focused and the tab has no single agent to return to.
    NoReturnTarget,
}

/// What the recorded pane of this tab currently is.
#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordState {
    /// No record, or a record for a pane that no longer exists.
    Absent { stale: bool },
    /// The recorded pane still runs this plugin's companion.
    Live {
        pane_id: String,
        session_id: SessionId,
    },
    /// The recorded pane survived a host restart as an idle shell.
    Dead {
        pane_id: String,
        session_id: SessionId,
    },
    /// The recorded pane now belongs to something else and is never touched.
    Reused,
}

fn record_state(
    panes: &[PaneObservation],
    record: Option<&CompanionRecord>,
    process: Option<&PaneProcess>,
) -> RecordState {
    let Some(record) = record else {
        return RecordState::Absent { stale: false };
    };
    let Some(pane) = panes.iter().find(|pane| pane.pane_id == record.pane_id) else {
        return RecordState::Absent { stale: true };
    };
    let live = RecordState::Live {
        pane_id: record.pane_id.clone(),
        session_id: record.session_id,
    };
    match process {
        Some(PaneProcess::Proqi { session_id }) => match session_id {
            Some(session_id) if *session_id != record.session_id => RecordState::Reused,
            _ => live,
        },
        Some(PaneProcess::Launcher) => live,
        Some(PaneProcess::IdleShell)
            if !pane.proqi_presence && pane.label.as_deref() == Some(COMPANION_PANE_LABEL) =>
        {
            RecordState::Dead {
                pane_id: record.pane_id.clone(),
                session_id: record.session_id,
            }
        }
        None => RecordState::Absent { stale: true },
        Some(_) => RecordState::Reused,
    }
}

/// Decide the toggle for one tab snapshot.
pub(crate) fn plan_toggle(
    context: &CompanionContext,
    panes: &[PaneObservation],
    record: Option<&CompanionRecord>,
    process: Option<&PaneProcess>,
) -> TogglePlan {
    let state = record_state(panes, record, process);
    let focused = context.focused_pane_id.as_str();
    if let RecordState::Live {
        pane_id,
        session_id,
    } = &state
    {
        return if pane_id == focused {
            TogglePlan::Close {
                pane_id: pane_id.clone(),
                session_id: *session_id,
            }
        } else {
            TogglePlan::Focus {
                pane_id: pane_id.clone(),
            }
        };
    }
    let present = |pane: &&PaneObservation| pane.proqi_presence;
    if panes
        .iter()
        .filter(present)
        .any(|pane| pane.pane_id == focused)
    {
        return return_focus(panes);
    }
    if let Some(pane) = panes.iter().find(present) {
        return TogglePlan::Focus {
            pane_id: pane.pane_id.clone(),
        };
    }
    open_plan(context, panes, &state)
}

fn return_focus(panes: &[PaneObservation]) -> TogglePlan {
    let mut agents = panes
        .iter()
        .filter(|pane| pane.agent && !pane.proqi_presence);
    match (agents.next(), agents.next()) {
        (Some(agent), None) => TogglePlan::ReturnFocus {
            pane_id: agent.pane_id.clone(),
        },
        _ => TogglePlan::NoReturnTarget,
    }
}

fn open_plan(
    context: &CompanionContext,
    panes: &[PaneObservation],
    state: &RecordState,
) -> TogglePlan {
    let (session, dead_pane_id, forget_record) = match state {
        RecordState::Dead {
            pane_id,
            session_id,
        } => (
            SessionChoice::Recorded(*session_id),
            Some(pane_id.clone()),
            false,
        ),
        RecordState::Absent { stale } => (SessionChoice::Named, None, *stale),
        RecordState::Reused => (SessionChoice::Named, None, true),
        RecordState::Live { .. } => (SessionChoice::Named, None, false),
    };
    let focused = context.focused_pane_id.as_str();
    let target_pane_id = if dead_pane_id.as_deref() == Some(focused) {
        replacement_target(panes, focused).unwrap_or_else(|| focused.to_owned())
    } else {
        focused.to_owned()
    };
    TogglePlan::Open {
        target_pane_id,
        cwd: context.focused_pane_cwd.clone(),
        session,
        forget_record,
        dead_pane_id,
    }
}

/// Prefer an agent, then any other pane, when the focused pane is being replaced.
fn replacement_target(panes: &[PaneObservation], dead: &str) -> Option<String> {
    let others = || panes.iter().filter(|pane| pane.pane_id != dead);
    others()
        .find(|pane| pane.agent)
        .or_else(|| others().next())
        .map(|pane| pane.pane_id.clone())
}

#[cfg(test)]
mod tests;
