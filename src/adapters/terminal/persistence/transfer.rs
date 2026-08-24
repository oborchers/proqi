//! Destination discovery and durable cross-session delivery.

use std::path::PathBuf;

use crate::{
    adapters::{
        control::LocalControlClient,
        runtime::{FileRuntimeCoordinator, SystemClock, SystemIdGenerator},
        sqlite::SqliteStore,
    },
    application::{SessionService, ThoughtMutation},
    domain::ThoughtId,
    ports::{
        control::{ControlClient, ControlMutation, ControlRequest},
        environment::IdGenerator,
        runtime::RuntimeCoordinator,
        store::{SessionHit, SessionQuery, Store, StoreError},
        transfer::SessionTransferRequest,
    },
};

pub(super) struct TransferRuntime {
    coordinator: FileRuntimeCoordinator,
    cwd: PathBuf,
    clock: SystemClock,
    ids: SystemIdGenerator,
}

impl TransferRuntime {
    pub(super) const fn new(coordinator: FileRuntimeCoordinator, cwd: PathBuf) -> Self {
        Self {
            coordinator,
            cwd,
            clock: SystemClock,
            ids: SystemIdGenerator,
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
        .add_thought_annotated(
            request.destination_session_id,
            request.content.clone(),
            request.annotations.clone(),
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
    if !(crate::ports::control::MIN_CONTROL_PROTOCOL_VERSION
        ..=crate::ports::control::CONTROL_PROTOCOL_VERSION)
        .contains(&protocol)
        || !request.annotations.is_empty() && protocol < 2
    {
        return Err("destination owner does not support annotation-aware transfer".to_owned());
    }
    let thought_id = ThoughtId::from_database_bytes(request.operation_id.database_bytes())
        .map_err(|error| error.to_string())?;
    let receipt = LocalControlClient
        .send(
            owner,
            &ControlRequest {
                protocol,
                request_id: runtime.ids.request_id(),
                session_id: request.destination_session_id,
                mutation: ControlMutation::Add {
                    operation_id: request.operation_id,
                    thought_id,
                    content: request.content.clone(),
                    annotations: request.annotations.clone(),
                    position: None,
                },
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(ThoughtMutation {
        thought_id,
        receipt: receipt.durable,
    })
}
