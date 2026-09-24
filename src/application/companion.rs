//! Tab-scoped companion policy for the multiplexer plugin toggle.
//!
//! One tab owns at most one Proqi companion and one recorded session. The
//! policy decides from one host snapshot whether the toggle closes, focuses,
//! returns focus, or opens a pane. It never adopts a process: only a pane this
//! plugin recorded, still running its own session, may be closed, and only an
//! idle shell left in that exact pane after a host restart may be replaced.

mod toggle;

use std::path::PathBuf;

use crate::{
    domain::SessionId,
    ports::companion::{CompanionContext, CompanionRecord, PaneObservation, PaneProcess},
};

pub use toggle::{CompanionToggleError, CompanionToggleOutcome, toggle_companion};

/// Persisted pane label the plugin manifest gives every companion pane.
pub const COMPANION_PANE_LABEL: &str = "Proqi";

/// Derive the Proqi session name for a tab's first companion.
///
/// A meaningful tab label names the session directly. Herdr's default labels
/// are tab positions, which change when tabs close or move, so a missing or
/// numeric-only label uses the stable public tab identity instead, prefixed by
/// the workspace label for readability. The plugin reuses an existing session
/// only when this rule produces its exact name and origin directory.
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
    let workspace = context
        .workspace_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(&context.workspace_id);
    format!("{workspace}-{}", context.tab_id.replace(':', "-"))
}

/// Origin directory for a tab's first session, independent of the focused split.
#[must_use]
pub fn companion_session_cwd(context: &CompanionContext) -> PathBuf {
    context
        .workspace_cwd
        .clone()
        .unwrap_or_else(|| context.focused_pane_cwd.clone())
}

/// Which session an opened companion resumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SessionChoice {
    /// Reopen the session the tab already recorded.
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
    /// A companion exists elsewhere in the tab.
    Focus { pane_id: String },
    /// A companion the plugin cannot close is focused; return to the tab's agent.
    ReturnFocus { pane_id: String },
    /// No live companion exists.
    Open {
        target_pane_id: String,
        session: SessionChoice,
        dead_pane_id: Option<String>,
    },
    /// A companion the plugin cannot close is focused and no single agent exists.
    NoReturnTarget,
}

/// What the tab's record currently describes.
#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordState {
    /// The tab has never recorded a session.
    None,
    /// The tab has a session but no open companion pane.
    Closed { session_id: SessionId },
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
    /// The recorded pane could not be classified; it is never closed or replaced.
    Unknown { pane_id: String },
}

fn record_state(
    panes: &[PaneObservation],
    record: Option<&CompanionRecord>,
    process: Option<&PaneProcess>,
) -> RecordState {
    let Some(record) = record else {
        return RecordState::None;
    };
    let session_id = record.session_id;
    let closed = RecordState::Closed { session_id };
    let Some(pane_id) = record.pane_id.clone() else {
        return closed;
    };
    let Some(pane) = panes.iter().find(|pane| pane.pane_id == pane_id) else {
        return closed;
    };
    match process {
        // The plugin always launches `--resume <id>`, so an unreadable or
        // different session identifies a Proqi someone else started here.
        Some(PaneProcess::Proqi {
            session_id: Some(running),
        }) if *running == session_id => RecordState::Live {
            pane_id,
            session_id,
        },
        Some(PaneProcess::Launcher) => RecordState::Live {
            pane_id,
            session_id,
        },
        // Any remaining Proqi signal keeps the pane: a lease left by a crash
        // expires within its TTL, and closing is never the conservative choice.
        Some(PaneProcess::IdleShell)
            if !pane.proqi_presence && pane.label.as_deref() == Some(COMPANION_PANE_LABEL) =>
        {
            RecordState::Dead {
                pane_id,
                session_id,
            }
        }
        Some(PaneProcess::Unknown) => RecordState::Unknown { pane_id },
        // The pane now belongs to something else; the tab keeps its session.
        None | Some(_) => closed,
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
    match &state {
        RecordState::Live {
            pane_id,
            session_id,
        } if pane_id == focused => {
            return TogglePlan::Close {
                pane_id: pane_id.clone(),
                session_id: *session_id,
            };
        }
        RecordState::Live { pane_id, .. } | RecordState::Unknown { pane_id } => {
            return if pane_id == focused {
                return_focus(panes)
            } else {
                TogglePlan::Focus {
                    pane_id: pane_id.clone(),
                }
            };
        }
        RecordState::None | RecordState::Closed { .. } | RecordState::Dead { .. } => {}
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
    let (session, dead_pane_id) = match state {
        RecordState::Dead {
            pane_id,
            session_id,
        } => (SessionChoice::Recorded(*session_id), Some(pane_id.clone())),
        RecordState::Closed { session_id } | RecordState::Live { session_id, .. } => {
            (SessionChoice::Recorded(*session_id), None)
        }
        RecordState::None | RecordState::Unknown { .. } => (SessionChoice::Named, None),
    };
    let focused = context.focused_pane_id.as_str();
    let target_pane_id = if dead_pane_id.as_deref() == Some(focused) {
        replacement_target(panes, focused).unwrap_or_else(|| focused.to_owned())
    } else {
        focused.to_owned()
    };
    TogglePlan::Open {
        target_pane_id,
        session,
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
