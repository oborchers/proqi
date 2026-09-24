//! Destination discovery and durable cross-session delivery.

use std::path::PathBuf;

use crate::{
    adapters::{
        control::CancellableLocalControlClient,
        process::CancellationFlag,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        sqlite::SqliteStore,
    },
    application::{SessionService, ThoughtMutation},
    domain::ThoughtId,
    ports::{
        control::{ControlClient, ControlMutation, ControlRequest},
        environment::{Clock, IdGenerator},
        runtime::RuntimeCoordinator,
        store::{CommitReceipt, SessionHit, SessionQuery, Store, StoreError},
        transfer::{SessionTransferBatchRequest, SessionTransferRequest},
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
        let mut service = SessionService::new(
            store,
            &runtime.coordinator,
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

pub(super) fn deliver(
    store: &mut SqliteStore,
    runtime: &mut TransferRuntime,
    request: &SessionTransferRequest,
) -> Result<ThoughtMutation, String> {
    if let Some(owner) = runtime
        .coordinator
        .active_instances()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|instance| instance.session_id == request.destination_session_id)
    {
        return forward(runtime, request, &owner);
    }
    let mut service = SessionService::new(
        store,
        &runtime.coordinator,
        &runtime.clock,
        &mut runtime.ids,
        runtime.cwd.clone(),
    )
    .map_err(|error| error.to_string())?;
    service
        .preserve_thought(
            request.destination_session_id,
            request.content.clone(),
            request.annotations.clone(),
            request.name.clone(),
            None,
            Some(request.operation_id),
        )
        .map_err(|error| error.to_string())
}

fn forward(
    runtime: &mut TransferRuntime,
    request: &SessionTransferRequest,
    owner: &crate::ports::runtime::InstanceInfo,
) -> Result<ThoughtMutation, String> {
    let protocol = owner
        .control_protocol
        .ok_or_else(|| "destination owner does not advertise control forwarding".to_owned())?;
    let thought_id = ThoughtId::from_database_bytes(request.operation_id.database_bytes())
        .map_err(|error| error.to_string())?;
    let mutation = ControlMutation::PreserveAdd {
        operation_id: request.operation_id,
        thought_id,
        content: request.content.clone(),
        annotations: request.annotations.clone(),
        name: request.name.clone(),
        position: None,
    };
    if !(crate::ports::control::MIN_CONTROL_PROTOCOL_VERSION
        ..=crate::ports::control::CONTROL_PROTOCOL_VERSION)
        .contains(&protocol)
        || protocol < mutation.minimum_protocol()
    {
        return Err("destination owner does not support annotation-aware transfer".to_owned());
    }
    let receipt = runtime
        .client
        .send(
            owner,
            &ControlRequest {
                protocol,
                request_id: runtime.ids.request_id(),
                session_id: request.destination_session_id,
                mutation,
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(ThoughtMutation {
        thought_id,
        receipt: receipt.durable,
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
