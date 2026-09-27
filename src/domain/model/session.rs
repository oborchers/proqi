//! Session identity, optional naming, and navigation metadata.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{DomainError, OperationSequence, Timestamp};
use crate::domain::SessionId;

/// A scratchpad session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Stable identity.
    pub id: SessionId,
    /// Optional user-assigned name.
    pub name: Option<String>,
    /// Directory from which the session was created.
    pub origin_cwd: PathBuf,
    /// Directory from which it was most recently opened.
    pub last_opened_cwd: PathBuf,
    /// Creation time.
    pub created_at: Timestamp,
    /// Most recent successful opening time.
    pub last_opened_at: Timestamp,
    /// Most recent content activity time.
    pub last_active_at: Timestamp,
    /// Last operation acknowledged as durable.
    pub last_durable_sequence: OperationSequence,
    /// Soft-deletion time, if in recoverable trash.
    pub deleted_at: Option<Timestamp>,
}

impl Session {
    /// Create a live, unnamed session.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::RelativeDirectory`] when `cwd` is not absolute.
    pub fn new(id: SessionId, cwd: PathBuf, now: Timestamp) -> Result<Self, DomainError> {
        Self::with_name(id, cwd, now, None)
    }

    /// Create a live session whose optional name exists from its first durable state.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::RelativeDirectory`] when `cwd` is not absolute and
    /// [`DomainError::BlankSessionName`] for a whitespace-only name.
    pub fn with_name(
        id: SessionId,
        cwd: PathBuf,
        now: Timestamp,
        name: Option<String>,
    ) -> Result<Self, DomainError> {
        validate_absolute_path(&cwd)?;
        if let Some(name) = name.as_deref() {
            validate_session_name(name)?;
        }
        Ok(Self {
            id,
            name,
            origin_cwd: cwd.clone(),
            last_opened_cwd: cwd,
            created_at: now,
            last_opened_at: now,
            last_active_at: now,
            last_durable_sequence: OperationSequence::ZERO,
            deleted_at: None,
        })
    }

    /// Rename the session, or clear its optional name.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::BlankSessionName`] for whitespace-only names.
    pub fn rename(&mut self, name: Option<String>) -> Result<(), DomainError> {
        if let Some(name) = name.as_deref() {
            validate_session_name(name)?;
        }
        self.name = name;
        Ok(())
    }

    /// Validate restored session paths and optional naming invariants.
    ///
    /// # Errors
    ///
    /// Returns a domain error when persisted state bypassed constructor invariants.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_absolute_path(&self.origin_cwd)?;
        validate_absolute_path(&self.last_opened_cwd)?;
        if let Some(name) = self.name.as_deref() {
            validate_session_name(name)?;
        }
        Ok(())
    }

    /// Record a successful open after a lease has been acquired.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::RelativeDirectory`] when `cwd` is not absolute.
    pub fn record_open(&mut self, cwd: PathBuf, now: Timestamp) -> Result<(), DomainError> {
        validate_absolute_path(&cwd)?;
        self.last_opened_cwd = cwd;
        self.last_opened_at = now;
        self.last_active_at = self.last_active_at.max(now);
        Ok(())
    }
}

/// Validate one supplied session name.
///
/// Session names are exact user metadata. They may contain any Unicode text,
/// but a name consisting only of whitespace is indistinguishable from no name.
///
/// # Errors
///
/// Returns [`DomainError::BlankSessionName`] for an empty or whitespace-only name.
pub fn validate_session_name(name: &str) -> Result<(), DomainError> {
    if name.trim().is_empty() {
        Err(DomainError::BlankSessionName)
    } else {
        Ok(())
    }
}

fn validate_absolute_path(path: &Path) -> Result<(), DomainError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(DomainError::RelativeDirectory(path.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{Session, validate_session_name};
    use crate::domain::{DomainError, SessionId, Timestamp};

    fn session_id() -> SessionId {
        "ses_06g30t7dv5qv55n1ppn3clis3k"
            .parse()
            .expect("session ID")
    }

    #[test]
    fn named_creation_sets_the_name_in_the_first_durable_state() {
        let session = Session::with_name(
            session_id(),
            PathBuf::from("/work/agent-os"),
            Timestamp::from_millis(7),
            Some("agent-os-claude".to_owned()),
        )
        .expect("named session");
        assert_eq!(session.name.as_deref(), Some("agent-os-claude"));
        assert_eq!(session.origin_cwd, PathBuf::from("/work/agent-os"));
        assert_eq!(session.last_opened_cwd, session.origin_cwd);
        assert!(session.validate().is_ok());
    }

    #[test]
    fn named_creation_rejects_blank_names_and_relative_directories() {
        for blank in ["", " ", "\t\n", "\u{3000}"] {
            assert_eq!(
                Session::with_name(
                    session_id(),
                    PathBuf::from("/work"),
                    Timestamp::from_millis(1),
                    Some(blank.to_owned()),
                ),
                Err(DomainError::BlankSessionName),
                "{blank:?}"
            );
        }
        assert!(matches!(
            Session::with_name(
                session_id(),
                PathBuf::from("relative"),
                Timestamp::from_millis(1),
                Some("named".to_owned()),
            ),
            Err(DomainError::RelativeDirectory(_))
        ));
    }

    #[test]
    fn exact_names_preserve_unicode_and_surrounding_text() {
        for name in [
            "ünïcödé 名前",
            " padded ",
            "line\nbreak",
            "x".repeat(200).as_str(),
        ] {
            assert!(validate_session_name(name).is_ok(), "{name:?}");
        }
    }
}
