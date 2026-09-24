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
            Store, StoreError, StoredSessionRequest,
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

    /// Resolve the session addressed by a retry-safe administration request.
    ///
    /// A retry may use a name that its own earlier commit changed, removed, or
    /// pruned. When the name no longer resolves, or resolves ambiguously among
    /// sessions that include the one recorded for the caller's operation
    /// identity, the recorded session is used and the ordinary replay
    /// comparison decides whether the retry is exact. When the name currently
    /// resolves to another session, the identity cannot describe this request,
    /// so it fails instead of acting on the recorded session. A typed session
    /// identifier always addresses exactly that session.
    ///
    /// # Errors
    ///
    /// Returns an idempotency conflict, identifier, lookup, or resolution failure.
    pub fn resolve_session_for_request(
        &mut self,
        reference: &str,
        supplied: Option<OperationId>,
    ) -> Result<SessionId, SessionServiceError> {
        if super::looks_like_typed_id(reference) {
            return self.resolve_session(reference, true);
        }
        let recorded = match supplied {
            Some(operation_id) => match self.store.session_request(operation_id)? {
                Some(StoredSessionRequest::Administration(stored)) => stored.request.session_id(),
                _ => None,
            },
            None => None,
        };
        match (self.resolve_session(reference, true), recorded) {
            (Ok(resolved), Some(recorded)) if resolved != recorded => {
                Err(SessionServiceError::IdempotencyConflict)
            }
            (Err(SessionServiceError::AmbiguousSession { ref matches, .. }), Some(recorded))
                if matches.contains(&recorded) =>
            {
                Ok(recorded)
            }
            (Err(SessionServiceError::AmbiguousSession { .. }), Some(_)) => {
                Err(SessionServiceError::IdempotencyConflict)
            }
            (Err(SessionServiceError::SessionNotFound(_)), Some(recorded)) => Ok(recorded),
            (resolved, _) => resolved,
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
            let result = self
                .store
                .commit_browser_noop_rename(operation_id, id, name, now);
            return self.settle(id, operation_id, &request, result, false);
        }
        let operation = BrowserOperation::rename(operation_id, id, current, replacement, now)?;
        let result = self.store.commit_browser_operation(&operation);
        self.settle(id, operation_id, &request, result, true)
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
            let result = self.store.commit_noop_trash(operation_id, id, now);
            return self.settle(id, operation_id, &request, result, false);
        }
        let operation = BrowserOperation::trash(operation_id, id, session.last_active_at, now);
        let result = self.store.commit_browser_operation(&operation);
        self.settle(id, operation_id, &request, result, true)
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
        let result = self.store.commit_browser_operation(&operation);
        self.settle(id, operation_id, &request, result, true)
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
        let result = self
            .store
            .prune_session_request(id, operation_id, self.clock.now());
        self.settle(id, operation_id, &request, result, true)
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
            match self
                .store
                .move_browser_history(operation_id, target, undo, self.clock.now())
            {
                Ok(receipt) => receipt,
                Err(StoreError::Conflict(message)) => {
                    return match self.session_request_replay(operation_id, &request)? {
                        Some(stored) => Ok(replayed_movement(&stored)),
                        None => Err(StoreError::Conflict(message).into()),
                    };
                }
                Err(error) => return Err(error.into()),
            };
        Ok(BrowserHistoryMovement {
            operation_id,
            cursor: receipt.cursor,
            idempotent_replay: receipt.idempotent_replay,
            target: Some(target.kind),
        })
    }

    /// Decide how a rename that an active owner may apply is reported.
    ///
    /// An exact earlier request replays without forwarding. Otherwise the
    /// returned identity is forwarded or applied, and `changed` compares the
    /// requested name with the durable name observed before forwarding.
    ///
    /// # Errors
    ///
    /// Returns a typed validation, idempotency, absence, or storage failure.
    pub fn admit_rename(
        &mut self,
        id: SessionId,
        name: Option<&str>,
        supplied: Option<OperationId>,
    ) -> Result<RenameAdmission, SessionServiceError> {
        if let Some(name) = name {
            validate_session_name(name)?;
        }
        let operation_id = supplied.unwrap_or_else(|| self.ids.operation_id());
        let request = SessionRequest::Rename {
            session_id: id,
            name: name.map(str::to_owned),
        };
        if self
            .session_request_replay(operation_id, &request)?
            .is_some()
        {
            return Ok(RenameAdmission::Replayed(replayed_receipt(
                id,
                operation_id,
            )));
        }
        let current = self.store.load_session(id)?.board.session.name;
        Ok(RenameAdmission::New(SessionAdministrationReceipt {
            session_id: id,
            operation_id,
            idempotent_replay: false,
            changed: current.as_deref() != name,
        }))
    }

    /// Classify a store result, resolving an identity race against its owner.
    ///
    /// Two processes can pass the pre-commit replay check with one identity
    /// before either writes. The loser then sees a storage conflict, which is
    /// either the exact same request or a reused identity.
    fn settle(
        &mut self,
        id: SessionId,
        operation_id: OperationId,
        request: &SessionRequest,
        result: Result<BrowserCommitReceipt, StoreError>,
        changes_state: bool,
    ) -> Result<SessionAdministrationReceipt, SessionServiceError> {
        match result {
            Ok(receipt) => Ok(administration_receipt(
                id,
                receipt,
                changes_state && !receipt.idempotent_replay,
            )),
            Err(StoreError::Conflict(message)) => {
                match self.session_request_replay(operation_id, request)? {
                    Some(_) => Ok(replayed_receipt(id, operation_id)),
                    None => Err(StoreError::Conflict(message).into()),
                }
            }
            Err(error) => Err(error.into()),
        }
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

/// Admission of one session rename that an active owner may apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenameAdmission {
    /// The identity already committed this exact rename.
    Replayed(SessionAdministrationReceipt),
    /// A new request and the receipt reported once an owner applies it.
    New(SessionAdministrationReceipt),
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
