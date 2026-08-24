//! Bounded persistence-lane lifecycle and request facade.

use super::{
    FileRuntimeCoordinator, JoinHandle, OperationBatch, OperationId, OperationSequence,
    PersistenceLane, PersistenceRequest, SessionId, SessionTransferRequest, SqliteStore,
    TerminalError, persistence_loop, sync_channel, thread, transfer,
};
use crate::domain::RequestId;
use std::sync::mpsc::TrySendError;

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
        request_id: RequestId,
        operation_id: OperationId,
    ) -> Result<(), TerminalError> {
        self.send(PersistenceRequest::Lookup {
            request_id,
            operation_id,
        })
    }

    fn send(&self, request: PersistenceRequest) -> Result<(), TerminalError> {
        self.sender
            .as_ref()
            .ok_or(TerminalError::Worker("persistence lane is closed"))?
            .try_send(request)
            .map_err(|error| match error {
                TrySendError::Full(_) => TerminalError::Worker("persistence lane is full"),
                TrySendError::Disconnected(_) => {
                    TerminalError::Worker("persistence lane disconnected")
                }
            })
    }

    pub(in crate::adapters::terminal) fn stop(self) -> Result<(), TerminalError> {
        let Self {
            sender,
            receiver,
            mut handle,
        } = self;
        drop(sender);
        if handle.is_some() {
            while receiver.recv().is_ok() {}
        }
        match handle.take().map(JoinHandle::join) {
            None | Some(Ok(())) => Ok(()),
            Some(Err(_)) => Err(TerminalError::Worker("persistence lane panicked")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::{memory::FakeIdGenerator, sqlite::StoreConfig},
        application::{Action, AppState, Effect, reduce},
        domain::{Session, SessionBoard, Timestamp},
        ports::{
            environment::IdGenerator as _,
            store::{MigrationMode, Store as _},
        },
    };

    #[test]
    fn full_and_disconnected_request_lanes_fail_without_blocking() {
        let (request_sender, request_receiver) = sync_channel(1);
        let (_result_sender, result_receiver) = sync_channel(1);
        request_sender
            .try_send(PersistenceRequest::Retry(OperationSequence::new(1)))
            .expect("fill request lane");
        let lane = PersistenceLane {
            sender: Some(request_sender),
            receiver: result_receiver,
            handle: None,
        };
        assert!(matches!(
            lane.retry(OperationSequence::new(2)),
            Err(TerminalError::Worker("persistence lane is full"))
        ));
        drop(request_receiver);
        assert!(matches!(
            lane.retry(OperationSequence::new(3)),
            Err(TerminalError::Worker("persistence lane disconnected"))
        ));
        lane.stop().expect("stop detached lane");
    }

    #[test]
    fn stop_drains_accepted_persistence_work_before_joining() {
        let directory = tempfile::tempdir().expect("temporary database");
        let config = StoreConfig::new(
            directory.path().join("proqi.sqlite3"),
            directory.path().join("backups"),
            MigrationMode::Allow,
            Timestamp::from_millis(1),
        );
        let mut ids = FakeIdGenerator::new(1_725_200_000_000);
        let session = Session::new(
            ids.session_id(),
            directory.path().join("workspace"),
            Timestamp::from_millis(2),
        )
        .expect("session");
        let mut setup = SqliteStore::open(&config).expect("setup store");
        setup
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("create session");
        let lane = PersistenceLane::spawn(SqliteStore::open(&config).expect("lane store"));
        let mut state =
            AppState::new(SessionBoard::new(session.clone(), Vec::new()).expect("board"));
        let effects = reduce(
            &mut state,
            Action::CreateThought {
                thought_id: ids.thought_id(),
                operation_id: ids.operation_id(),
                content: "queued before shutdown".to_owned(),
                annotations: Vec::new(),
                insertion_index: None,
                at: Timestamp::from_millis(3),
            },
        )
        .expect("mutation");
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("expected durable board operation");
        };
        lane.commit(OperationBatch::Board(operation.clone()))
            .expect("queue commit");

        lane.stop().expect("drain and stop lane");

        let snapshot = setup.load_session(session.id).expect("durable snapshot");
        assert_eq!(
            snapshot.board.live_thoughts()[0].content,
            "queued before shutdown"
        );
    }
}
