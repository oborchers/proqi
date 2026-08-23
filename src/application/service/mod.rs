//! Terminal-independent session lifecycle and scriptable mutation service.

mod sessions;
mod thoughts;

use std::path::PathBuf;

use thiserror::Error;

use crate::{
    domain::{DomainError, SessionId, ThoughtId},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::{RuntimeCoordinator, RuntimeError},
        store::{CommitReceipt, Store, StoreError},
    },
};

use super::{AppState, ApplicationError};

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
}

/// Durable result of one thought mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThoughtMutation {
    /// Affected thought.
    pub thought_id: ThoughtId,
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
