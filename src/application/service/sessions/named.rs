//! Atomic creation and get-or-create of named sessions without a TUI.

use std::path::{Path, PathBuf};

use crate::{
    domain::{OperationId, Session, SessionId},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{
            NamedSessionCreation, NamedSessionMatch, NamedSessionOutcome, NamedSessionPolicy,
            Store, StoreError,
        },
    },
};

use super::super::{SessionService, SessionServiceError};

/// Whether a named-session request found or created its session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NamedSessionDisposition {
    /// The session was created with its name in one atomic commit.
    Created,
    /// An existing live session with the same name and origin directory was returned.
    Reused,
}

impl NamedSessionDisposition {
    /// Stable machine-readable spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Reused => "reused",
        }
    }
}

/// Result of one named-session request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedSession {
    /// Created or reused session.
    pub session_id: SessionId,
    /// Whether the request created or reused the session.
    pub disposition: NamedSessionDisposition,
    /// Operation identity retained for creation, when one owns the request.
    pub operation_id: Option<OperationId>,
    /// Whether an exact earlier creation request was replayed.
    pub idempotent_replay: bool,
}

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Return the one live session with this exact name and origin, creating it when absent.
    ///
    /// The name lookup and insertion run in one storage transaction, so concurrent
    /// callers create at most one session. No lease is acquired and no TUI starts.
    ///
    /// # Errors
    ///
    /// Returns ambiguity when several live sessions match, a name conflict when the
    /// name belongs only to other directories, or a typed validation or storage failure.
    pub fn ensure_named_session(
        &mut self,
        name: String,
        cwd: PathBuf,
    ) -> Result<NamedSession, SessionServiceError> {
        let session = Session::with_name(self.ids.session_id(), cwd, self.clock.now(), Some(name))?;
        let session_id = session.id;
        let creation = NamedSessionCreation {
            session,
            policy: NamedSessionPolicy::UnlessNameExists,
            operation_id: None,
        };
        match self.store.create_named_session(&creation)? {
            NamedSessionOutcome::Created => Ok(NamedSession {
                session_id,
                disposition: NamedSessionDisposition::Created,
                operation_id: None,
                idempotent_replay: false,
            }),
            NamedSessionOutcome::NameInUse(existing) => {
                let requested = &creation.session;
                reuse_exact(
                    requested.name.as_deref().unwrap_or_default(),
                    &requested.origin_cwd,
                    existing,
                )
            }
            NamedSessionOutcome::Replayed | NamedSessionOutcome::IdentityReused => Err(
                StoreError::Invariant("get-or-create carried no operation identity".to_owned())
                    .into(),
            ),
        }
    }

    /// Create one additional named session, even when the name is already in use.
    ///
    /// The session identity derives from the operation identity, and the retained
    /// creation receipt makes an exact retry return the same session.
    ///
    /// # Errors
    ///
    /// Returns an idempotency conflict for reused identities or a typed validation
    /// or storage failure.
    pub fn create_named_session(
        &mut self,
        name: String,
        cwd: PathBuf,
        supplied: Option<OperationId>,
    ) -> Result<NamedSession, SessionServiceError> {
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let session_id = SessionId::from_database_bytes(operation_id.database_bytes())
            .map_err(|_| SessionServiceError::IdempotencyConflict)?;
        let session = Session::with_name(session_id, cwd, self.clock.now(), Some(name))?;
        let creation = NamedSessionCreation {
            session,
            policy: NamedSessionPolicy::Always,
            operation_id: Some(operation_id),
        };
        let idempotent_replay = match self.store.create_named_session(&creation)? {
            NamedSessionOutcome::Created => false,
            NamedSessionOutcome::Replayed => true,
            NamedSessionOutcome::IdentityReused => {
                return Err(SessionServiceError::IdempotencyConflict);
            }
            NamedSessionOutcome::NameInUse(_) => {
                return Err(StoreError::Invariant(
                    "unconditional creation reported a name collision".to_owned(),
                )
                .into());
            }
        };
        Ok(NamedSession {
            session_id,
            disposition: NamedSessionDisposition::Created,
            operation_id: Some(operation_id),
            idempotent_replay,
        })
    }
}

/// Classify live sessions that already use the requested name.
fn reuse_exact(
    name: &str,
    cwd: &Path,
    existing: Vec<NamedSessionMatch>,
) -> Result<NamedSession, SessionServiceError> {
    let exact: Vec<_> = existing
        .iter()
        .filter(|candidate| candidate.origin_cwd == cwd)
        .map(|candidate| candidate.id)
        .collect();
    match exact.as_slice() {
        [session_id] => Ok(NamedSession {
            session_id: *session_id,
            disposition: NamedSessionDisposition::Reused,
            operation_id: None,
            idempotent_replay: false,
        }),
        [] => Err(SessionServiceError::SessionNameConflict {
            name: name.to_owned(),
            sessions: existing,
        }),
        _ => Err(SessionServiceError::AmbiguousSession {
            reference: name.to_owned(),
            matches: exact,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{NamedSessionDisposition, reuse_exact};
    use crate::{
        application::SessionServiceError, domain::SessionId, ports::store::NamedSessionMatch,
    };

    fn candidate(id: &str, cwd: &str) -> NamedSessionMatch {
        NamedSessionMatch {
            id: id.parse::<SessionId>().expect("session ID"),
            origin_cwd: PathBuf::from(cwd),
        }
    }

    const FIRST: &str = "ses_06g30t7dv5qv55n1ppn3clis3k";
    const SECOND: &str = "ses_06g30t8fudrq55fdkjqr6mpe44";

    #[test]
    fn one_exact_directory_match_is_reused_even_beside_other_directories() {
        let reused = reuse_exact(
            "agent",
            Path::new("/work/a"),
            vec![candidate(FIRST, "/work/b"), candidate(SECOND, "/work/a")],
        )
        .expect("reuse");
        assert_eq!(reused.session_id.to_string(), SECOND);
        assert_eq!(reused.disposition, NamedSessionDisposition::Reused);
    }

    #[test]
    fn several_exact_matches_are_ambiguous_without_guessing() {
        let error = reuse_exact(
            "agent",
            Path::new("/work/a"),
            vec![candidate(FIRST, "/work/a"), candidate(SECOND, "/work/a")],
        )
        .expect_err("ambiguous");
        assert!(matches!(
            error,
            SessionServiceError::AmbiguousSession { ref matches, .. } if matches.len() == 2
        ));
    }

    #[test]
    fn a_name_used_only_elsewhere_is_a_structured_conflict() {
        let error = reuse_exact(
            "agent",
            Path::new("/work/a"),
            vec![candidate(FIRST, "/work/b")],
        )
        .expect_err("conflict");
        assert!(matches!(
            error,
            SessionServiceError::SessionNameConflict { ref name, ref sessions }
                if name == "agent" && sessions.len() == 1
        ));
    }
}
