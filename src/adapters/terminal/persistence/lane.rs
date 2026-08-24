//! Bounded persistence-lane lifecycle and request facade.

use super::{
    Duration, FileRuntimeCoordinator, JoinHandle, OperationBatch, OperationId, OperationSequence,
    PersistenceLane, PersistenceRequest, SessionId, SessionTransferRequest, SqliteStore,
    StoredOperationRequest, TerminalError, persistence_loop, sync_channel, thread, transfer,
};

impl PersistenceLane {
    #[cfg(test)]
    pub(in crate::adapters::terminal) fn spawn(store: SqliteStore) -> Self {
        let (request_sender, request_receiver) = sync_channel(64);
        let (result_sender, result_receiver) = sync_channel(64);
        let handle =
            thread::spawn(move || persistence_loop(store, None, &request_receiver, &result_sender));
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
        }
    }

    pub(in crate::adapters::terminal) fn spawn_with_runtime(
        store: SqliteStore,
        coordinator: FileRuntimeCoordinator,
        cwd: std::path::PathBuf,
    ) -> Self {
        let (request_sender, request_receiver) = sync_channel(64);
        let (result_sender, result_receiver) = sync_channel(64);
        let runtime = transfer::TransferRuntime::new(coordinator, cwd);
        let handle = thread::spawn(move || {
            persistence_loop(store, Some(runtime), &request_receiver, &result_sender);
        });
        Self {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: Some(handle),
        }
    }

    pub(in crate::adapters::terminal) fn commit(
        &self,
        batch: OperationBatch,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Commit(Box::new(batch)))
    }

    pub(in crate::adapters::terminal) fn retry(
        &self,
        sequence: OperationSequence,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Retry(sequence))
    }

    pub(in crate::adapters::terminal) fn metadata(
        &self,
        batch: OperationBatch,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Metadata(Box::new(batch)))
    }

    pub(in crate::adapters::terminal) fn rename_session(
        &self,
        session_id: SessionId,
        previous_name: Option<String>,
        name: Option<String>,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::RenameSession {
            session_id,
            previous_name,
            name,
        })
    }

    pub(in crate::adapters::terminal) fn discover_transfer_sessions(
        &self,
        current_session_id: SessionId,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::DiscoverTransferSessions { current_session_id })
    }

    pub(in crate::adapters::terminal) fn transfer_thought(
        &self,
        request: SessionTransferRequest,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::TransferThought(request))
    }

    pub(in crate::adapters::terminal) fn lookup(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<StoredOperationRequest>, TerminalError> {
        let (response, result) = sync_channel(1);
        self.send(PersistenceRequest::Lookup {
            operation_id,
            response,
        })?;
        result
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| TerminalError::Worker("persistence lookup timed out"))?
            .map_err(TerminalError::from)
    }

    fn send(&self, request: PersistenceRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("persistence lane is closed"))?
            .send(request)
            .map_err(|_| TerminalError::Worker("persistence lane disconnected"))
    }

    pub(in crate::adapters::terminal) fn stop(mut self) -> Result<(), TerminalError> {
        drop(self.sender.take());
        match self.handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("persistence lane panicked")),
        }
    }
}
