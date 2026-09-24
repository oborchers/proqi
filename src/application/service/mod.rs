//! Terminal-independent session lifecycle and scriptable mutation service.

mod board_items;
mod external_edits;
mod sessions;
mod thoughts;
mod transformations;

pub(crate) use board_items::derived_duplicate_item_ids;
pub use sessions::{
    BrowserHistoryMovement, NamedSession, NamedSessionDisposition, RenameAdmission,
    SessionAdministrationReceipt,
};

use std::path::PathBuf;

use thiserror::Error;

use crate::{
    domain::{BoardItemId, DomainError, SessionId, ThoughtId},
    ports::{
        control::{ControlMutation, ControlReceipt},
        environment::{Clock, IdGenerator},
        runtime::{RuntimeCoordinator, RuntimeError},
        store::{CommitReceipt, Store, StoreError, StoredOperationRequest},
    },
};

use super::{AppState, ApplicationError, SequencedMutationEffects};
use super::{ControlReplay, match_control_replay};

fn match_replay(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> Result<ControlReceipt, SessionServiceError> {
    match match_control_replay(existing, session_id, mutation) {
        ControlReplay::Accepted(receipt) => Ok(receipt),
        ControlReplay::Conflict => Err(SessionServiceError::IdempotencyConflict),
    }
}

/// Application facade shared by CLI and terminal UI composition.
pub struct SessionService<'a, S, R, C, I> {
    store: &'a mut S,
    runtime: &'a R,
    clock: &'a C,
    ids: &'a mut I,
    cwd: PathBuf,
}

impl<'a, S, R, C, I> SessionService<'a, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Construct a service for one absolute process working directory.
    ///
    /// # Errors
    ///
    /// Returns an error when `cwd` is not absolute.
    pub fn new(
        store: &'a mut S,
        runtime: &'a R,
        clock: &'a C,
        ids: &'a mut I,
        cwd: PathBuf,
    ) -> Result<Self, SessionServiceError> {
        if !cwd.is_absolute() {
            return Err(SessionServiceError::InvalidDirectory(cwd));
        }
        Ok(Self {
            store,
            runtime,
            clock,
            ids,
            cwd,
        })
    }

    fn load_live_state(&mut self, id: SessionId) -> Result<AppState, SessionServiceError> {
        self.store.compact_session(id)?;
        let snapshot = self.store.load_session(id)?;
        if snapshot.board.session.deleted_at.is_some() {
            return Err(SessionServiceError::SessionTrashed(id));
        }
        Ok(AppState::from_snapshot(snapshot)?)
    }

    fn commit_control_effects(
        &mut self,
        effects: Vec<super::Effect>,
        session_id: SessionId,
        mutation: &ControlMutation,
    ) -> Result<CommitReceipt, SessionServiceError> {
        let routed = SequencedMutationEffects::new(effects)
            .map_err(|_| SessionServiceError::NoDurableMutation)?;
        let routed = super::attach_control_fingerprint(routed, session_id, mutation)?;
        drop(routed.auxiliary);
        self.store
            .commit(&routed.batch)?
            .ok_or(SessionServiceError::NoDurableMutation)
    }
}

/// Editable state paired with its authoritative lease.
pub struct LeasedSession<L> {
    /// Rehydrated application state.
    pub state: AppState,
    lease: L,
}

impl<L> LeasedSession<L> {
    /// Borrow the lease so composition keeps it alive for the editing lifetime.
    #[must_use]
    pub const fn lease(&self) -> &L {
        &self.lease
    }

    /// Consume the editing handle while preserving both state and lease.
    #[must_use]
    pub fn into_parts(self) -> (AppState, L) {
        (self.state, self.lease)
    }
}

/// Durable result of one thought mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThoughtMutation {
    /// Affected thought.
    pub thought_id: ThoughtId,
    /// Durable operation receipt.
    pub receipt: CommitReceipt,
}

/// Durable result of one mixed Board-item mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoardItemMutation {
    /// Created or affected items in canonical Board order.
    pub item_ids: Vec<BoardItemId>,
    /// Durable operation receipt.
    pub receipt: CommitReceipt,
}

/// Session service failure with stable semantic categories.
#[derive(Debug, Error)]
pub enum SessionServiceError {
    /// Persistence adapter failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Runtime ownership failed.
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    /// Reducer rejected the request.
    #[error(transparent)]
    Application(#[from] ApplicationError),
    /// Domain validation failed before reducer construction.
    #[error(transparent)]
    Domain(#[from] DomainError),
    /// No matching session exists.
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// The requested session name belongs only to sessions from other directories.
    #[error("session name is already used for another directory: {name}")]
    SessionNameConflict {
        /// Exact requested name.
        name: String,
        /// Live sessions that already use the name.
        sessions: Vec<crate::ports::store::NamedSessionMatch>,
    },
    /// More than one session has the requested name.
    #[error("session name is ambiguous: {reference}")]
    AmbiguousSession {
        /// User-supplied reference.
        reference: String,
        /// Matching canonical identifiers.
        matches: Vec<SessionId>,
    },
    /// A recoverably deleted session cannot be edited until restored.
    #[error("session is in trash: {0}")]
    SessionTrashed(SessionId),
    /// A live session cannot be restored because it is not in recoverable trash.
    #[error("session is not in trash: {0}")]
    SessionNotTrashed(SessionId),
    /// The installation-wide Browser history has no entry in this direction.
    #[error("nothing to {action} in Browser history", action = if *undo { "undo" } else { "redo" })]
    NoBrowserHistory {
        /// Undo when true, redo when false.
        undo: bool,
    },
    /// Supplied operation identity belongs to another semantic request.
    #[error("operation identity was already used for another request")]
    IdempotencyConflict,
    /// The requested action produced no durable mutation.
    #[error("request did not change durable state")]
    NoDurableMutation,
    /// A typed identifier was malformed or had the wrong resource prefix.
    #[error("invalid identifier {value}: {reason}")]
    InvalidIdentifier {
        /// Original value.
        value: String,
        /// Stable validation explanation.
        reason: String,
    },
    /// Process working directory is invalid.
    #[error("working directory must be absolute: {0}")]
    InvalidDirectory(PathBuf),
}
