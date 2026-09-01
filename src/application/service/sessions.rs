//! Session creation, recognition, ownership, and metadata operations.

use std::str::FromStr;

use crate::{
    application::AppState,
    domain::{Session, SessionId},
    ports::{
        environment::{Clock, IdGenerator},
        runtime::{RuntimeCoordinator, RuntimeError},
        store::{OperationBatch, SessionHit, SessionQuery, SessionSnapshot, Store},
    },
};

use super::{LeasedSession, SessionService, SessionServiceError};

#[cfg(test)]
mod tests;

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Create, persist, and exclusively lease a fresh session.
    ///
    /// # Errors
    ///
    /// Returns a typed identity, domain, lease, storage, or rehydration failure.
    pub fn create_session(
        &mut self,
    ) -> Result<LeasedSession<R::SessionLease>, SessionServiceError> {
        let id = self.ids.session_id();
        let lease = self.runtime.acquire_session(id)?;
        let session = Session::new(id, self.cwd.clone(), self.clock.now())?;
        let _receipt = self.store.commit(&OperationBatch::CreateSession(session))?;
        self.load_after_lease(id, lease, false)
    }

    /// Resume one live session after acquiring its authoritative lease.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, absence, trash-state, storage, or rehydration failure.
    pub fn resume(
        &mut self,
        id: SessionId,
    ) -> Result<LeasedSession<R::SessionLease>, SessionServiceError> {
        let lease = self.runtime.acquire_session(id)?;
        self.load_after_lease(id, lease, true)
    }

    /// Continue the most recent inactive session opened from the current directory.
    ///
    /// # Errors
    ///
    /// Returns a typed search, lease, or not-found failure.
    pub fn continue_current(
        &mut self,
    ) -> Result<LeasedSession<R::SessionLease>, SessionServiceError> {
        let hits = self.list_sessions(None, false)?;
        let cwd = self.cwd.clone();
        for hit in hits.into_iter().filter(|hit| hit.last_opened_cwd == cwd) {
            match self.resume(hit.id) {
                Ok(session) => return Ok(session),
                Err(SessionServiceError::Runtime(RuntimeError::SessionBusy { .. })) => {}
                Err(error) => return Err(error),
            }
        }
        Err(SessionServiceError::SessionNotFound(
            self.cwd.display().to_string(),
        ))
    }

    /// Search sessions with current-directory ranking.
    ///
    /// # Errors
    ///
    /// Returns a typed persistence failure.
    pub fn list_sessions(
        &mut self,
        text: Option<String>,
        include_trashed: bool,
    ) -> Result<Vec<SessionHit>, SessionServiceError> {
        Ok(self.store.search_sessions(&SessionQuery {
            text,
            include_trashed,
            current_directory: Some(self.cwd.clone()),
        })?)
    }

    /// Load one session without acquiring an editing lease.
    ///
    /// # Errors
    ///
    /// Returns a typed absence, corruption, or persistence failure.
    pub fn inspect_session(
        &mut self,
        id: SessionId,
    ) -> Result<SessionSnapshot, SessionServiceError> {
        Ok(self.store.load_session(id)?)
    }

    /// Resolve either a canonical session identifier or an exact session name.
    ///
    /// # Errors
    ///
    /// Returns invalid-identifier, not-found, ambiguous-name, or storage failure.
    pub fn resolve_session(
        &mut self,
        reference: &str,
        include_trashed: bool,
    ) -> Result<SessionId, SessionServiceError> {
        if looks_like_typed_id(reference) {
            return SessionId::from_str(reference).map_err(|error| {
                SessionServiceError::InvalidIdentifier {
                    value: reference.to_owned(),
                    reason: error.to_string(),
                }
            });
        }
        let matches: Vec<_> = self
            .list_sessions(None, include_trashed)?
            .into_iter()
            .filter(|hit| hit.name.as_deref() == Some(reference))
            .map(|hit| hit.id)
            .collect();
        match matches.as_slice() {
            [id] => Ok(*id),
            [] => Err(SessionServiceError::SessionNotFound(reference.to_owned())),
            _ => Err(SessionServiceError::AmbiguousSession {
                reference: reference.to_owned(),
                matches,
            }),
        }
    }

    /// Rename or clear one session while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, validation, absence, or persistence failure.
    pub fn rename_session(
        &mut self,
        id: SessionId,
        name: Option<&str>,
    ) -> Result<(), SessionServiceError> {
        let _lease = self.runtime.acquire_session(id)?;
        self.store.rename_session(id, name)?;
        Ok(())
    }

    /// Move one session to recoverable trash while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, absence, or persistence failure.
    pub fn trash_session(&mut self, id: SessionId) -> Result<(), SessionServiceError> {
        let _lease = self.runtime.acquire_session(id)?;
        self.store.trash_session(id, self.clock.now())?;
        Ok(())
    }

    /// Restore one recoverably trashed session while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, absence, or persistence failure.
    pub fn restore_session(&mut self, id: SessionId) -> Result<(), SessionServiceError> {
        let _lease = self.runtime.acquire_session(id)?;
        self.store.restore_session(id)?;
        Ok(())
    }

    /// Permanently prune one trashed session while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed lease, precondition, absence, or persistence failure.
    pub fn prune_session(&mut self, id: SessionId) -> Result<(), SessionServiceError> {
        let _lease = self.runtime.acquire_session(id)?;
        self.store.prune_session(id)?;
        Ok(())
    }

    fn load_after_lease(
        &mut self,
        id: SessionId,
        lease: R::SessionLease,
        resuming: bool,
    ) -> Result<LeasedSession<R::SessionLease>, SessionServiceError> {
        if resuming {
            self.store.compact_session(id)?;
        }
        let snapshot = self.store.load_session(id)?;
        if snapshot.board.session.deleted_at.is_some() {
            return Err(SessionServiceError::SessionTrashed(id));
        }
        if resuming {
            self.store
                .record_session_open(id, &self.cwd, self.clock.now())?;
        }
        let snapshot = if resuming {
            self.store.load_session(id)?
        } else {
            snapshot
        };
        Ok(LeasedSession {
            state: AppState::from_snapshot(snapshot)?,
            lease,
        })
    }
}

fn looks_like_typed_id(value: &str) -> bool {
    value.split_once('_').is_some_and(|(prefix, _)| {
        matches!(prefix, "ses" | "tht" | "rev" | "op" | "ins" | "req" | "sub")
    })
}
