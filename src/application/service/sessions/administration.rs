//! Retry-safe session rename, trash, restore, prune, and Browser history movement.

use crate::{
    domain::{
        BrowserOperation, BrowserOperationKind, OperationId, SessionId, validate_session_name,
    },
    ports::{
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{
            BrowserCommitReceipt, BrowserHistoryEntry, SessionRequest, SessionRequestReceipt,
            Store, StoredSessionRequest,
        },
    },
};

use super::super::{SessionService, SessionServiceError};

/// Durable result of one session-administration request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionAdministrationReceipt {
    /// Addressed session.
    pub session_id: SessionId,
    /// Operation identity that now owns the request.
    pub operation_id: OperationId,
    /// Whether this exact request had already committed.
    pub idempotent_replay: bool,
    /// Whether this call changed durable session state.
    pub changed: bool,
}

/// Durable result of one Browser history movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserHistoryMovement {
    /// Operation identity that owns the movement.
    pub operation_id: OperationId,
    /// Browser history cursor after the movement.
    pub cursor: usize,
    /// Whether this exact movement had already committed.
    pub idempotent_replay: bool,
    /// Moved Browser operation kind, when it is still retained.
    pub target: Option<BrowserOperationKind>,
}

impl<S, R, C, I> SessionService<'_, S, R, C, I>
where
    S: Store,
    R: RuntimeCoordinator,
    C: Clock,
    I: IdGenerator,
{
    /// Find the retained request that owns one caller-supplied identity.
    ///
    /// # Errors
    ///
    /// Returns [`SessionServiceError::IdempotencyConflict`] when the identity
    /// belongs to another request, or a typed storage failure.
    pub fn session_request_replay(
        &mut self,
        operation_id: OperationId,
        request: &SessionRequest,
    ) -> Result<Option<SessionRequestReceipt>, SessionServiceError> {
        match self.store.session_request(operation_id)? {
            None => Ok(None),
            Some(StoredSessionRequest::Administration(stored)) if stored.request == *request => {
                Ok(Some(stored))
            }
            Some(_) => Err(SessionServiceError::IdempotencyConflict),
        }
    }

    /// Rename or clear one inactive session while holding its lease.
    ///
    /// A request whose name already matches still reserves its operation identity.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, idempotency, lease, absence, or persistence failure.
    pub fn rename_session(
        &mut self,
        id: SessionId,
        name: Option<&str>,
        supplied: Option<OperationId>,
    ) -> Result<SessionAdministrationReceipt, SessionServiceError> {
        if let Some(name) = name {
            validate_session_name(name)?;
        }
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let replacement = name.map(str::to_owned);
        let request = SessionRequest::Rename {
            session_id: id,
            name: replacement.clone(),
        };
        let _lease = match self.leased_replay(id, operation_id, &request)? {
            Leased::Replayed(receipt) => return Ok(receipt),
            Leased::Held(lease) => lease,
        };
        let current = self.store.load_session(id)?.board.session.name;
        let now = self.clock.now();
        if current == replacement {
            let receipt = self
                .store
                .commit_browser_noop_rename(operation_id, id, name, now)?;
            return Ok(administration_receipt(id, receipt, false));
        }
        let operation = BrowserOperation::rename(operation_id, id, current, replacement, now)?;
        let receipt = self.store.commit_browser_operation(&operation)?;
        Ok(administration_receipt(
            id,
            receipt,
            !receipt.idempotent_replay,
        ))
    }

    /// Move one session to recoverable trash, or confirm that it is already there.
    ///
    /// # Errors
    ///
    /// Returns a typed idempotency, lease, absence, or persistence failure.
    pub fn trash_session(
        &mut self,
        id: SessionId,
        supplied: Option<OperationId>,
    ) -> Result<SessionAdministrationReceipt, SessionServiceError> {
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let request = SessionRequest::Trash { session_id: id };
        let _lease = match self.leased_replay(id, operation_id, &request)? {
            Leased::Replayed(receipt) => return Ok(receipt),
            Leased::Held(lease) => lease,
        };
        let session = self.store.load_session(id)?.board.session;
        let now = self.clock.now();
        if session.deleted_at.is_some() {
            let receipt = self.store.commit_noop_trash(operation_id, id, now)?;
            return Ok(administration_receipt(id, receipt, false));
        }
        let operation = BrowserOperation::trash(operation_id, id, session.last_active_at, now);
        let receipt = self.store.commit_browser_operation(&operation)?;
        Ok(administration_receipt(
            id,
            receipt,
            !receipt.idempotent_replay,
        ))
    }

    /// Restore one recoverably trashed session while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed idempotency, lease, trash-state, absence, or persistence failure.
    pub fn restore_session(
        &mut self,
        id: SessionId,
        supplied: Option<OperationId>,
    ) -> Result<SessionAdministrationReceipt, SessionServiceError> {
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let request = SessionRequest::Restore { session_id: id };
        let _lease = match self.leased_replay(id, operation_id, &request)? {
            Leased::Replayed(receipt) => return Ok(receipt),
            Leased::Held(lease) => lease,
        };
        let session = self.store.load_session(id)?.board.session;
        let deleted_at = session
            .deleted_at
            .ok_or(SessionServiceError::SessionNotTrashed(id))?;
        let operation = BrowserOperation::restore(
            operation_id,
            id,
            deleted_at,
            session.last_active_at,
            self.clock.now(),
        );
        let receipt = self.store.commit_browser_operation(&operation)?;
        Ok(administration_receipt(
            id,
            receipt,
            !receipt.idempotent_replay,
        ))
    }

    /// Permanently prune one trashed session while holding its lease.
    ///
    /// # Errors
    ///
    /// Returns a typed idempotency, lease, trash-state, absence, or persistence failure.
    pub fn prune_session(
        &mut self,
        id: SessionId,
        supplied: Option<OperationId>,
    ) -> Result<SessionAdministrationReceipt, SessionServiceError> {
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let request = SessionRequest::Prune { session_id: id };
        let _lease = match self.leased_replay(id, operation_id, &request)? {
            Leased::Replayed(receipt) => return Ok(receipt),
            Leased::Held(lease) => lease,
        };
        if self
            .store
            .load_session(id)?
            .board
            .session
            .deleted_at
            .is_none()
        {
            return Err(SessionServiceError::SessionNotTrashed(id));
        }
        let receipt = self
            .store
            .prune_session_request(id, operation_id, self.clock.now())?;
        Ok(administration_receipt(
            id,
            receipt,
            !receipt.idempotent_replay,
        ))
    }

    /// Undo or redo the current installation-wide Browser history entry.
    ///
    /// # Errors
    ///
    /// Returns a typed idempotency, empty-history, stale-history, lease, or
    /// persistence failure.
    pub fn move_browser_history(
        &mut self,
        undo: bool,
        supplied: Option<OperationId>,
    ) -> Result<BrowserHistoryMovement, SessionServiceError> {
        self.move_history_entry(None, undo, supplied)
    }

    /// Move the exact Browser entry presented to an interactive owner.
    ///
    /// # Errors
    ///
    /// Returns a typed stale-history, active-session, or persistence failure.
    pub fn move_presented_browser_history(
        &mut self,
        target: BrowserHistoryEntry,
        undo: bool,
    ) -> Result<BrowserHistoryMovement, SessionServiceError> {
        self.move_history_entry(Some(target), undo, None)
    }

    fn move_history_entry(
        &mut self,
        presented: Option<BrowserHistoryEntry>,
        undo: bool,
        supplied: Option<OperationId>,
    ) -> Result<BrowserHistoryMovement, SessionServiceError> {
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let request = SessionRequest::History { undo };
        if let Some(stored) = self.session_request_replay(operation_id, &request)? {
            return Ok(replayed_movement(&stored));
        }
        let status = self.store.browser_history_status()?;
        let target = if undo { status.undo } else { status.redo }
            .ok_or(SessionServiceError::NoBrowserHistory { undo })?;
        if presented.is_some_and(|presented| presented != target) {
            return Err(crate::ports::store::StoreError::Conflict(
                "Browser history changed while the action was visible".to_owned(),
            )
            .into());
        }
        let _lease = self.runtime.acquire_session(target.session_id)?;
        if let Some(stored) = self.session_request_replay(operation_id, &request)? {
            return Ok(replayed_movement(&stored));
        }
        let receipt =
            self.store
                .move_browser_history(operation_id, target, undo, self.clock.now())?;
        Ok(BrowserHistoryMovement {
            operation_id,
            cursor: receipt.cursor,
            idempotent_replay: receipt.idempotent_replay,
            target: Some(target.kind),
        })
    }

    /// Replay before and after acquiring the addressed session lease.
    ///
    /// A new request returns the held lease, which the caller must keep alive
    /// until its mutation is durable.
    fn leased_replay(
        &mut self,
        id: SessionId,
        operation_id: OperationId,
        request: &SessionRequest,
    ) -> Result<Leased<R::SessionLease>, SessionServiceError> {
        if self
            .session_request_replay(operation_id, request)?
            .is_some()
        {
            return Ok(Leased::Replayed(replayed_receipt(id, operation_id)));
        }
        let lease = self.runtime.acquire_session(id)?;
        if self
            .session_request_replay(operation_id, request)?
            .is_some()
        {
            return Ok(Leased::Replayed(replayed_receipt(id, operation_id)));
        }
        Ok(Leased::Held(lease))
    }
}

/// Either an exact earlier result or the lease that protects a new mutation.
enum Leased<L> {
    /// The operation identity already committed this exact request.
    Replayed(SessionAdministrationReceipt),
    /// The session lease held for the new mutation.
    Held(L),
}

const fn administration_receipt(
    session_id: SessionId,
    receipt: BrowserCommitReceipt,
    changed: bool,
) -> SessionAdministrationReceipt {
    SessionAdministrationReceipt {
        session_id,
        operation_id: receipt.operation_id,
        idempotent_replay: receipt.idempotent_replay,
        changed,
    }
}

const fn replayed_receipt(
    session_id: SessionId,
    operation_id: OperationId,
) -> SessionAdministrationReceipt {
    SessionAdministrationReceipt {
        session_id,
        operation_id,
        idempotent_replay: true,
        changed: false,
    }
}

const fn replayed_movement(stored: &SessionRequestReceipt) -> BrowserHistoryMovement {
    BrowserHistoryMovement {
        operation_id: stored.operation_id,
        cursor: stored.cursor,
        idempotent_replay: true,
        target: stored.history_target,
    }
}

#[cfg(test)]
mod tests;
