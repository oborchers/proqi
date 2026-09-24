//! Retry-safe session-administration requests and atomic named creation.

use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::domain::{BrowserOperationKind, OperationId, Session, SessionId};

const CREATE_DOMAIN_SEPARATOR: &[u8] = b"proqi-session-create-request\0";

/// Semantic identity of one public session-administration request.
///
/// Two requests with the same caller-supplied operation identity are the same
/// request exactly when these values are equal. Timestamps, cursors, and the
/// prior state that a request happened to observe are deliberately excluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionRequest {
    /// Create one session with its name and origin directory.
    ///
    /// The name and directory are retained only as a digest, so a receipt that
    /// outlives a permanently pruned session retains no user content.
    Create {
        /// Deterministic session identity derived from the operation identity.
        session_id: SessionId,
        /// SHA-256 of the exact requested name and canonical origin directory.
        request_digest: [u8; 32],
    },
    /// Set or clear one session name.
    Rename {
        /// Addressed session.
        session_id: SessionId,
        /// Exact requested name, or `None` to clear it.
        name: Option<String>,
    },
    /// Move one session into recoverable trash, or confirm that it is there.
    Trash {
        /// Addressed session.
        session_id: SessionId,
    },
    /// Restore one session from recoverable trash.
    Restore {
        /// Addressed session.
        session_id: SessionId,
    },
    /// Permanently delete one trashed session.
    Prune {
        /// Addressed session.
        session_id: SessionId,
    },
    /// Move installation-wide Browser history.
    History {
        /// Undo when true, redo otherwise.
        undo: bool,
    },
}

impl SessionRequest {
    /// Identify one named creation without retaining its name or directory.
    #[must_use]
    pub fn create(session_id: SessionId, name: &str, origin_cwd: &Path) -> Self {
        Self::Create {
            session_id,
            request_digest: session_create_digest(name, origin_cwd),
        }
    }

    /// Session addressed by the request, when it addresses one.
    #[must_use]
    pub const fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::Create { session_id, .. }
            | Self::Rename { session_id, .. }
            | Self::Trash { session_id }
            | Self::Restore { session_id }
            | Self::Prune { session_id } => Some(*session_id),
            Self::History { .. } => None,
        }
    }
}

/// Content-free SHA-256 identity of one requested name and origin directory.
#[must_use]
pub fn session_create_digest(name: &str, origin_cwd: &Path) -> [u8; 32] {
    let name = name.as_bytes();
    let mut digest = Sha256::new();
    digest.update(CREATE_DOMAIN_SEPARATOR);
    digest.update(u64::try_from(name.len()).unwrap_or(u64::MAX).to_be_bytes());
    digest.update(name);
    digest.update(origin_cwd.as_os_str().as_encoded_bytes());
    digest.finalize().into()
}

/// Durable result of one retained session-administration request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionRequestReceipt {
    /// Caller-visible operation identity.
    pub operation_id: OperationId,
    /// Semantic request that owns the identity.
    pub request: SessionRequest,
    /// Browser history cursor recorded with the request.
    pub cursor: usize,
    /// Kind of the moved Browser operation, for a retained history movement.
    pub history_target: Option<BrowserOperationKind>,
}

/// Prior use of one operation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredSessionRequest {
    /// A session-administration request already owns the identity.
    Administration(SessionRequestReceipt),
    /// A session's Board or editor history already owns the identity.
    SessionHistory,
}

/// Whether creation may proceed when the requested name is already in use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamedSessionPolicy {
    /// Always create one additional session. Duplicate names remain valid.
    Always,
    /// Create only when no live session has exactly the requested name.
    UnlessNameExists,
}

/// One atomic request to create a named session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedSessionCreation {
    /// Complete initial session, including its name.
    pub session: Session,
    /// Name-collision policy evaluated in the same transaction as insertion.
    pub policy: NamedSessionPolicy,
    /// Operation identity retained for exact retries, when supplied.
    pub operation_id: Option<OperationId>,
}

/// Live session that already uses a requested name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedSessionMatch {
    /// Existing session identity.
    pub id: SessionId,
    /// Directory in which the existing session was created.
    pub origin_cwd: PathBuf,
}

/// Committed or observed result of one atomic named creation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamedSessionOutcome {
    /// The requested session was inserted with its name in one transaction.
    Created,
    /// The operation identity already created this exact session.
    Replayed,
    /// No session was inserted because these live sessions use the name.
    NameInUse(Vec<NamedSessionMatch>),
    /// The operation identity already belongs to another request.
    IdentityReused,
}
