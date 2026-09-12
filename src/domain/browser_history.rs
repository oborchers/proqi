//! Durable cross-session history owned by the session browser.

use serde::{Deserialize, Serialize};

use super::{DomainError, OperationId, SessionId, Timestamp};

/// Semantic session-administration operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserOperationKind {
    /// Changed the optional session name.
    Rename,
    /// Moved a live session into recoverable trash.
    Trash,
    /// Restored a recoverably trashed session.
    Restore,
}

impl BrowserOperationKind {
    /// Truthful local action label used by history presentation.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rename => "session rename",
            Self::Trash => "session trash",
            Self::Restore => "session restore",
        }
    }
}

/// One exact session metadata transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum BrowserMutation {
    /// Compare and replace an optional session name.
    SetName {
        /// Affected session.
        session_id: SessionId,
        /// Required current name.
        expected: Option<String>,
        /// Replacement name.
        value: Option<String>,
    },
    /// Compare and replace recoverable deletion state.
    SetDeletedAt {
        /// Affected session.
        session_id: SessionId,
        /// Required current deletion time.
        expected: Option<Timestamp>,
        /// Replacement deletion time.
        value: Option<Timestamp>,
        /// Required current activity time.
        expected_last_active_at: Timestamp,
        /// Exact replacement activity time.
        last_active_at: Timestamp,
    },
}

impl BrowserMutation {
    /// Session resource addressed by the mutation.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        match self {
            Self::SetName { session_id, .. } | Self::SetDeletedAt { session_id, .. } => *session_id,
        }
    }
}

/// One reversible Browser history entry with its complete inverse.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserOperation {
    /// Stable idempotency identity.
    id: OperationId,
    /// Semantic operation kind.
    kind: BrowserOperationKind,
    /// Mutation used for first apply and redo.
    forward: BrowserMutation,
    /// Mutation used for undo.
    inverse: BrowserMutation,
    /// Creation time.
    created_at: Timestamp,
}

impl BrowserOperation {
    /// Construct an exact reversible rename.
    ///
    /// # Errors
    ///
    /// Returns a domain error for a blank name or a no-op transition.
    pub fn rename(
        id: OperationId,
        session_id: SessionId,
        before: Option<String>,
        after: Option<String>,
        created_at: Timestamp,
    ) -> Result<Self, DomainError> {
        validate_name(before.as_deref())?;
        validate_name(after.as_deref())?;
        if before == after {
            return Err(DomainError::InvalidBrowserOperation);
        }
        Ok(Self {
            id,
            kind: BrowserOperationKind::Rename,
            forward: BrowserMutation::SetName {
                session_id,
                expected: before.clone(),
                value: after.clone(),
            },
            inverse: BrowserMutation::SetName {
                session_id,
                expected: after,
                value: before,
            },
            created_at,
        })
    }

    /// Construct an exact reversible move into recoverable trash.
    #[must_use]
    pub fn trash(
        id: OperationId,
        session_id: SessionId,
        prior_last_active_at: Timestamp,
        deleted_at: Timestamp,
    ) -> Self {
        let trashed_last_active_at = prior_last_active_at.max(deleted_at);
        Self {
            id,
            kind: BrowserOperationKind::Trash,
            forward: BrowserMutation::SetDeletedAt {
                session_id,
                expected: None,
                value: Some(deleted_at),
                expected_last_active_at: prior_last_active_at,
                last_active_at: trashed_last_active_at,
            },
            inverse: BrowserMutation::SetDeletedAt {
                session_id,
                expected: Some(deleted_at),
                value: None,
                expected_last_active_at: trashed_last_active_at,
                last_active_at: prior_last_active_at,
            },
            created_at: deleted_at,
        }
    }

