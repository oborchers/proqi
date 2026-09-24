//! Destination discovery and durable cross-session delivery.

#[cfg(test)]
#[path = "transfer/tests.rs"]
mod tests;

use std::path::PathBuf;

use crate::{
    adapters::{
        control::CancellableLocalControlClient,
        process::CancellationFlag,
        runtime::{
            FileRuntimeCoordinator, SystemClock, SystemIdGenerator, TransientSessionCoordinator,
        },
        sqlite::SqliteStore,
    },
    application::SessionService,
    ports::{
        control::{ControlClient, ControlMutation, ControlRequest},
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, SessionHit, SessionQuery, Store, StoreError},
        transfer::SessionTransferBatchRequest,
    },
};

pub(super) struct TransferRuntime {
    coordinator: FileRuntimeCoordinator,
    cwd: PathBuf,
    clock: SystemClock,
    ids: SystemIdGenerator,
    client: CancellableLocalControlClient,
}

pub(super) fn deliver_batch(
    store: &mut SqliteStore,
    runtime: &mut TransferRuntime,
    request: &SessionTransferBatchRequest,
) -> Result<CommitReceipt, String> {
    if let Some(receipt) = store
        .prepare_transfer(request, runtime.clock.now())
        .map_err(|error| error.to_string())?
    {
        return Ok(receipt);
    }
    store
        .mark_transfer_sending(request.operation_id)
        .map_err(|error| error.to_string())?;
    let receipt = if let Some(owner) = runtime
        .coordinator
        .active_instances()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.session_id == request.destination_session_id)
    {
        let mutation = ControlMutation::PreserveAddMany {
            operation_id: request.operation_id,
            items: request.items.clone(),
        };
        let protocol = owner
            .control_protocol
            .ok_or_else(|| "destination owner does not advertise control forwarding".to_owned())?;
        if protocol < mutation.minimum_protocol()
            || protocol > crate::ports::control::CONTROL_PROTOCOL_VERSION
        {
            return Err("destination owner does not support selected transfer".to_owned());
        }
        runtime
            .client
            .send(
                &owner,
                &ControlRequest {
                    protocol,
                    request_id: runtime.ids.request_id(),
                    session_id: request.destination_session_id,
                    mutation,
                },
            )
            .map_err(|error| error.to_string())?
            .durable
    } else {
        let destination_owner = runtime.transient_destination_owner();
        let mut service = SessionService::new(
            store,
            &destination_owner,
            &runtime.clock,
            &mut runtime.ids,
            runtime.cwd.clone(),
        )
        .map_err(|error| error.to_string())?;
        service
            .preserve_thoughts(
                request.destination_session_id,
                request.operation_id,
                request.items.clone(),
            )
            .map_err(|error| error.to_string())?
            .receipt
    };
    store
        .accept_transfer(request, receipt)
        .map_err(|error| error.to_string())?;
    Ok(receipt)
}

impl TransferRuntime {
    pub(super) const fn new(
        coordinator: FileRuntimeCoordinator,
        cwd: PathBuf,
        cancellation: CancellationFlag,
    ) -> Self {
        Self {
            coordinator,
            cwd,
            clock: SystemClock,
            ids: SystemIdGenerator,
            client: CancellableLocalControlClient::new(cancellation),
        }
    }

    fn transient_destination_owner(&mut self) -> TransientSessionCoordinator {
        self.coordinator
            .transient_session_owner(self.ids.instance_id(), self.clock.now())
    }
}

pub(super) fn discover(
    store: &mut SqliteStore,
    current_session_id: crate::domain::SessionId,
) -> Result<Vec<SessionHit>, StoreError> {
    store
        .search_sessions(&SessionQuery {
            text: None,
            include_trashed: false,
            current_directory: None,
        })
        .map(|hits| {
            hits.into_iter()
                .filter(|hit| hit.id != current_session_id && !hit.trashed)
                .collect()
        })
}

use super::{PersistenceResult, RetainedCommit, fail_first_test_commit, retention};
use crate::domain::{BoardOperation, OperationSequence};
use std::{collections::BTreeMap, sync::mpsc::SyncSender};

pub(super) fn commit_transfer_removal(
    store: &mut SqliteStore,
    request: Box<crate::ports::transfer::SessionTransferBatchRequest>,
    removal: Box<BoardOperation>,
    reason: &'static str,
    retained: &mut BTreeMap<OperationSequence, RetainedCommit>,
    results: &SyncSender<PersistenceResult>,
    retried: bool,
) -> bool {
    let sequence = removal.sequence;
    let operation_id = request.operation_id;
    let commit = RetainedCommit::TransferRemoval {
        request,
        removal,
        reason,
    };
    let RetainedCommit::TransferRemoval {
        request,
        removal,
        reason,
    } = &commit
    else {
        return false;
    };
    let result = fail_first_test_commit().map_or_else(
        || store.finish_transfer(request, Some(removal), reason),
        Err,
    );
    let result = if result.is_err() && !retention::can_retain(retained, sequence, &commit) {
        Err(StoreError::RecoveryCapacity)
    } else {
        result
    };
    if result.is_ok() {
        retained.remove(&sequence);
    } else if !matches!(result, Err(StoreError::RecoveryCapacity)) {
        retained.insert(sequence, commit);
    }
    results
        .send(PersistenceResult::TransferFinished {
            operation_id,
            sequence: Some(sequence),
            result,
            retried,
        })
        .is_ok()
}
