//! Capture the selection or clipboard text into the invoking tab's session.
//!
//! The text is read and validated before any session is resolved or created,
//! so a failure stores and creates nothing. The tab's session is chosen by the
//! same rule as the toggle, which needs no open Proqi pane. The tab records the
//! session it captured into whenever its record named no available session, so
//! later captures and toggles use the same one.

use std::fmt;

use crate::{
    application::{CaptureError, CaptureSource, NOTHING_CAPTURED, capture_text},
    domain::{SessionId, ThoughtId},
    ports::{
        clipboard::Clipboard,
        companion::{
            CompanionError, CompanionHost, CompanionRecord, CompanionRecords, CompanionSessions,
        },
    },
};

use super::{
    SessionChoice,
    session::{SessionResolutionError, TabSession, resolve_tab_session},
};

/// Completed capture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionCaptureOutcome {
    /// Tab whose session received the thought.
    pub tab_id: String,
    /// Session that received the thought.
    pub session_id: SessionId,
    /// New thought.
    pub thought_id: ThoughtId,
    /// Where the text came from.
    pub source: CaptureSource,
    /// Perceived characters stored.
    pub characters: usize,
    /// Exact UTF-8 bytes stored.
    pub bytes: usize,
}

/// Typed capture failure; nothing was stored.
#[derive(Debug)]
pub enum CompanionCaptureError<E> {
    /// No acceptable text was available.
    Capture(CaptureError),
    /// The host or plugin state failed.
    Host(CompanionError),
    /// The session service failed.
    Session(E),
}

impl<E: fmt::Display> fmt::Display for CompanionCaptureError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(error) => write!(formatter, "{error}"),
            // Host and plugin-state failures happen before anything is stored.
            Self::Host(error) => write!(formatter, "{NOTHING_CAPTURED}: {error}"),
            // A store failure may be a timeout after the owner committed, so
            // the wording claims no outcome.
            Self::Session(error) => write!(formatter, "Capture to Proqi failed: {error}"),
        }
    }
}

impl<E> From<CompanionError> for CompanionCaptureError<E> {
    fn from(error: CompanionError) -> Self {
        Self::Host(error)
    }
}

impl<E> From<SessionResolutionError<E>> for CompanionCaptureError<E> {
    fn from(error: SessionResolutionError<E>) -> Self {
        match error {
            SessionResolutionError::Host(error) => Self::Host(error),
            SessionResolutionError::Session(error) => Self::Session(error),
        }
    }
}

/// Store the invocation's selection, or else the clipboard text, as one thought
/// in the tab's session, and tell the user what happened.
///
/// # Errors
///
/// Returns the typed failure after a best-effort host notification.
pub fn capture_to_companion<H, R, S, C>(
    host: &mut H,
    records: &mut R,
    sessions: &mut S,
    clipboard: &mut C,
) -> Result<CompanionCaptureOutcome, CompanionCaptureError<S::Error>>
where
    H: CompanionHost,
    R: CompanionRecords,
    S: CompanionSessions,
    C: Clipboard + ?Sized,
{
    let result = capture(host, records, sessions, clipboard);
    match &result {
        Ok((_, confirmation)) => host.notify(confirmation),
        Err(error) => host.notify(&error.to_string()),
    }
    result.map(|(outcome, _)| outcome)
}

fn capture<H, R, S, C>(
    host: &mut H,
    records: &mut R,
    sessions: &mut S,
    clipboard: &mut C,
) -> Result<(CompanionCaptureOutcome, String), CompanionCaptureError<S::Error>>
where
    H: CompanionHost,
    R: CompanionRecords,
    S: CompanionSessions,
    C: Clipboard + ?Sized,
{
    let context = host.context()?;
    let text = capture_text(context.selected_text.clone(), clipboard)
        .map_err(CompanionCaptureError::Capture)?;
    let record = records.load(&context.tab_id)?;
    let choice = record.as_ref().map_or(SessionChoice::Named, |record| {
        SessionChoice::Recorded(record.session_id)
    });
    let TabSession { session_id, .. } = resolve_tab_session(host, sessions, &context, choice)?;
    let thought_id = sessions
        .capture(session_id, text.text())
        .map_err(CompanionCaptureError::Session)?;
    if record.as_ref().map(|record| record.session_id) != Some(session_id) {
        // Remember the session so later captures and toggles cannot drift to
        // another name. A pane the record still names keeps its entry; the
        // toggle never closes a pane running a different session. The thought
        // is already durable, so a failed save only means the next action
        // resolves the session again by the same rule.
        let _best_effort = records.save(&CompanionRecord {
            tab_id: context.tab_id.clone(),
            pane_id: record.and_then(|record| record.pane_id),
            session_id,
        });
    }
    Ok((
        CompanionCaptureOutcome {
            tab_id: context.tab_id,
            session_id,
            thought_id,
            source: text.source(),
            characters: text.characters(),
            bytes: text.text().len(),
        },
        text.confirmation(),
    ))
}