    /// Construct an exact reversible restoration from recoverable trash.
    #[must_use]
    pub const fn restore(
        id: OperationId,
        session_id: SessionId,
        deleted_at: Timestamp,
        last_active_at: Timestamp,
        created_at: Timestamp,
    ) -> Self {
        Self {
            id,
            kind: BrowserOperationKind::Restore,
            forward: BrowserMutation::SetDeletedAt {
                session_id,
                expected: Some(deleted_at),
                value: None,
                expected_last_active_at: last_active_at,
                last_active_at,
            },
            inverse: BrowserMutation::SetDeletedAt {
                session_id,
                expected: None,
                value: Some(deleted_at),
                expected_last_active_at: last_active_at,
                last_active_at,
            },
            created_at,
        }
    }

    /// Validate an operation restored from an untrusted durable payload.
    ///
    /// # Errors
    ///
    /// Returns a content-redacted invariant error for mismatched endpoints.
    pub fn validate(&self) -> Result<(), DomainError> {
        match (&self.kind, &self.forward, &self.inverse) {
            (
                BrowserOperationKind::Rename,
                BrowserMutation::SetName {
                    session_id,
                    expected,
                    value,
                },
                BrowserMutation::SetName {
                    session_id: inverse_session,
                    expected: inverse_expected,
                    value: inverse_value,
                },
            ) if session_id == inverse_session
                && expected == inverse_value
                && value == inverse_expected
                && expected != value =>
            {
                validate_name(expected.as_deref())?;
                validate_name(value.as_deref())
            }
            (
                BrowserOperationKind::Trash,
                BrowserMutation::SetDeletedAt {
                    session_id,
                    expected: None,
                    value: Some(deleted_at),
                    expected_last_active_at,
                    last_active_at,
                },
                BrowserMutation::SetDeletedAt {
                    session_id: inverse_session,
                    expected: Some(inverse_deleted_at),
                    value: None,
                    expected_last_active_at: inverse_expected_active,
                    last_active_at: inverse_active,
                },
            ) if session_id == inverse_session
                && deleted_at == inverse_deleted_at
                && *deleted_at == self.created_at
                && *last_active_at == (*expected_last_active_at).max(*deleted_at)
                && last_active_at == inverse_expected_active
                && expected_last_active_at == inverse_active =>
            {
                Ok(())
            }
            (
                BrowserOperationKind::Restore,
                BrowserMutation::SetDeletedAt {
                    session_id,
                    expected: Some(deleted_at),
                    value: None,
                    expected_last_active_at,
                    last_active_at,
                },
                BrowserMutation::SetDeletedAt {
                    session_id: inverse_session,
                    expected: None,
                    value: Some(inverse_deleted_at),
                    expected_last_active_at: inverse_expected_active,
                    last_active_at: inverse_active,
                },
            ) if session_id == inverse_session
                && deleted_at == inverse_deleted_at
                && expected_last_active_at == last_active_at
                && last_active_at == inverse_expected_active
                && inverse_expected_active == inverse_active =>
            {
                Ok(())
            }
            _ => Err(DomainError::InvalidBrowserOperation),
        }
    }

    /// Stable idempotency identity.
    #[must_use]
    pub const fn id(&self) -> OperationId {
        self.id
    }

    /// Semantic operation kind.
    #[must_use]
    pub const fn kind(&self) -> BrowserOperationKind {
        self.kind
    }

    /// Mutation used for first apply and redo.
    #[must_use]
    pub const fn forward(&self) -> &BrowserMutation {
        &self.forward
    }

    /// Mutation used for undo.
    #[must_use]
    pub const fn inverse(&self) -> &BrowserMutation {
        &self.inverse
    }

    /// Creation time.
    #[must_use]
    pub const fn created_at(&self) -> Timestamp {
        self.created_at
    }

    /// Affected session resource.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.forward.session_id()
    }
}

fn validate_name(name: Option<&str>) -> Result<(), DomainError> {
    if name.is_some_and(|value| value.trim().is_empty()) {
        Err(DomainError::BlankSessionName)
    } else {
        Ok(())
    }
}
